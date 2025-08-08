use postcard_schema::Schema;
use serde::{Deserialize, Serialize};

define_config_enum!(
    AccelFsr,
    icm_45605::AccelFsr,
    {
        Fs16G,
        Fs8G,
        Fs4G,
        Fs2G,
    }
);

define_config_enum!(
    AccelOdr,
    icm_45605::AccelOdr,
    {
        Odr64KHz,
        Odr32KHz,
        Odr16KHz,
        Odr800Hz,
        Odr400Hz,
        Odr200Hz,
        Odr100Hz,
        Odr50Hz,
        Odr25Hz,
        Odr125Hz,
        Odr625Hz,
        Odr3125Hz,
        Odr15625Hz,
    }
);

define_config_enum!(
    FifoMode,
    icm_45605::FifoMode,
    {
        Bypass,
        Stream,
        StopOnFull,
    }
);

define_config_enum!(
    GyroFsr,
    icm_45605::GyroFsr,
    {
        Fs2000Dps,
        Fs1000Dps,
        Fs500Dps,
        Fs250Dps,
        Fs125Dps,
        Fs625Dps,
        Fs3125Dps,
        Fs15625Dps,
    }
);

define_config_enum!(
    GyroOdr,
    icm_45605::GyroOdr,
    {
        Odr64KHz,
        Odr32KHz,
        Odr16KHz,
        Odr800Hz,
        Odr400Hz,
        Odr200Hz,
        Odr100Hz,
        Odr50Hz,
        Odr25Hz,
        Odr125Hz,
        Odr625Hz,
        Odr3125Hz,
        Odr15625Hz,
    }
);

impl AccelOdr {
    pub const fn sleep_duration_ns(&self) -> u64 {
        match self {
            AccelOdr::Odr64KHz => 1_000_000_000 / 64_000,
            AccelOdr::Odr32KHz => 1_000_000_000 / 32_000,
            AccelOdr::Odr16KHz => 1_000_000_000 / 16_000,
            AccelOdr::Odr800Hz => 1_000_000_000 / 800,
            AccelOdr::Odr400Hz => 1_000_000_000 / 400,
            AccelOdr::Odr200Hz => 1_000_000_000 / 200,
            AccelOdr::Odr100Hz => 1_000_000_000 / 100,
            AccelOdr::Odr50Hz => 1_000_000_000 / 50,
            AccelOdr::Odr25Hz => 1_000_000_000 / 25,
            AccelOdr::Odr125Hz => 1_000_000_000_000 / 12_500,
            AccelOdr::Odr625Hz => 1_000_000_000_000 / 6_250,
            AccelOdr::Odr3125Hz => 1_000_000_000_000 / 3_125,
            AccelOdr::Odr15625Hz => 1_000_000_000_000 / 1_562,
        }
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Schema, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ImuConfig {
    // Accelerometer settings
    pub accel_odr: AccelOdr,
    pub accel_fsr: AccelFsr,
    pub accel_lpf_enabled: bool,
    pub accel_power_mode: bool, // true for low noise, false for low power

    // Gyroscope settings
    pub gyro_odr: GyroOdr,
    pub gyro_fsr: GyroFsr,
    pub gyro_lpf_enabled: bool,
    pub gyro_power_mode: bool, // true for low noise, false for low power

    // FIFO settings
    pub fifo_enabled: bool,
    pub fifo_mode: FifoMode,
    pub fifo_watermark: u16,
    pub fifo_temp_en: bool, // Include temperature in FIFO
    pub fifo_hires_en: bool, // High resolution mode for FIFO

    // Motion detection features
    pub wake_on_motion_enabled: bool,
    pub wake_on_motion_threshold: u8, // in mg
    pub tap_detection_enabled: bool,
    pub pedometer_enabled: bool,
    pub tilt_detection_enabled: bool,

    // Quaternion/orientation settings
    pub quaternion_enabled: bool,
    pub quaternion_rate: u8, // Update rate in Hz (typical values: 50, 100, 200)
}

impl Default for ImuConfig {
    fn default() -> Self {
        Self {
            // Accelerometer defaults - 100Hz, ±4g, low noise mode
            accel_odr: AccelOdr::Odr200Hz,
            accel_fsr: AccelFsr::Fs8G,
            accel_lpf_enabled: true,
            accel_power_mode: true,

            // Gyroscope defaults - 100Hz, ±2000dps, low noise mode
            gyro_odr: GyroOdr::Odr200Hz,
            gyro_fsr: GyroFsr::Fs250Dps,
            gyro_lpf_enabled: true,
            gyro_power_mode: true,

            // FIFO defaults - enabled, stream mode, 32 samples watermark
            fifo_enabled: false,
            fifo_mode: FifoMode::Stream,
            fifo_watermark: 64,
            fifo_temp_en: false,
            fifo_hires_en: false,

            // Motion detection features - all disabled by default
            wake_on_motion_enabled: false,
            wake_on_motion_threshold: 50, // 50mg default threshold
            tap_detection_enabled: false,
            pedometer_enabled: false,
            tilt_detection_enabled: false,

            // Quaternion disabled by default
            quaternion_enabled: false,
            quaternion_rate: 100, // 100Hz default when enabled
        }
    }
}

pub fn default_imu_settings() -> ImuConfig {
    ImuConfig::default()
}
