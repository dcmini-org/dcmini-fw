use super::*;
use crate::prelude::*;
use dc_mini_bsp::ImuResources;
use dc_mini_icd::ImuConfig;
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_futures::select::{select, Either};
use embassy_sync::mutex::Mutex;
use portable_atomic::Ordering;

#[embassy_executor::task]
pub async fn imu_task(
    bus_manager: &'static I2cBusManager,
    imu: &'static Mutex<CriticalSectionRawMutex, ImuResources>,
    config: ImuConfig,
) {
    IMU_MEAS.store(true, Ordering::SeqCst);
    report_status(
        icd::SubsystemId::Imu,
        icd::SubsystemState::Active,
        icd::FaultCode::None,
    )
    .await;

    // Acquire bus handle - configures bus if needed
    let handle = match bus_manager.acquire().await {
        Ok(handle) => handle,
        Err(_) => {
            report_status(
                icd::SubsystemId::Imu,
                icd::SubsystemState::Degraded,
                icd::FaultCode::BusUnavailable,
            )
            .await;
            IMU_MEAS.store(false, Ordering::SeqCst);
            return;
        }
    };

    let mut imu_resources = imu.lock().await;
    let device = I2cDevice::new(handle.bus());
    let mut imu = imu_resources.configure_with_device(device).await;

    // Initialize IMU
    let mut initialized = false;
    for i in 0..5 {
        if imu.init().await.is_ok() {
            initialized = true;
            break;
        } else {
            info!("Retry connection attempt {:?} to IMU...", i);
            Timer::after_millis(1000).await;
        }
    }

    if !initialized {
        report_status(
            icd::SubsystemId::Imu,
            icd::SubsystemState::Degraded,
            icd::FaultCode::ImuInitFailed,
        )
        .await;
        IMU_MEAS.store(false, Ordering::SeqCst);
        return;
    }

    // Apply all configuration settings
    apply_imu_config(&mut imu, &config).await;

    let sender = IMU_DATA_WATCH.sender();

    loop {
        match select(IMU_MEAS_SIG.wait(), async {
            match imu.new_data_ready().await {
                Ok(ready) => {
                    if !ready {
                        return Ok(None);
                    }
                }
                Err(e) => return Err(e),
            }
            let raw = imu.read_6dof().await?;
            Ok(Some(raw))
        })
        .await
        {
            Either::First(config) => {
                if let Some(config) = config {
                    // Stop all features before reconfiguring
                    if imu.stop_accel().await.is_err()
                        || imu.stop_gyro().await.is_err()
                    {
                        report_status(
                            icd::SubsystemId::Imu,
                            icd::SubsystemState::Degraded,
                            icd::FaultCode::ImuInitFailed,
                        )
                        .await;
                        break;
                    }

                    // Flush FIFO if it was enabled
                    if config.fifo_enabled && imu.flush_fifo().await.is_err() {
                        report_status(
                            icd::SubsystemId::Imu,
                            icd::SubsystemState::Degraded,
                            icd::FaultCode::ImuInitFailed,
                        )
                        .await;
                        break;
                    }

                    // Apply new configuration
                    apply_imu_config(&mut imu, &config).await;
                } else {
                    break;
                }
            }
            Either::Second(Ok(data)) => {
                if let Some(data) = data {
                    sender.send(data);
                }
                Timer::after_nanos(config.accel_odr.sleep_duration_ns()).await;
            }
            Either::Second(Err(e)) => {
                error!("Error reading IMU data: {:?}", e);
                report_status(
                    icd::SubsystemId::Imu,
                    icd::SubsystemState::Degraded,
                    icd::FaultCode::ImuInitFailed,
                )
                .await;
                break;
            }
        }
    }

    // Clean up - stop all features
    let _ = imu.stop_accel().await;
    let _ = imu.stop_gyro().await;

    IMU_MEAS_SIG.reset();
    IMU_MEAS.store(false, Ordering::SeqCst);
    report_status(
        icd::SubsystemId::Imu,
        icd::SubsystemState::Ready,
        icd::FaultCode::None,
    )
    .await;

    // Handle and resources drop automatically, managing bus cleanup
}
