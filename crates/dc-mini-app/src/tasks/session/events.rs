use super::*;
use crate::prelude::*;
use portable_atomic::Ordering;
use session::recording_task;

#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum SessionEvent {
    StartRecording,
    StopRecording,
}

#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum SessionEventError {
    InvalidConversion(u8),
}

impl TryFrom<u8> for SessionEvent {
    type Error = SessionEventError;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(SessionEvent::StartRecording),
            1 => Ok(SessionEvent::StopRecording),
            _ => Err(SessionEventError::InvalidConversion(value)),
        }
    }
}

pub struct SessionManager {
    app: &'static Mutex<CriticalSectionRawMutex, AppContext>,
    sd: &'static Mutex<CriticalSectionRawMutex, SdCardResources>,
}

impl SessionManager {
    pub fn new(
        app: &'static Mutex<CriticalSectionRawMutex, AppContext>,
        sd: &'static Mutex<CriticalSectionRawMutex, SdCardResources>,
    ) -> Self {
        Self { app, sd }
    }

    pub async fn handle_event(&mut self, event: SessionEvent) {
        match event {
            SessionEvent::StartRecording => {
                if SESSION_ACTIVE.load(Ordering::SeqCst) {
                    warn!("Tried to StartRecording while recording already active!");
                    return;
                }
                SESSION_SIG.reset();
                let mut app_ctx = self.app.lock().await;
                let id =
                    app_ctx.profile_manager.get_session_id().await.cloned();
                app_ctx
                    .low_prio_spawner
                    .must_spawn(recording_task(self.sd, id));
            }
            SessionEvent::StopRecording => {
                if !SESSION_ACTIVE.load(Ordering::SeqCst) {
                    warn!("Tried to StopRecording while recording already stopped!");
                    return;
                }
                SESSION_SIG.signal(());
            }
        }
    }
}
