pub(crate) mod events;

mod tasks; // Tasks module is private

pub use events::*;
use tasks::*;

use crate::prelude::*;
use embassy_sync::signal::Signal;
use portable_atomic::AtomicBool;

pub(self) static HAPTIC_ACTIVE: AtomicBool = AtomicBool::new(false);

pub(self) static HAPTIC_CMD_SIG: Signal<
    CriticalSectionRawMutex,
    Option<HapticCommand>,
> = Signal::new();
