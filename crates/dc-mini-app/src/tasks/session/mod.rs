pub(crate) mod events;
mod tasks;

pub use events::*;
use tasks::*;

use crate::prelude::*;
use embassy_sync::signal::Signal;
use portable_atomic::AtomicBool;

pub(crate) static SESSION_ACTIVE: AtomicBool = AtomicBool::new(false);
pub(self) static SESSION_SIG: Signal<CriticalSectionRawMutex, ()> =
    Signal::new();

pub(self) const MAX_FILENAME_LEN: usize = 12; // For possible date in name

pub(crate) fn is_active() -> bool {
    SESSION_ACTIVE.load(portable_atomic::Ordering::SeqCst)
}
