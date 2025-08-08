use super::*;
use crate::prelude::*;
use dc_mini_bsp::Imu;
use dc_mini_icd::ImuConfig;
use embassy_sync::blocking_mutex::raw::RawMutex;
use icm_45605::FifoConfig;

pub async fn apply_imu_config<MutexType: RawMutex>(
    imu: &mut Imu<'_, '_, MutexType>,
    config: &ImuConfig,
) {
    // Configure gyroscope
    unwrap!(
        imu.start_gyro(config.gyro_odr.into(), config.gyro_fsr.into()).await
    );
    // Configure accelerometer
    unwrap!(
        imu.start_accel(config.accel_odr.into(), config.accel_fsr.into())
            .await
    );

    // Configure FIFO if enabled
    if config.fifo_enabled {
        let fifo_config = FifoConfig {
            accel_en: true,
            gyro_en: true,
            temp_en: config.fifo_temp_en,
            hires_en: config.fifo_hires_en,
            watermark: config.fifo_watermark,
            mode: config.fifo_mode.into(),
        };
        unwrap!(imu.configure_fifo(fifo_config).await);
        unwrap!(imu.configure_fifo_interrupt(true).await);
    }

    // Configure motion detection features
    if config.wake_on_motion_enabled {
        unwrap!(
            imu.start_wake_on_motion(config.wake_on_motion_threshold).await
        );
    }

    if config.tap_detection_enabled {
        unwrap!(imu.start_tap_detection().await);
    }

    if config.pedometer_enabled {
        unwrap!(imu.start_pedometer().await);
    }

    if config.tilt_detection_enabled {
        unwrap!(imu.start_tilt_detection().await);
    }
}
