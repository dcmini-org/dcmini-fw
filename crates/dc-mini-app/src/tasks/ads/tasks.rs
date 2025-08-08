use super::*;
use crate::prelude::*;
use dc_mini_icd::AdsConfig;
use embassy_futures::select::{select, Either};
use embassy_nrf::gpio::{Level, Output, OutputDrive};
use embassy_sync::mutex::Mutex;
use embassy_time::Delay;
use portable_atomic::Ordering;

#[embassy_executor::task]
pub async fn ads_pwdn_task(
    ads_resources: &'static Mutex<MutexType, AdsResources>,
) {
    ADS_PWDN.store(true, Ordering::SeqCst);

    let mut ads_resources = ads_resources.lock().await;
    let _pwdn = Output::new(
        ads_resources.pwdn.reborrow(),
        Level::Low,
        OutputDrive::Standard,
    );

    ADS_PWDN_SIG.wait().await;
    ADS_PWDN_SIG.reset();

    ADS_PWDN.store(false, Ordering::SeqCst);
}

#[embassy_executor::task]
pub async fn ads_measure_task(
    bus: &'static Mutex<CriticalSectionRawMutex, Spi3BusResources>,
    ads: &'static Mutex<CriticalSectionRawMutex, AdsResources>,
    config: AdsConfig,
) {
    ADS_MEAS.store(true, Ordering::SeqCst);

    let mut bus_resources = bus.lock().await;
    let bus = bus_resources.get_bus::<CriticalSectionRawMutex>();

    let mut ads_resources = ads.lock().await;
    let mut frontend = ads_resources.configure(&bus).await;

    frontend.reset(&mut Delay).await.unwrap();

    apply_ads_config(&mut frontend, &config).await;

    // Create array mapping channel indices to their power state
    let mut config_idx = 0;
    let mut channel_active = [false; 16]; // Max possible channels across all ADSs
    for ads_dev in frontend.ads.iter() {
        let num_channels = ads_dev.num_chs.unwrap() as usize;
        for i in 0..num_channels {
            channel_active[config_idx + i] =
                !config.channels[config_idx + i].power_down;
        }
        config_idx += num_channels;
    }
    info!("Channel active: {:?}", channel_active);

    frontend.start_stream().await.unwrap();
    let publisher = ADS_MEAS_CH
        .publisher()
        .expect("This is the only expected publisher of ADS data.");

    loop {
        match select(ADS_MEAS_SIG.wait(), frontend.poll()).await {
            Either::First(config) => {
                if let Some(config) = config {
                    frontend
                        .stop_stream()
                        .await
                        .expect("Failed to stop ads stream.");
                    apply_ads_config(&mut frontend, &config).await;

                    // Create array mapping channel indices to their power state
                    let mut config_idx = 0;
                    let mut channel_active = [false; 16]; // Max possible channels across all ADSs
                    for ads_dev in frontend.ads.iter() {
                        let num_channels = ads_dev.num_chs.unwrap() as usize;
                        for i in 0..num_channels {
                            channel_active[config_idx + i] =
                                !config.channels[config_idx + i].power_down;
                        }
                        config_idx += num_channels;
                    }
                    info!("Channel active: {:?}", channel_active);
                    frontend
                        .start_stream()
                        .await
                        .expect("Failed to restart ads stream");
                } else {
                    break;
                }
            }
            Either::Second(ads_data) => {
                let mut ads_data =
                    ads_data.expect("ADS poll resulted in error.");

                let mut config_idx = 0;
                let mut i = 0;
                while i < ads_data.len() {
                    let num_channels = ads_data[i].data.len();
                    let start_idx = config_idx;

                    ads_data[i].data = ads_data[i]
                        .data
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| channel_active[start_idx + i])
                        .map(|(_, &v)| v)
                        .collect();

                    // Remove the ADS device if it has no active channels
                    if ads_data[i].data.is_empty() {
                        let _ = ads_data.remove(i);
                    } else {
                        i += 1;
                    }

                    config_idx += num_channels;
                }

                if let Err(_) = publisher.try_publish(ads_data.into()) {
                    warn!("Failed to publish ads data! Subscriber back pressure!");
                }
            }
        }
    }
    frontend.stop_stream().await.unwrap();
    ADS_MEAS_SIG.reset();

    ADS_MEAS.store(false, Ordering::SeqCst);
}
