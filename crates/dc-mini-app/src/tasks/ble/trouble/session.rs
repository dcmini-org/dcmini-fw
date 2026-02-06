use super::Server;
use crate::prelude::*;
use heapless::{String, Vec};
use trouble_host::prelude::*;

#[gatt_service(uuid = "32200000-af46-43af-a0ba-4dbeb457f51c")]
pub struct SessionService {
    #[characteristic(
        uuid = "32200001-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    pub recording_id: Vec<u8, MAX_ID_LEN>,

    #[characteristic(
        uuid = "32200002-af46-43af-a0ba-4dbeb457f51c",
        read,
        notify
    )]
    pub recording_status: bool,

    #[characteristic(uuid = "32200004-af46-43af-a0ba-4dbeb457f51c", write)]
    pub command: u8,
}

impl<'d> Server<'d> {
    pub async fn handle_session_read_event(
        &self,
        handle: u16,
        app_context: &'static Mutex<CriticalSectionRawMutex, AppContext>,
    ) {
        let _app_ctx = app_context.lock().await;

        if handle == self.session.recording_id.handle {
            // No need to handle read for recording_id as it's handled by the characteristic
        } else if handle == self.session.recording_status.handle {
            // No need to handle read for recording_status as it's handled by the characteristic
        }
    }

    pub async fn handle_session_write_event(
        &self,
        handle: u16,
        app_context: &'static Mutex<CriticalSectionRawMutex, AppContext>,
    ) {
        let app_ctx = app_context.lock().await;
        let evt_sender = app_ctx.event_sender;

        if handle == self.session.recording_id.handle {
            if let Ok(value) = self.get(&self.session.recording_id) {
                if let Ok(_id) = String::<MAX_ID_LEN>::from_utf8(value) {
                    todo!();
                    // let _ = evt_sender
                    //     .send(SessionEvent::UpdateRecordingInfo { id }.into())
                    //     .await;
                }
            }
        } else if handle == self.session.command.handle {
            if let Ok(value) = self.get(&self.session.command) {
                let evt = SessionEvent::try_from(value);
                match evt {
                    Ok(e) => evt_sender.send(e.into()).await,
                    Err(e) => warn!("{:?}", e),
                };
            }
        }
    }
}

pub async fn update_session_characteristics(
    server: &Server<'_>,
    recording_id: &[u8],
    is_recording: bool,
) {
    unwrap!(server.set(
        &server.session.recording_id,
        &Vec::from_slice(recording_id).unwrap(),
    ));
    unwrap!(server.set(&server.session.recording_status, &is_recording));
}
