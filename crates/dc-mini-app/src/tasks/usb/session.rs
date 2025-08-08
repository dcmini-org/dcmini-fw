use crate::prelude::*;
use dc_mini_icd::SessionId;
use heapless::String;
use postcard_rpc::header::VarHeader;

pub async fn session_get_status(
    _context: &mut Context,
    _header: VarHeader,
    _rqst: (),
) -> bool {
    // For now, we'll return false since we're not tracking session status in the profile manager
    false
}

pub async fn session_get_id(
    context: &mut Context,
    _header: VarHeader,
    _rqst: (),
) -> SessionId {
    let mut app_ctx = context.app.lock().await;
    app_ctx
        .profile_manager
        .get_session_id()
        .await
        .cloned()
        .unwrap_or_else(|| SessionId(unwrap!(String::try_from(""))))
}

pub async fn session_set_id(
    context: &mut Context,
    _header: VarHeader,
    rqst: SessionId,
) -> bool {
    let mut app_ctx = context.app.lock().await;
    unwrap!(app_ctx.profile_manager.set_session_id(rqst).await);
    true
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
