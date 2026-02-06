use core::marker::PhantomData;
use core::ops::Deref;

use embassy_sync::blocking_mutex::raw::RawMutex;
use portable_atomic::{AtomicUsize, Ordering};

use crate::factory::BusFactory;

/// RAII handle providing shared access to the bus.
///
/// Dropping a handle decrements the user count atomically. The bus is **not**
/// torn down on drop — call [`BusManager::try_release`](crate::BusManager::try_release)
/// explicitly when the bus should be deconfigured.
pub struct BusHandle<'a, M: RawMutex, F: BusFactory> {
    bus_ptr: *const F::Bus,
    users: &'a AtomicUsize,
    _phantom: PhantomData<(&'a F::Bus, M)>,
}

impl<M: RawMutex, F: BusFactory> Deref for BusHandle<'_, M, F> {
    type Target = F::Bus;

    #[inline]
    fn deref(&self) -> &F::Bus {
        // SAFETY: The handle is alive (caller has `&self`), so users > 0.
        // `BusManager::try_release` refuses to drop the bus while users > 0,
        // so the pointee is valid for the lifetime of `self`.
        unsafe { &*self.bus_ptr }
    }
}

impl<M: RawMutex, F: BusFactory> Drop for BusHandle<'_, M, F> {
    fn drop(&mut self) {
        self.users.fetch_sub(1, Ordering::Release);
    }
}

// SAFETY: A BusHandle is conceptually a shared reference to F::Bus.
// It is Send if F::Bus is Sync (mirroring &T: Send iff T: Sync).
unsafe impl<M: RawMutex, F: BusFactory> Send for BusHandle<'_, M, F> where
    F::Bus: Sync
{
}

// SAFETY: Sharing a &BusHandle across threads is fine if F::Bus is Sync,
// because all access goes through Deref which yields &F::Bus.
unsafe impl<M: RawMutex, F: BusFactory> Sync for BusHandle<'_, M, F> where
    F::Bus: Sync
{
}

impl<'a, M: RawMutex, F: BusFactory> BusHandle<'a, M, F> {
    /// Create a new handle. Only called by `BusManager`.
    pub(crate) fn new(bus_ptr: *const F::Bus, users: &'a AtomicUsize) -> Self {
        Self { bus_ptr, users, _phantom: PhantomData }
    }

    /// Returns a reference to the underlying bus.
    #[inline]
    pub fn bus(&self) -> &F::Bus {
        self
    }
}
