use crate::prelude::*;
use dc_mini_icd::{ProfileCommand, MAX_PROFILES};
use embassy_sync::channel::Receiver;

/// Custom Profile Service (UUID: 0x32300000-af46-43af-a0ba-4dbeb457f51c)
#[nrf_softdevice::gatt_service(uuid = "32300000-af46-43af-a0ba-4dbeb457f51c")]
pub struct ProfileService {
    /// Current Profile (UUID: 0x32300001-af46-43af-a0ba-4dbeb457f51c)
    #[characteristic(
        uuid = "32300001-af46-43af-a0ba-4dbeb457f51c",
        read,
        notify,
        write
    )]
    current_profile: u8,

    /// Profile Command (UUID: 0x32300002-af46-43af-a0ba-4dbeb457f51c)
    #[characteristic(uuid = "32300002-af46-43af-a0ba-4dbeb457f51c", write)]
    command: u8,
}

impl ProfileService {
    pub async fn handle(
        &self,
        rx: Receiver<'_, NoopRawMutex, ProfileServiceEvent, 10>,
        app_context: &'static Mutex<CriticalSectionRawMutex, AppContext>,
    ) {
        loop {
            let event = rx.receive().await;
            match event {
                ProfileServiceEvent::CommandWrite(value) => {
                    if let Ok(cmd) = ProfileCommand::try_from(value) {
                        {
                            let mut app_ctx = app_context.lock().await;
                            match cmd {
                                ProfileCommand::Reset => {
                                    unwrap!(
                                        app_ctx
                                            .profile_manager
                                            .set_current_profile(0)
                                            .await
                                    );
                                }
                                ProfileCommand::Next => {
                                    let current = app_ctx
                                        .profile_manager
                                        .get_current_profile()
                                        .await;
                                    let next = if current >= MAX_PROFILES {
                                        0
                                    } else {
                                        current + 1
                                    };
                                    unwrap!(
                                        app_ctx
                                            .profile_manager
                                            .set_current_profile(next)
                                            .await
                                    );
                                }
                                ProfileCommand::Previous => {
                                    let current = app_ctx
                                        .profile_manager
                                        .get_current_profile()
                                        .await;
                                    let prev = if current == 0 {
                                        MAX_PROFILES
                                    } else {
                                        current - 1
                                    };
                                    unwrap!(
                                        app_ctx
                                            .profile_manager
                                            .set_current_profile(prev)
                                            .await
                                    )
                                }
                            }
                        }
                        update_session_characteristics(app_context).await;
                        update_ads_characteristics(app_context).await;
                    }
                }
                ProfileServiceEvent::CurrentProfileWrite(profile) => {
                    {
                        let mut app_ctx = app_context.lock().await;
                        unwrap!(
                            app_ctx
                                .profile_manager
                                .set_current_profile(profile)
                                .await
                        );
                    }
                    update_session_characteristics(app_context).await;
                    update_ads_characteristics(app_context).await;
                }
                ProfileServiceEvent::CurrentProfileCccdWrite {
                    notifications,
                } => {
                    info!("Profile notifications = {:?}", notifications);
                }
            }
        }
    }
}

/// Updates the profile characteristics
pub async fn update_profile_characteristics(
    app_context: &'static Mutex<CriticalSectionRawMutex, AppContext>,
) {
    let app_ctx = app_context.lock().await;
    let current_profile = app_ctx.profile_manager.get_current_profile().await;
    unwrap!(app_ctx.ble_server.profile.current_profile_set(&current_profile));
}
