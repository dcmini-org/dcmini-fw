use super::gatt::{ServerWithDfu, ServerWithoutDfu};
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
        read
    )]
    pub recording_status: bool,

    #[characteristic(uuid = "32200004-af46-43af-a0ba-4dbeb457f51c", write)]
    pub command: u8,
}

macro_rules! impl_session_support {
    ($server_ty:ident, $update_fn:ident) => {
        impl<'d> $server_ty<'d> {
            pub async fn handle_session_read_event(
                &self,
                handle: u16,
                _app_context: &'static Mutex<
                    CriticalSectionRawMutex,
                    AppContext,
                >,
            ) {
                if handle == self.session.recording_id.handle {
                } else if handle == self.session.recording_status.handle {
                    unwrap!(self.set(
                        &self.session.recording_status,
                        &crate::tasks::session::is_active(),
                    ));
                }
            }

            pub async fn handle_session_write_event(
                &self,
                handle: u16,
                app_context: &'static Mutex<
                    CriticalSectionRawMutex,
                    AppContext,
                >,
            ) {
                if handle == self.session.recording_id.handle {
                    if let Ok(value) = self.get(&self.session.recording_id) {
                        let mut app_ctx = app_context.lock().await;
                        match String::<MAX_ID_LEN>::from_utf8(value) {
                            Ok(id) => {
                                if let Err(e) = app_ctx
                                    .profile_manager
                                    .set_session_id(SessionId(id))
                                    .await
                                {
                                    warn!(
                                        "Failed to persist BLE session id update: {:?}",
                                        e
                                    );
                                    report_status(
                                        icd::SubsystemId::Storage,
                                        icd::SubsystemState::Degraded,
                                        icd::FaultCode::StorageWriteFailed,
                                    )
                                    .await;
                                }
                            }
                            Err(_) => {
                                warn!("Rejected invalid BLE session id");
                                report_status(
                                    icd::SubsystemId::Storage,
                                    icd::SubsystemState::Degraded,
                                    icd::FaultCode::InvalidSessionId,
                                )
                                .await;
                            }
                        }
                    }
                } else if handle == self.session.command.handle {
                    let app_ctx = app_context.lock().await;
                    let evt_sender = app_ctx.event_sender;
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

        pub async fn $update_fn(
            server: &$server_ty<'_>,
            recording_id: &[u8],
            _is_recording: bool,
        ) {
            if let Ok(id) = Vec::from_slice(recording_id) {
                if let Err(e) = server.set(&server.session.recording_id, &id) {
                    warn!(
                        "Failed to update BLE session id characteristic: {:?}",
                        e
                    );
                }
            } else {
                report_status(
                    icd::SubsystemId::BleStream,
                    icd::SubsystemState::Degraded,
                    icd::FaultCode::InvalidSessionId,
                )
                .await;
            }
            if let Err(e) = server.set(
                &server.session.recording_status,
                &crate::tasks::session::is_active(),
            ) {
                warn!(
                    "Failed to update BLE session status characteristic: {:?}",
                    e
                );
            }
        }
    };
}

impl_session_support!(ServerWithDfu, update_session_characteristics_with_dfu);
impl_session_support!(
    ServerWithoutDfu,
    update_session_characteristics_without_dfu
);
