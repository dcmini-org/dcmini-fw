use crate::prelude::*;
use dc_mini_icd::SessionId;
use embassy_sync::channel::Receiver;
use heapless::String;

#[nrf_softdevice::gatt_service(uuid = "32200000-af46-43af-a0ba-4dbeb457f51c")]
pub struct SessionService {
    #[characteristic(
        uuid = "32200001-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    recording_id: String<MAX_ID_LEN>,

    #[characteristic(
        uuid = "32200002-af46-43af-a0ba-4dbeb457f51c",
        read,
        notify
    )]
    recording_status: bool,

    #[characteristic(uuid = "32200004-af46-43af-a0ba-4dbeb457f51c", write)]
    command: u8,
}

impl SessionService {
    pub async fn handle(
        &self,
        rx: Receiver<'_, NoopRawMutex, SessionServiceEvent, 10>,
        app_context: &'static Mutex<CriticalSectionRawMutex, AppContext>,
    ) {
        loop {
            let event = rx.receive().await;
            match event {
                SessionServiceEvent::RecordingIdWrite(id_str) => {
                    info!("Received SessionId = {:?}", id_str);
                    let mut app_ctx = app_context.lock().await;
                    let _ = app_ctx
                        .profile_manager
                        .set_session_id(SessionId(id_str))
                        .await;
                }
                SessionServiceEvent::CommandWrite(cmd) => {
                    // Commands are not supported when using profile manager
                    if let Ok(session_event) = SessionEvent::try_from(cmd) {
                        let app_ctx = app_context.lock().await;
                        app_ctx.event_sender.send(session_event.into()).await;
                    }
                }
                SessionServiceEvent::RecordingStatusCccdWrite {
                    notifications,
                } => {
                    info!(
                        "Recording status notifications = {:?}",
                        notifications
                    );
                }
            }
        }
    }
}

pub async fn update_session_characteristics(
    app_context: &'static Mutex<CriticalSectionRawMutex, AppContext>,
) {
    let mut app_ctx = app_context.lock().await;
    let id = app_ctx
        .profile_manager
        .get_session_id()
        .await
        .cloned()
        .unwrap_or(SessionId { 0: heapless::String::new() });
    unwrap!(app_ctx.ble_server.session.recording_id_set(&id.0));
    unwrap!(app_ctx.ble_server.session.recording_status_set(&false)); // Always false since we're not tracking status
}
