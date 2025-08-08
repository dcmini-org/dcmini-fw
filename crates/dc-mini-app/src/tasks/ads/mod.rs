pub mod config;
pub mod events;

mod tasks; // Tasks module is private

pub use config::*;
pub use events::*;
use tasks::*;

use crate::prelude::*;
use ads1299::{self, AdsData};
use alloc::sync::Arc;
use embassy_sync::pubsub::PubSubChannel;
use embassy_sync::signal::Signal;
use embassy_sync::watch::Watch;
use heapless::Vec;
use portable_atomic::AtomicBool;

pub(self) static ADS_PWDN: AtomicBool = AtomicBool::new(false);
pub(self) static ADS_MEAS: AtomicBool = AtomicBool::new(false);

pub(self) static ADS_MEAS_SIG: Signal<
    CriticalSectionRawMutex,
    Option<AdsConfig>,
> = Signal::new();
pub(self) static ADS_PWDN_SIG: Signal<CriticalSectionRawMutex, ()> =
    Signal::new();

pub const ADS_CAP: usize = 100;
pub const ADS_SUBS: usize = 3;
pub type MutexType = CriticalSectionRawMutex;
pub type AdsCh<T> =
    PubSubChannel<CriticalSectionRawMutex, T, ADS_CAP, ADS_SUBS, 1>;
pub static ADS_MEAS_CH: AdsCh<Arc<Vec<ads1299::AdsData, 2>>> = AdsCh::new();
pub static ADS_WATCH: Watch<CriticalSectionRawMutex, bool, ADS_SUBS> =
    Watch::new();

pub(crate) fn convert_to_proto(
    samples: alloc::sync::Arc<Vec<AdsData, 2>>,
) -> icd::proto::AdsSample {
    // Calculate the total number of channels across all ADS devices
    let total_channels: usize =
        samples.iter().map(|sample| sample.data.len()).sum();

    let mut data = alloc::vec::Vec::with_capacity(total_channels);
    let mut lead_off_positive: u32 = 0;
    let mut lead_off_negative: u32 = 0;
    let mut gpio: u32 = 0;

    let mut bit_shift = 0; // Tracks where to place the next lead-off bits
    let mut gpio_shift = 0; // Tracks where to place the next GPIO bits

    for sample in samples.iter() {
        let ch = sample.data.len(); // Number of channels in this ADS device

        // Append channel data
        data.extend(sample.data.iter());

        // Create a bitmask for the number of channels
        let mask = (1 << ch) - 1;

        // Encode lead-off status for positive and negative signals
        lead_off_positive |=
            (sample.lead_off_status_pos.bits() as u32 & mask) << bit_shift;
        lead_off_negative |=
            (sample.lead_off_status_neg.bits() as u32 & mask) << bit_shift;

        // Encode GPIO data (4 bits per ADS device)
        gpio |= (sample.gpio.bits() as u32 & 0x0F) << gpio_shift;

        // Increment shifts for the next ADS device
        bit_shift += ch; // Shift by the number of channels
        gpio_shift += 4; // Shift by 4 bits (1 nibble per GPIO)
    }

    // Return the constructed AdsSample
    let sample = if let Some(current_imu) = IMU_DATA_WATCH.try_get() {
        icd::proto::AdsSample {
            lead_off_positive,
            lead_off_negative,
            gpio,
            data,
            accel_x: Some(current_imu.accel_x),
            accel_y: Some(current_imu.accel_y),
            accel_z: Some(current_imu.accel_z),
            gyro_x: Some(current_imu.gyro_x),
            gyro_y: Some(current_imu.gyro_y),
            gyro_z: Some(current_imu.gyro_z),
        }
    } else {
        icd::proto::AdsSample {
            lead_off_positive,
            lead_off_negative,
            gpio,
            data,
            accel_x: None,
            accel_y: None,
            accel_z: None,
            gyro_x: None,
            gyro_y: None,
            gyro_z: None,
        }
    };
    info!("Converted sample = {}", sample);
    sample
}
