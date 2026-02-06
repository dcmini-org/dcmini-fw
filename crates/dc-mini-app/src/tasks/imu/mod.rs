pub(crate) mod config;
pub(crate) mod events;

#[cfg(not(feature = "r6"))]
mod tasks; // Tasks module is private

pub use config::*;
pub use events::*;
#[cfg(not(feature = "r6"))]
use tasks::*;

use crate::prelude::*;
#[cfg(not(feature = "r6"))]
use embassy_sync::signal::Signal;
use embassy_sync::watch::Watch;
use icm_45605::{self, CalibSensorData};
#[cfg(not(feature = "r6"))]
use portable_atomic::AtomicBool;

#[cfg(not(feature = "r6"))]
pub(self) static IMU_MEAS: AtomicBool = AtomicBool::new(false);

#[cfg(not(feature = "r6"))]
pub(self) static IMU_MEAS_SIG: Signal<
    CriticalSectionRawMutex,
    Option<ImuConfig>,
> = Signal::new();

pub const IMU_CAP: usize = 100;
pub const IMU_SUBS: usize = 3;
pub static IMU_WATCH: Watch<CriticalSectionRawMutex, bool, IMU_SUBS> =
    Watch::new();
pub static IMU_DATA_WATCH: Watch<
    CriticalSectionRawMutex,
    CalibSensorData,
    IMU_SUBS,
> = Watch::new();
