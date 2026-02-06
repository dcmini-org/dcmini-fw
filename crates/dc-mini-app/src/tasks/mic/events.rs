use super::*;
use crate::prelude::*;
use derive_more::From;
use embassy_sync::mutex::Mutex;
use portable_atomic::Ordering;

#[derive(Debug, From)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum MicEvent {
    StartStream,
    StopStream,
    SingleSample,
    ConfigChanged,
}

#[derive(Clone)]
pub struct MicManager {
    mic: &'static Mutex<CriticalSectionRawMutex, MicResources>,
    app: &'static Mutex<CriticalSectionRawMutex, AppContext>,
}

impl MicManager {
    pub fn new(
        mic: &'static Mutex<CriticalSectionRawMutex, MicResources>,
        app: &'static Mutex<CriticalSectionRawMutex, AppContext>,
    ) -> Self {
        Self { mic, app }
    }

    pub async fn handle_event(&self, event: MicEvent) {
        info!("Received event {:?}", event);
        match event {
            MicEvent::StartStream => {
                if MIC_STREAMING.load(Ordering::SeqCst) {
                    info!("Tried to start mic stream while already running.");
                } else {
                    let mut app_ctx = self.app.lock().await;
                    let mut mic_config = app_ctx
                        .profile_manager
                        .get_mic_config()
                        .await
                        .cloned();
                    if mic_config.is_none() {
                        mic_config = Some(default_mic_settings());
                        app_ctx
                            .save_mic_config(mic_config.clone().unwrap())
                            .await;
                    }
                    app_ctx.medium_prio_spawner.must_spawn(mic_stream_task(
                        self.mic,
                        mic_config.unwrap(),
                    ));
                    MIC_WATCH.sender().send(true);
                }
            }
            MicEvent::StopStream => {
                if !MIC_STREAMING.load(Ordering::SeqCst) {
                    info!("Tried to stop mic when it was already stopped.");
                } else {
                    MIC_STREAM_SIG.signal(None);
                    MIC_WATCH.sender().send(false);
                }
            }
            MicEvent::SingleSample => {
                if MIC_STREAMING.load(Ordering::SeqCst) {
                    info!("Cannot single-sample while streaming.");
                } else {
                    let mut app_ctx = self.app.lock().await;
                    let mut mic_config = app_ctx
                        .profile_manager
                        .get_mic_config()
                        .await
                        .cloned();
                    if mic_config.is_none() {
                        mic_config = Some(default_mic_settings());
                        app_ctx
                            .save_mic_config(mic_config.clone().unwrap())
                            .await;
                    }
                    app_ctx
                        .low_prio_spawner
                        .must_spawn(mic_single_sample_task(
                            self.mic,
                            mic_config.unwrap(),
                        ));
                }
            }
            MicEvent::ConfigChanged => {
                if MIC_STREAMING.load(Ordering::SeqCst) {
                    let mut app_ctx = self.app.lock().await;
                    if let Some(mic_config) =
                        app_ctx.profile_manager.get_mic_config().await.cloned()
                    {
                        MIC_STREAM_SIG.signal(Some(mic_config));
                    }
                }
            }
        }
    }
}
