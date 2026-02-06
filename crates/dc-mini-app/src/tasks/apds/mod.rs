pub(crate) mod config;
pub(crate) mod events;

mod tasks; // Tasks module is private

pub use config::*;
pub use events::*;
use tasks::*;

use crate::prelude::*;
use embassy_sync::signal::Signal;
use embassy_sync::watch::Watch;
use portable_atomic::AtomicBool;

pub(self) static APDS_MEAS: AtomicBool = AtomicBool::new(false);

pub(self) static APDS_MEAS_SIG: Signal<
    CriticalSectionRawMutex,
    Option<ApdsConfig>,
> = Signal::new();

pub const APDS_SUBS: usize = 3;
pub static APDS_WATCH: Watch<CriticalSectionRawMutex, bool, APDS_SUBS> =
    Watch::new();
pub static APDS_DATA_WATCH: Watch<
    CriticalSectionRawMutex,
    ApdsDataFrame,
    APDS_SUBS,
> = Watch::new();
