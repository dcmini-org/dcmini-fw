use super::*;
use crate::prelude::*;
use derive_more::From;
use embassy_sync::mutex::Mutex;
use portable_atomic::Ordering;

#[derive(Debug, From)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum CvepEvent {
    Start,
    Stop,
}

#[derive(Clone)]
pub struct CvepManager {
    app: &'static Mutex<CriticalSectionRawMutex, AppContext>,
}

impl CvepManager {
    pub fn new(
        app: &'static Mutex<CriticalSectionRawMutex, AppContext>,
    ) -> Self {
        Self { app }
    }

    pub async fn handle_event(&self, event: CvepEvent) {
        info!("Received event {:?}", event);
        match event {
            CvepEvent::Start => {
                if CVEP_ACTIVE.load(Ordering::SeqCst) {
                    info!("CVEP decoder task already running.");
                    return;
                }

                let app_ctx = self.app.lock().await;
                app_ctx.medium_prio_spawner.must_spawn(cvep_decode_task());
            }
            CvepEvent::Stop => {
                if !CVEP_ACTIVE.load(Ordering::SeqCst) {
                    info!("CVEP decoder task not running.");
                    return;
                }

                CVEP_STOP_SIG.signal(());
            }
        }
    }
}
