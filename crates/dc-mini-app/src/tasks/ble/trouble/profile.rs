use super::Server;
use crate::prelude::*;
use trouble_host::prelude::*;

/// Profile Command types
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum ProfileCommand {
    Reset = 0,
    Next = 1,
    Previous = 2,
}

impl TryFrom<u8> for ProfileCommand {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(ProfileCommand::Reset),
            1 => Ok(ProfileCommand::Next),
            2 => Ok(ProfileCommand::Previous),
            _ => Err("Invalid profile command"),
        }
    }
}

/// Custom Profile Service (UUID: 0x32300000-af46-43af-a0ba-4dbeb457f51c)
#[gatt_service(uuid = "32300000-af46-43af-a0ba-4dbeb457f51c")]
pub struct ProfileService {
    /// Current Profile (UUID: 0x32300001-af46-43af-a0ba-4dbeb457f51c)
    #[characteristic(
        uuid = "32300001-af46-43af-a0ba-4dbeb457f51c",
        read,
        notify
    )]
    pub current_profile: u8,

    /// Profile Command (UUID: 0x32300002-af46-43af-a0ba-4dbeb457f51c)
    #[characteristic(uuid = "32300002-af46-43af-a0ba-4dbeb457f51c", write)]
    pub command: u8,
}

impl<'d> Server<'d> {
    pub async fn handle_profile_read_event(
        &self,
        handle: u16,
        _app_context: &'static Mutex<CriticalSectionRawMutex, AppContext>,
    ) {
        if handle == self.profile.current_profile.handle {
            // Profile reads are handled by the characteristic directly
        }
    }

    pub async fn handle_profile_write_event(
        &self,
        handle: u16,
        app_context: &'static Mutex<CriticalSectionRawMutex, AppContext>,
    ) {
        let mut app_ctx = app_context.lock().await;
        let _evt_sender = app_ctx.event_sender;

        if handle == self.profile.command.handle {
            if let Ok(value) = self.get(&self.profile.command) {
                if let Ok(cmd) = ProfileCommand::try_from(value) {
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
                            unwrap!(
                                app_ctx
                                    .profile_manager
                                    .set_current_profile(
                                        current.wrapping_add(1)
                                    )
                                    .await
                            );
                        }
                        ProfileCommand::Previous => {
                            let current = app_ctx
                                .profile_manager
                                .get_current_profile()
                                .await;
                            unwrap!(
                                app_ctx
                                    .profile_manager
                                    .set_current_profile(
                                        current.wrapping_sub(1)
                                    )
                                    .await
                            );
                        }
                    }
                }
            }
        }
    }
}

/// Updates the profile characteristics
pub async fn update_profile_characteristics(
    server: &Server<'_>,
    current_profile: u8,
) {
    unwrap!(server.set(&server.profile.current_profile, &current_profile));
}
