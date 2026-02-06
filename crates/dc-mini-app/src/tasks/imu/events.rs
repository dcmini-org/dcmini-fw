#[cfg(not(feature = "r6"))]
use super::*;
use crate::prelude::*;
use dc_mini_bsp::ImuResources;
use derive_more::From;
use embassy_sync::mutex::Mutex;
#[cfg(not(feature = "r6"))]
use portable_atomic::Ordering;

#[derive(Debug, From)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ImuEvent {
    StartStream,
    StopStream,
    ResetConfig,
    PrintConfig,
    ConfigChanged,
}

#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ImuEventError {
    InvalidConversion(u8),
}

impl TryFrom<u8> for ImuEvent {
    type Error = ImuEventError;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(ImuEvent::StartStream),
            1 => Ok(ImuEvent::StopStream),
            2 => Ok(ImuEvent::ResetConfig),
            3 => Ok(ImuEvent::PrintConfig),
            _ => Err(ImuEventError::InvalidConversion(value)),
        }
    }
}

#[derive(Clone)]
pub struct ImuManager {
    #[allow(dead_code)]
    bus_manager: &'static I2cBusManager,
    #[allow(dead_code)]
    imu: &'static Mutex<CriticalSectionRawMutex, ImuResources>,
    #[allow(dead_code)]
    app: &'static Mutex<CriticalSectionRawMutex, AppContext>,
}

impl ImuManager {
    pub fn new(
        bus_manager: &'static I2cBusManager,
        imu: &'static Mutex<CriticalSectionRawMutex, ImuResources>,
        app: &'static Mutex<CriticalSectionRawMutex, AppContext>,
    ) -> Self {
        Self { bus_manager, imu, app }
    }

    pub async fn handle_event(&self, event: ImuEvent) {
        info!("Received event {:?}", event);
        #[cfg(not(feature = "r6"))]
        match event {
            ImuEvent::ConfigChanged => {
                // Handle configuration changes
                if IMU_MEAS.load(Ordering::SeqCst) {
                    // We are streaming and need to update the active IMU config.
                    let mut app_ctx = self.app.lock().await;
                    if let Some(imu_config) =
                        app_ctx.profile_manager.get_imu_config().await.cloned()
                    {
                        IMU_MEAS_SIG.signal(Some(imu_config));
                    }
                }
            }
            ImuEvent::StopStream => {
                if !IMU_MEAS.load(Ordering::SeqCst) {
                    info!("Tried to stop IMU when it was already stopped.")
                } else {
                    IMU_MEAS_SIG.signal(None);
                    IMU_WATCH.sender().send(false);
                }
            }
            ImuEvent::StartStream => {
                if IMU_MEAS.load(Ordering::SeqCst) {
                    info!("Tried to start IMU stream while already running.");
                } else {
                    let mut app_ctx = self.app.lock().await;
                    let mut imu_config = app_ctx
                        .profile_manager
                        .get_imu_config()
                        .await
                        .cloned();
                    if imu_config.is_none() {
                        imu_config = Some(default_imu_settings());
                        app_ctx
                            .save_imu_config(imu_config.clone().unwrap())
                            .await;
                    }
                    app_ctx.low_prio_spawner.must_spawn(imu_task(
                        self.bus_manager,
                        self.imu,
                        imu_config.unwrap(),
                    ));
                    IMU_WATCH.sender().send(true);
                };
            }
            ImuEvent::ResetConfig => {
                if IMU_MEAS.load(Ordering::SeqCst) {
                    warn!("Not allowed to reset config while IMU streaming.");
                    return;
                }

                // Overwrite the current ImuConfig with the default.
                let config = default_imu_settings();
                {
                    let mut context = self.app.lock().await;
                    info!(
                        "Resetting IMU config for profile {:?} to default: {:?}",
                        context.profile_manager.get_current_profile().await,
                        config
                    );
                    context.save_imu_config(config).await;
                }

                // #[cfg(feature = "softdevice")]
                // update_imu_characteristics(self.app).await;
            }
            ImuEvent::PrintConfig => {
                let mut context = self.app.lock().await;
                let config =
                    unwrap!(context.profile_manager.get_imu_config().await);
                info!("PrintConfig Requested: {:?}", config);
            }
        }
        #[cfg(feature = "r6")]
        {}
    }
}
