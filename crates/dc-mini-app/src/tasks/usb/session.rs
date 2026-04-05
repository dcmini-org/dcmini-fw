use crate::prelude::*;
use dc_mini_icd::SessionId;
use heapless::String;
use postcard_rpc::header::VarHeader;

pub async fn session_get_status(
    _context: &mut Context,
    _header: VarHeader,
    _rqst: (),
) -> bool {
    crate::tasks::session::is_active()
}

pub async fn session_get_id(
    context: &mut Context,
    _header: VarHeader,
    _rqst: (),
) -> SessionId {
    let mut app_ctx = context.app.lock().await;
    match app_ctx.profile_manager.get_session_id().await.cloned() {
        Some(session_id) => session_id,
        None => {
            let default = SessionId(unwrap!(String::try_from("")));
            if app_ctx
                .profile_manager
                .set_session_id(default.clone())
                .await
                .is_err()
            {
                warn!("Failed to persist default session id");
            }
            report_status(
                icd::SubsystemId::Storage,
                icd::SubsystemState::Degraded,
                icd::FaultCode::ConfigReseeded,
            )
            .await;
            default
        }
    }
}

pub async fn session_set_id(
    context: &mut Context,
    _header: VarHeader,
    rqst: SessionId,
) -> bool {
    let mut app_ctx = context.app.lock().await;
    if app_ctx.profile_manager.set_session_id(rqst).await.is_ok() {
        true
    } else {
        report_status(
            icd::SubsystemId::Storage,
            icd::SubsystemState::Degraded,
            icd::FaultCode::InvalidSessionId,
        )
        .await;
        false
    }
}

pub async fn session_start(
    context: &mut Context,
    _header: VarHeader,
    _rqst: (),
) -> bool {
    let app_ctx = context.app.lock().await;
    app_ctx.event_sender.send(SessionEvent::StartRecording.into()).await;
    true
}

pub async fn session_stop(
    context: &mut Context,
    _header: VarHeader,
    _rqst: (),
) -> bool {
    let app_ctx = context.app.lock().await;
    app_ctx.event_sender.send(SessionEvent::StopRecording.into()).await;
    true
}
