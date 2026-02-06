use super::*;
use crate::prelude::*;
use apds9253::Apds9253;
use dc_mini_icd::ApdsConfig;
use embassy_futures::select::{select, Either};
use portable_atomic::Ordering;

#[embassy_executor::task]
pub async fn apds_task(
    bus_manager: &'static I2cBusManager,
    config: ApdsConfig,
) {
    APDS_MEAS.store(true, Ordering::SeqCst);

    // Acquire bus handle - configures bus if needed
    let handle = bus_manager.acquire().await.unwrap();
    let mut sensor = Apds9253::new(handle.device());

    // Initialize sensor with retry loop
    for i in 0..5 {
        if sensor.init_async().await.is_ok() {
            break;
        } else {
            info!("Retry connection attempt {:?} to APDS...", i);
            Timer::after_millis(1000).await;
        }
    }

    // Apply all configuration settings
    apply_apds_config(&mut sensor, &config).await;

    let sender = APDS_DATA_WATCH.sender();
    let poll_delay_ms = sensor.get_measurement_delay_ms() as u64 + 5;

    loop {
        match select(APDS_MEAS_SIG.wait(), async {
            Timer::after_millis(poll_delay_ms).await;

            match sensor.is_data_ready_async().await {
                Ok(true) => {}
                Ok(false) => return Ok(None),
                Err(e) => return Err(e),
            }

            let rgb_data = sensor.read_rgb_data_async().await?;

            let lux = sensor.calculate_lux_async(&rgb_data).await?;

            let color = apds9253::calculate_color_temperature(&rgb_data)
                .unwrap_or(apds9253::ColorData {
                    cct: 0,
                    x: 0.0,
                    y: 0.0,
                });

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
                    apply_apds_config(&mut sensor, &config).await;
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
                break;
            }
        }
    }

    // Clean up - disable sensor
    let _ = sensor.enable_async(false).await;

    APDS_MEAS_SIG.reset();
    APDS_MEAS.store(false, Ordering::SeqCst);
}
