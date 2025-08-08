use crate::prelude::*;
use dc_mini_icd::{ProfileCommand, MAX_PROFILES};
use postcard_rpc::header::VarHeader;

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
    unwrap!(app_ctx.profile_manager.set_current_profile(req).await);
    true
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
                unwrap!(app_ctx.profile_manager.set_current_profile(0).await);
            }
            ProfileCommand::Next => {
                let current =
                    app_ctx.profile_manager.get_current_profile().await;
                let next =
                    if current >= MAX_PROFILES { 0 } else { current + 1 };
                unwrap!(
                    app_ctx.profile_manager.set_current_profile(next).await
                );
            }
            ProfileCommand::Previous => {
                let current =
                    app_ctx.profile_manager.get_current_profile().await;
                let prev =
                    if current == 0 { MAX_PROFILES } else { current - 1 };
                unwrap!(
                    app_ctx.profile_manager.set_current_profile(prev).await
                );
            }
        }
    }

    #[cfg(feature = "softdevice")]
    {
        update_profile_characteristics(context.app).await;
        update_session_characteristics(context.app).await;
        update_ads_characteristics(context.app).await;
    }
    true
}
