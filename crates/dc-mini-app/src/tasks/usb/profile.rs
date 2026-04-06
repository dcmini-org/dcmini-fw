use crate::prelude::*;
use dc_mini_icd::{ProfileCommand, MAX_PROFILES};
use postcard_rpc::header::VarHeader;

async fn persist_current_profile(
    app_ctx: &mut AppContext,
    profile: u8,
) -> bool {
    match app_ctx.profile_manager.set_current_profile(profile).await {
        Ok(()) => true,
        Err(e) => {
            warn!("Failed to persist current profile: {:?}", e);
            report_status(
                icd::SubsystemId::Storage,
                icd::SubsystemState::Degraded,
                icd::FaultCode::StorageWriteFailed,
            )
            .await;
            false
        }
    }
}

pub async fn profile_get(
    context: &mut super::Context,
    _header: VarHeader,
    _req: (),
) -> u8 {
    let app_ctx = context.app.lock().await;
    let profile = app_ctx.profile_manager.get_current_profile().await;
    profile
}

pub async fn profile_set(
    context: &mut super::Context,
    _header: VarHeader,
    req: u8,
) -> bool {
    if req > MAX_PROFILES {
        return false;
    }
    let mut app_ctx = context.app.lock().await;
    persist_current_profile(&mut app_ctx, req).await
}

pub async fn profile_command(
    context: &mut super::Context,
    _header: VarHeader,
    req: ProfileCommand,
) -> bool {
    {
        let mut app_ctx = context.app.lock().await;
        match req {
            ProfileCommand::Reset => {
                return persist_current_profile(&mut app_ctx, 0).await;
            }
            ProfileCommand::Next => {
                let current =
                    app_ctx.profile_manager.get_current_profile().await;
                let next =
                    if current >= MAX_PROFILES { 0 } else { current + 1 };
                return persist_current_profile(&mut app_ctx, next).await;
            }
            ProfileCommand::Previous => {
                let current =
                    app_ctx.profile_manager.get_current_profile().await;
                let prev =
                    if current == 0 { MAX_PROFILES } else { current - 1 };
                return persist_current_profile(&mut app_ctx, prev).await;
            }
        }
    }
}
