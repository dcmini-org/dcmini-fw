use super::*;
use crate::prelude::*;
use dc_mini_bsp::ImuResources;
use dc_mini_icd::ImuConfig;
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

    // Acquire bus handle - configures bus if needed
    let handle = bus_manager.acquire().await.unwrap();

    let mut imu_resources = imu.lock().await;
    let device = handle.device();
    let mut imu = imu_resources.configure_with_device(device).await;

    // Initialize IMU
    for i in 0..5 {
        if imu.init().await.is_ok() {
            break;
        } else {
            info!("Retry connection attempt {:?} to IMU...", i);
            Timer::after_millis(1000).await;
        }
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
                    imu.stop_accel().await.unwrap();
                    imu.stop_gyro().await.unwrap();

                    // Flush FIFO if it was enabled
                    if config.fifo_enabled {
                        imu.flush_fifo().await.unwrap();
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
                break;
            }
        }
    }

    // Clean up - stop all features
    imu.stop_accel().await.unwrap();
    imu.stop_gyro().await.unwrap();

    IMU_MEAS_SIG.reset();
    IMU_MEAS.store(false, Ordering::SeqCst);

    // Handle and resources drop automatically, managing bus cleanup
}
