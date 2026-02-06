use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::mutex::Mutex;
use grounded::uninit::GroundedCell;
use portable_atomic::{AtomicUsize, Ordering};

use crate::error::BusError;
use crate::factory::BusFactory;
use crate::handle::BusHandle;

/// Phase state machine for the bus lifecycle.
enum Phase<F: BusFactory> {
    /// Bus is not configured; resources are available.
    Idle(F::Resources),
    /// Bus is configured and stored in `bus_cell`.
    Active(F::Destructor),
    /// Unrecoverable error state (should not normally be reached).
    Poisoned,
}

/// Generic bus lifecycle manager.
///
/// Manages the creation, sharing, and teardown of a bus peripheral.
/// The bus is lazily created on first `acquire()` and can be explicitly
/// released with `try_release()` when all handles have been dropped.
pub struct BusManager<M: RawMutex, F: BusFactory> {
    bus_cell: GroundedCell<F::Bus>,
    state: Mutex<M, Phase<F>>,
    users: AtomicUsize,
}

impl<M: RawMutex, F: BusFactory> BusManager<M, F> {
    /// Create a new bus manager with the given resources.
    pub const fn new(resources: F::Resources) -> Self {
        Self {
            bus_cell: GroundedCell::uninit(),
            state: Mutex::new(Phase::Idle(resources)),
            users: AtomicUsize::new(0),
        }
    }

    /// Acquire a handle to the bus.
    ///
    /// If the bus is not yet configured, it will be created via the factory.
    /// The bus will remain configured as long as at least one handle exists
    /// (and until `try_release()` is called after all handles are dropped).
    pub async fn acquire(
        &self,
    ) -> Result<BusHandle<'_, M, F>, BusError<F::Error>> {
        let mut state = self.state.lock().await;

        match &*state {
            Phase::Idle(_) => {
                // Take resources out, replacing with Poisoned temporarily.
                let resources =
                    match core::mem::replace(&mut *state, Phase::Poisoned) {
                        Phase::Idle(r) => r,
                        _ => unreachable!(),
                    };

                match F::create(resources) {
                    Ok((bus, destructor)) => {
                        // SAFETY: We hold the mutex, so no other code can access
                        // bus_cell concurrently. The cell is uninit (Idle state),
                        // so writing is safe.
                        unsafe {
                            self.bus_cell.get().write(bus);
                        }

                        self.users.store(1, Ordering::Release);
                        *state = Phase::Active(destructor);

                        // SAFETY: Just a pointer conversion from MaybeUninit<Bus>
                        // to *const Bus. We just wrote a valid Bus above.
                        let bus_ptr = self.bus_cell.get() as *const F::Bus;
                        Ok(BusHandle::new(bus_ptr, &self.users))
                    }
                    Err((err, resources)) => {
                        // Restore resources so the manager can try again later.
                        *state = Phase::Idle(resources);
                        Err(BusError::FactoryError(err))
                    }
                }
            }
            Phase::Active(_) => {
                self.users.fetch_add(1, Ordering::Acquire);
                let bus_ptr = self.bus_cell.get() as *const F::Bus;
                Ok(BusHandle::new(bus_ptr, &self.users))
            }
            Phase::Poisoned => Err(BusError::Poisoned),
        }
    }

    /// Attempt to release (deconfigure) the bus and recover resources.
    ///
    /// Returns `Ok(())` if the bus was successfully torn down or was already idle.
    /// Returns `Err(InUse(n))` if there are still `n` active handles.
    pub async fn try_release(&self) -> Result<(), BusError<F::Error>> {
        let mut state = self.state.lock().await;

        match &*state {
            Phase::Idle(_) => Ok(()),
            Phase::Active(_) => {
                let n = self.users.load(Ordering::Acquire);
                if n > 0 {
                    return Err(BusError::InUse(n));
                }

                // Take the destructor out, replacing with Poisoned temporarily.
                let destructor =
                    match core::mem::replace(&mut *state, Phase::Poisoned) {
                        Phase::Active(d) => d,
                        _ => unreachable!(),
                    };

                // SAFETY: We hold the mutex and users == 0, so no live BusHandles
                // exist. The bus was written during acquire(), so it is valid.
                unsafe {
                    core::ptr::drop_in_place(
                        self.bus_cell.get() as *mut F::Bus
                    );
                }

                let resources = F::recover(destructor);
                *state = Phase::Idle(resources);

                Ok(())
            }
            Phase::Poisoned => Err(BusError::Poisoned),
        }
    }

    /// Returns the current number of active handles.
    pub fn user_count(&self) -> usize {
        self.users.load(Ordering::Relaxed)
    }

    /// Returns `Some(true)` if active, `Some(false)` if idle, `None` if poisoned.
    ///
    /// This is a non-blocking best-effort check using `try_lock`.
    pub fn is_active(&self) -> Option<bool> {
        self.state
            .try_lock()
            .ok()
            .map(|state| matches!(&*state, Phase::Active(_)))
    }
}
