use super::*;
use crate::prelude::*;
use derive_more::From;
use embassy_sync::mutex::Mutex;
use portable_atomic::Ordering;

#[derive(Debug, From)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ApdsEvent {
    StartStream,
    StopStream,
    ResetConfig,
    PrintConfig,
    ConfigChanged,
}

#[derive(Clone)]
pub struct ApdsManager {
    bus_manager: &'static I2cBusManager,
    app: &'static Mutex<CriticalSectionRawMutex, AppContext>,
}

impl ApdsManager {
    pub fn new(
        bus_manager: &'static I2cBusManager,
        app: &'static Mutex<CriticalSectionRawMutex, AppContext>,
    ) -> Self {
        Self { bus_manager, app }
    }

    pub async fn handle_event(&self, event: ApdsEvent) {
        info!("Received event {:?}", event);
        match event {
            ApdsEvent::ConfigChanged => {
                if APDS_MEAS.load(Ordering::SeqCst) {
                    let mut app_ctx = self.app.lock().await;
                    if let Some(apds_config) = app_ctx
                        .profile_manager
                        .get_apds_config()
                        .await
                        .cloned()
                    {
                        APDS_MEAS_SIG.signal(Some(apds_config));
                    }
                }
            }
            ApdsEvent::StopStream => {
                if !APDS_MEAS.load(Ordering::SeqCst) {
                    info!("Tried to stop APDS when it was already stopped.")
                } else {
                    APDS_MEAS_SIG.signal(None);
                    APDS_WATCH.sender().send(false);
                }
            }
            ApdsEvent::StartStream => {
                if APDS_MEAS.load(Ordering::SeqCst) {
                    info!("Tried to start APDS stream while already running.");
                } else {
                    let mut app_ctx = self.app.lock().await;
                    let apds_config = if let Some(config) = app_ctx
                        .profile_manager
                        .get_apds_config()
                        .await
                        .cloned()
                    {
                        config
                    } else {
                        let config = default_apds_settings();
                        app_ctx.save_apds_config(config.clone()).await;
                        report_status(
                            icd::SubsystemId::Apds,
                            icd::SubsystemState::Degraded,
                            icd::FaultCode::ConfigReseeded,
                        )
                        .await;
                        config
                    };
                    app_ctx.low_prio_spawner.must_spawn(apds_task(
                        self.bus_manager,
                        apds_config,
                    ));
                    APDS_WATCH.sender().send(true);
                };
            }
            ApdsEvent::ResetConfig => {
                if APDS_MEAS.load(Ordering::SeqCst) {
                    warn!("Not allowed to reset config while APDS streaming.");
                    return;
                }

                let config = default_apds_settings();
                {
                    let mut context = self.app.lock().await;
                    info!(
                        "Resetting APDS config for profile {:?} to default: {:?}",
                        context.profile_manager.get_current_profile().await,
                        config
                    );
                    context.save_apds_config(config).await;
                }
            }
            ApdsEvent::PrintConfig => {
                let mut context = self.app.lock().await;
                if let Some(config) =
                    context.profile_manager.get_apds_config().await
                {
                    info!("PrintConfig Requested: {:?}", config);
                } else {
                    warn!("APDS config missing");
                }
            }
        }
    }
}
