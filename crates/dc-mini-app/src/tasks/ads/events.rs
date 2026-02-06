use super::*;
use crate::prelude::*;
use derive_more::From;
use embassy_executor::SendSpawner;
use embassy_sync::mutex::Mutex;
use portable_atomic::Ordering;
use tasks::ads_pwdn_task;

#[derive(Debug, From)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum AdsEvent {
    StartStream,
    StopStream,
    ResetConfig,
    PrintConfig,
    ConfigChanged,
    ManualRecord,
}

#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum AdsEventError {
    InvalidConversion(u8),
}

impl TryFrom<u8> for AdsEvent {
    type Error = AdsEventError;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(AdsEvent::StartStream),
            1 => Ok(AdsEvent::StopStream),
            2 => Ok(AdsEvent::ResetConfig),
            3 => Ok(AdsEvent::PrintConfig),
            _ => Err(AdsEventError::InvalidConversion(value)),
        }
    }
}

#[derive(Clone)]
pub struct AdsManager {
    bus: &'static Mutex<CriticalSectionRawMutex, Spi3BusResources>,
    ads: &'static Mutex<CriticalSectionRawMutex, AdsResources>,
    app: &'static Mutex<CriticalSectionRawMutex, AppContext>,
}

impl AdsManager {
    pub fn new(
        bus: &'static Mutex<CriticalSectionRawMutex, Spi3BusResources>,
        ads: &'static Mutex<CriticalSectionRawMutex, AdsResources>,
        app: &'static Mutex<CriticalSectionRawMutex, AppContext>,
    ) -> Self {
        Self { bus, ads, app }
    }

    pub async fn get_num_channels(&self) -> u8 {
        let mut bus_resources = self.bus.lock().await;
        let bus = bus_resources.get_bus::<CriticalSectionRawMutex>();

        let mut ads_resources = self.ads.lock().await;
        let mut frontend = ads_resources.configure(&bus).await;

        // We don't need to reset because we have already done that when we configured the frontend
        // above.
        unwrap!(frontend.init().await);

        let mut total_channels: u8 = 0;
        for dev in frontend.ads {
            if dev.num_chs.is_some() {
                total_channels = total_channels + dev.num_chs.unwrap();
            }
        }
        total_channels
    }

    pub fn power_down(&self, spawner: SendSpawner) {
        // Power down the ADS on startup
        spawner.must_spawn(ads_pwdn_task(self.ads));
    }

    pub async fn handle_event(&self, event: AdsEvent) {
        match event {
            AdsEvent::ConfigChanged => {
                // Handle configuration changes
                if ADS_MEAS.load(Ordering::SeqCst) {
                    // We are streaming and need to update the active ADS config.
                    let mut app_ctx = self.app.lock().await;
                    if let Some(ads_config) =
                        app_ctx.profile_manager.get_ads_config().await.cloned()
                    {
                        ADS_MEAS_SIG.signal(Some(ads_config));
                    }
                }
            }
            AdsEvent::StopStream => {
                if ADS_PWDN.load(Ordering::SeqCst) {
                    info!("Tried to power down ADS when it was already powered down.")
                } else {
                    ADS_MEAS_SIG.signal(None);
                    let app_ctx = self.app.lock().await;
                    self.power_down(app_ctx.low_prio_spawner);
                    ADS_WATCH.sender().send(false);
                }
            }
            AdsEvent::StartStream => {
                if ADS_MEAS.load(Ordering::SeqCst) {
                    info!("Tried to start ADS stream while already running.");
                } else {
                    if ADS_PWDN.load(Ordering::SeqCst) {
                        ADS_PWDN_SIG.signal(());
                    }
                    let mut app_ctx = self.app.lock().await;
                    let ads_config = app_ctx
                        .profile_manager
                        .get_ads_config()
                        .await
                        .unwrap()
                        .clone();
                    app_ctx.high_prio_spawner.must_spawn(ads_measure_task(
                        self.bus, self.ads, ads_config,
                    ));
                    ADS_WATCH.sender().send(true);
                };
            }
            AdsEvent::ResetConfig => {
                if ADS_MEAS.load(Ordering::SeqCst) {
                    warn!("Not allowed to reset config while ADS streaming.");
                }

                let mut was_ads_pwdn = false;
                if ADS_PWDN.load(Ordering::SeqCst) {
                    ADS_PWDN_SIG.signal(());
                    was_ads_pwdn = true;
                }

                // Overwrite the current AdsConfig with the default.
                let num_chs = self.get_num_channels().await;
                let config = default_ads_settings(num_chs);
                {
                    let mut context = self.app.lock().await;
                    info!(
                        "Resetting ADS config for profile {:?} to default: {:?}",
                        context.profile_manager.get_current_profile().await,
                        config
                    );
                    context.save_ads_config(config).await;

                    if was_ads_pwdn {
                        self.power_down(context.low_prio_spawner);
                    }
                }

            }
            AdsEvent::PrintConfig => {
                let mut context = self.app.lock().await;
                let config =
                    unwrap!(context.profile_manager.get_ads_config().await);
                info!("PrintConfig Requested: {:?}", config);
            }
            AdsEvent::ManualRecord => {
                let context = self.app.lock().await;
                if ADS_MEAS.load(Ordering::SeqCst) {
                    // Stop Recording.
                    context
                        .event_sender
                        .send(AdsEvent::StopStream.into())
                        .await;
                    Timer::after_millis(500).await;
                    context
                        .event_sender
                        .send(SessionEvent::StopRecording.into())
                        .await;
                    NEOPIX_CHAN.send(NeopixEvent::PowerOn).await;
                } else {
                    // Start Recording.
                    context
                        .event_sender
                        .send(SessionEvent::StartRecording.into())
                        .await;
                    Timer::after_millis(500).await;
                    context
                        .event_sender
                        .send(AdsEvent::StartStream.into())
                        .await;
                    NEOPIX_CHAN.send(NeopixEvent::Recording).await;
                }
            }
        }
    }
}
