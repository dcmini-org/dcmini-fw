use super::*;
use crate::prelude::*;
use apds9253::Apds9253;
use dc_mini_icd::ApdsConfig;
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_futures::select::{select, Either};
use portable_atomic::Ordering;

#[embassy_executor::task]
pub async fn apds_task(
    bus_manager: &'static I2cBusManager,
    config: ApdsConfig,
) {
    APDS_MEAS.store(true, Ordering::SeqCst);
    report_status(
        icd::SubsystemId::Apds,
        icd::SubsystemState::Active,
        icd::FaultCode::None,
    )
    .await;

    // Acquire bus handle - configures bus if needed
    let handle = match bus_manager.acquire().await {
        Ok(handle) => handle,
        Err(_) => {
            report_status(
                icd::SubsystemId::Apds,
                icd::SubsystemState::Degraded,
                icd::FaultCode::BusUnavailable,
            )
            .await;
            APDS_MEAS.store(false, Ordering::SeqCst);
            return;
        }
    };
    let mut sensor = Apds9253::new(I2cDevice::new(handle.bus()));

    // Initialize sensor with retry loop
    let mut initialized = false;
    for i in 0..5 {
        if sensor.init_async().await.is_ok() {
            initialized = true;
            break;
        } else {
            info!("Retry connection attempt {:?} to APDS...", i);
            Timer::after_millis(1000).await;
        }
    }

    if !initialized {
        report_status(
            icd::SubsystemId::Apds,
            icd::SubsystemState::Degraded,
            icd::FaultCode::ApdsInitFailed,
        )
        .await;
        APDS_MEAS.store(false, Ordering::SeqCst);
        return;
    }

    // Apply all configuration settings
    if !apply_apds_config(&mut sensor, &config).await {
        report_status(
            icd::SubsystemId::Apds,
            icd::SubsystemState::Degraded,
            icd::FaultCode::ApdsInitFailed,
        )
        .await;
        APDS_MEAS.store(false, Ordering::SeqCst);
        return;
    }

    let sender = APDS_DATA_WATCH.sender();
    let poll_delay_ms = sensor.get_measurement_delay_ms() as u64 + 5;

    loop {
        match select(APDS_MEAS_SIG.wait(), async {
            Timer::after_millis(poll_delay_ms).await;

            let rgb_data = match sensor.read_rgb_data_async().await {
                Ok(data) => data,
                Err(apds9253::Error::NotReady) => return Ok(None),
                Err(e) => return Err(e),
            };

            let lux = sensor.calculate_lux_async(&rgb_data).await?;

            let color = apds9253::calculate_color_temperature(&rgb_data)
                .unwrap_or(apds9253::ColorData { cct: 0, x: 0.0, y: 0.0 });

            Ok(Some(ApdsDataFrame {
                red: rgb_data.red,
                green: rgb_data.green,
                blue: rgb_data.blue,
                ir: rgb_data.ir,
                lux,
                cct: color.cct,
                cie_x: color.x,
                cie_y: color.y,
            }))
        })
        .await
        {
            Either::First(config) => {
                if let Some(config) = config {
                    // Disable sensor before reconfiguring
                    let _ = sensor.enable_async(false).await;
                    if !apply_apds_config(&mut sensor, &config).await {
                        report_status(
                            icd::SubsystemId::Apds,
                            icd::SubsystemState::Degraded,
                            icd::FaultCode::ApdsInitFailed,
                        )
                        .await;
                        break;
                    }
                } else {
                    break;
                }
            }
            Either::Second(Ok(data)) => {
                if let Some(data) = data {
                    sender.send(data);
                }
            }
            Either::Second(Err(e)) => {
                error!("Error reading APDS data: {:?}", e);
                report_status(
                    icd::SubsystemId::Apds,
                    icd::SubsystemState::Degraded,
                    icd::FaultCode::ApdsInitFailed,
                )
                .await;
                break;
            }
        }
    }

    // Clean up - disable sensor
    let _ = sensor.enable_async(false).await;

    APDS_MEAS_SIG.reset();
    APDS_MEAS.store(false, Ordering::SeqCst);
    report_status(
        icd::SubsystemId::Apds,
        icd::SubsystemState::Ready,
        icd::FaultCode::None,
    )
    .await;
}
