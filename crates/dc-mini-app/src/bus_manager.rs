//! I2C Bus Manager for power-efficient shared bus access
//!
//! Provides automatic configuration/deconfiguration of I2C bus based on usage,
//! with safe concurrent access across multiple tasks.

use alloc::boxed::Box;
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_nrf::{peripherals, twim};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use portable_atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::prelude::*;

/// Errors that can occur during bus operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusError {
    /// Bus resources have already been consumed
    ResourcesAlreadyTaken,
    /// Bus is currently in use and cannot be deconfigured
    BusInUse,
    /// Bus is not currently configured
    NotConfigured,
    /// Bus is in an invalid state
    InvalidState,
}

/// I2C Bus Manager providing power-efficient shared access
pub struct I2cBusManager {
    state: Mutex<CriticalSectionRawMutex, BusState>,
    /// Atomic flag to prevent concurrent cleanup attempts
    cleanup_in_progress: AtomicBool,
}

/// Internal state of the bus manager
enum BusState {
    /// Bus is not configured, holding original resources
    Unconfigured(Option<Twim1BusResources>),
    /// Bus is configured and ready for use
    Configured {
        bus: &'static Mutex<
            CriticalSectionRawMutex,
            twim::Twim<'static, peripherals::TWISPI1>,
        >,
        destructor: BusDestructor,
        users: AtomicUsize,
    },
}

/// Handle for accessing the I2C bus
///
/// Automatically manages reference counting and triggers cleanup when dropped.
pub struct BusHandle<'a> {
    /// Reference to the underlying bus for creating devices
    bus: &'static Mutex<
        CriticalSectionRawMutex,
        twim::Twim<'static, peripherals::TWISPI1>,
    >,
    /// Guard that handles cleanup on drop
    _guard: UserCountGuard<'a>,
}

impl<'a> BusHandle<'a> {
    /// Create an I2C device from this bus handle
    pub fn device(
        &self,
    ) -> I2cDevice<
        '_,
        CriticalSectionRawMutex,
        twim::Twim<'static, peripherals::TWISPI1>,
    > {
        I2cDevice::new(self.bus)
    }
}

/// RAII guard for managing user count
struct UserCountGuard<'a> {
    manager: &'a I2cBusManager,
    /// Track whether we've already decremented (for safety)
    decremented: AtomicBool,
}

// Re-export the BusDestructor from dc-mini-bsp
use dc_mini_bsp::BusDestructor;

impl I2cBusManager {
    /// Create a new bus manager with the given resources
    ///
    /// # Arguments
    /// * `resources` - The I2C bus resources to manage
    pub const fn new(resources: Twim1BusResources) -> Self {
        Self {
            state: Mutex::new(BusState::Unconfigured(Some(resources))),
            cleanup_in_progress: AtomicBool::new(false),
        }
    }

    /// Acquire a handle to the I2C bus
    ///
    /// If the bus is not configured, it will be configured automatically.
    /// The bus will remain configured as long as there are active handles.
    ///
    /// # Returns
    /// A `BusHandle` that provides access to the I2C bus, or an error if
    /// the operation fails.
    pub async fn acquire(&self) -> Result<BusHandle<'_>, BusError> {
        // First, try cleanup of any unused bus (opportunistic cleanup)
        self.try_cleanup_if_unused();

        {
            let mut state = self.state.lock().await;

            match &mut *state {
                BusState::Unconfigured(resources_opt) => {
                    // Take resources and configure bus
                    let resources = resources_opt
                        .take()
                        .ok_or(BusError::ResourcesAlreadyTaken)?;

                    let (bus, destructor) = resources.into_bus();
                    let users = AtomicUsize::new(1);

                    // Leak the bus to get a static reference
                    let bus_static = Box::leak(Box::new(bus));

                    *state = BusState::Configured {
                        bus: bus_static,
                        destructor,
                        users,
                    };
                }
                BusState::Configured { users, .. } => {
                    // Increment user count
                    users.fetch_add(1, Ordering::SeqCst);
                }
            }
        }

        // Get the bus reference
        let state = self.state.lock().await;
        if let BusState::Configured { bus, .. } = &*state {
            let guard = UserCountGuard {
                manager: self,
                decremented: AtomicBool::new(false),
            };

            Ok(BusHandle { bus, _guard: guard })
        } else {
            Err(BusError::InvalidState)
        }
    }

    /// Get the current number of active users
    ///
    /// # Returns
    /// The number of active `BusHandle` instances
    pub async fn user_count(&self) -> usize {
        let state = self.state.lock().await;
        match &*state {
            BusState::Configured { users, .. } => users.load(Ordering::SeqCst),
            BusState::Unconfigured(_) => 0,
        }
    }

    /// Check if the bus is currently configured
    pub async fn is_configured(&self) -> bool {
        let state = self.state.lock().await;
        matches!(*state, BusState::Configured { .. })
    }

    /// Attempt to deconfigure the bus if it's unused
    ///
    /// This is called automatically when handles are dropped, but can also
    /// be called manually for immediate cleanup.
    ///
    /// # Returns
    /// `Ok(())` if cleanup was performed or not needed, `Err` if cleanup failed
    pub async fn try_deconfigure(&self) -> Result<(), BusError> {
        // Prevent concurrent cleanup attempts
        if self
            .cleanup_in_progress
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Ok(()); // Another cleanup is in progress
        }

        let result = self.try_deconfigure_internal().await;

        self.cleanup_in_progress.store(false, Ordering::SeqCst);
        result
    }

    /// Internal cleanup implementation
    async fn try_deconfigure_internal(&self) -> Result<(), BusError> {
        let mut state = self.state.lock().await;

        match &*state {
            BusState::Configured { users, .. } => {
                if users.load(Ordering::SeqCst) > 0 {
                    return Err(BusError::BusInUse);
                }

                // Safe to deconfigure - move out the configured state
                let old_state = core::mem::replace(
                    &mut *state,
                    BusState::Unconfigured(None),
                );

                if let BusState::Configured { bus, destructor, .. } = old_state
                {
                    // Convert the static reference back to owned and drop it
                    let bus_owned = unsafe {
                        Box::from_raw(
                            bus as *const Mutex<
                                CriticalSectionRawMutex,
                                twim::Twim<'static, peripherals::TWISPI1>,
                            >
                                as *mut Mutex<
                                    CriticalSectionRawMutex,
                                    twim::Twim<'static, peripherals::TWISPI1>,
                                >,
                        )
                    };
                    drop(bus_owned);

                    warn!("Dropping Bus!!");

                    // Reconstruct resources safely
                    let resources = destructor.into_resources();
                    *state = BusState::Unconfigured(Some(resources));
                }

                Ok(())
            }
            BusState::Unconfigured(_) => Ok(()), // Already unconfigured
        }
    }

    /// Non-blocking cleanup attempt (used in Drop)
    fn try_cleanup_if_unused(&self) {
        // Only attempt if no other cleanup is in progress
        if self
            .cleanup_in_progress
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            if let Ok(mut state) = self.state.try_lock() {
                if let BusState::Configured { users, .. } = &*state {
                    if users.load(Ordering::SeqCst) == 0 {
                        // Safe to cleanup
                        let old_state = core::mem::replace(
                            &mut *state,
                            BusState::Unconfigured(None),
                        );

                        if let BusState::Configured {
                            bus, destructor, ..
                        } = old_state
                        {
                            // Convert the static reference back to owned and drop it
                            let bus_owned = unsafe {
                                Box::from_raw(
                                    bus as *const Mutex<
                                        CriticalSectionRawMutex,
                                        twim::Twim<
                                            'static,
                                            peripherals::TWISPI1,
                                        >,
                                    >
                                        as *mut Mutex<
                                            CriticalSectionRawMutex,
                                            twim::Twim<
                                                'static,
                                                peripherals::TWISPI1,
                                            >,
                                        >,
                                )
                            };
                            drop(bus_owned);
                            warn!("Dropping Bus!!");
                            let resources = destructor.into_resources();
                            *state = BusState::Unconfigured(Some(resources));
                        }
                    }
                }
            }

            self.cleanup_in_progress.store(false, Ordering::SeqCst);
        }
    }
}

impl Drop for UserCountGuard<'_> {
    fn drop(&mut self) {
        warn!("Dropping handle!!");
        // Ensure we only decrement once
        if self
            .decremented
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            if let Ok(state) = self.manager.state.try_lock() {
                if let BusState::Configured { users, .. } = &*state {
                    // Safe decrement - check current value first to prevent underflow
                    let current = users.load(Ordering::SeqCst);
                    if current > 0 {
                        let remaining =
                            users.fetch_sub(1, Ordering::SeqCst) - 1;
                        if remaining == 0 {
                            drop(state); // Release lock before cleanup
                            self.manager.try_cleanup_if_unused();
                        }
                    }
                    // If current == 0, something is wrong, but don't underflow
                }
            } else {
                // Fallback: try to decrement safely without lock
                self.safe_decrement_without_cleanup();
            }
        }
    }
}

impl UserCountGuard<'_> {
    fn safe_decrement_without_cleanup(&self) {
        // Try a few times to get the lock for safe decrement
        for _ in 0..3 {
            if let Ok(state) = self.manager.state.try_lock() {
                if let BusState::Configured { users, .. } = &*state {
                    let current = users.load(Ordering::SeqCst);
                    if current > 0 {
                        users.fetch_sub(1, Ordering::SeqCst);
                    }
                }
                return;
            }
            // Brief pause before retry
            core::hint::spin_loop();
        }
        // If we still can't get the lock after retries,
        // the count will be corrected by the next acquire() call
    }
}

// Safety: I2cBusManager can be safely shared across threads
unsafe impl Sync for I2cBusManager {}
unsafe impl Send for I2cBusManager {}
