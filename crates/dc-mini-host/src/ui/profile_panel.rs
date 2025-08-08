use crate::DeviceConnection;
use dc_mini_icd::MAX_PROFILES;
use egui::{Color32, RichText};
use std::sync::{Arc, Mutex};
use tokio::{runtime::Handle, sync::mpsc};

#[derive(Debug, Clone)]
pub enum ProfileCommand {
    GetProfile,
    SetProfile(u8),
}

#[derive(Debug, Clone)]
pub enum ProfileEvent {
    Changed(u8),
}

pub struct ProfilePanel {
    profile: Option<u8>,
    client: Arc<Mutex<Option<DeviceConnection>>>,
    command_sender: mpsc::UnboundedSender<ProfileCommand>,
    event_receiver: mpsc::UnboundedReceiver<ProfileEvent>,
    background_task: Option<tokio::task::JoinHandle<()>>,
    rt: Handle,
}

impl ProfilePanel {
    pub fn new(
        client: Arc<Mutex<Option<DeviceConnection>>>,
        rt: Handle,
    ) -> (Self, mpsc::UnboundedReceiver<ProfileEvent>) {
        // Channel for sending commands to the background task
        let (command_sender, command_receiver) = mpsc::unbounded_channel();
        // Channel for receiving events from the background task
        let (event_sender, event_receiver) = mpsc::unbounded_channel();
        // Channel for sending events to the parent panel
        let (ui_event_sender, ui_event_receiver) = mpsc::unbounded_channel();

        let mut panel = Self {
            profile: None,
            client,
            command_sender,
            event_receiver,
            background_task: None,
            rt,
        };

        // Start background task that:
        // 1. Receives commands from UI via command_receiver
        // 2. Processes commands and gets/sets profile data
        // 3. Sends events back to UI via event_sender and to parent via ui_event_sender
        panel.start_background_task(
            command_receiver,
            event_sender,
            ui_event_sender,
        );
        (panel, ui_event_receiver)
    }

    fn start_background_task(
        &mut self,
        mut command_receiver: mpsc::UnboundedReceiver<ProfileCommand>,
        event_sender: mpsc::UnboundedSender<ProfileEvent>,
        ui_event_sender: mpsc::UnboundedSender<ProfileEvent>,
    ) {
        let client = self.client.clone();

        self.background_task = Some(self.rt.spawn(async move {
            while let Some(command) = command_receiver.recv().await {
                let connection =
                    client.lock().ok().and_then(|guard| guard.clone());

                match (command, connection) {
                    (
                        ProfileCommand::GetProfile,
                        Some(DeviceConnection::Usb(client)),
                    ) => {
                        if let Ok(profile) = client.get_profile().await {
                            let event = ProfileEvent::Changed(profile);
                            let _ = event_sender.send(event.clone());
                            let _ = ui_event_sender.send(event);
                        }
                    }
                    (
                        ProfileCommand::GetProfile,
                        Some(DeviceConnection::Ble(client)),
                    ) => {
                        if let Ok(profile) = client.get_profile().await {
                            let event = ProfileEvent::Changed(profile);
                            let _ = event_sender.send(event.clone());
                            let _ = ui_event_sender.send(event);
                        }
                    }
                    (
                        ProfileCommand::SetProfile(profile),
                        Some(DeviceConnection::Usb(client)),
                    ) => {
                        if let Ok(_) = client.set_profile(profile).await {
                            // After setting, get the new profile to confirm
                            if let Ok(profile) = client.get_profile().await {
                                let event = ProfileEvent::Changed(profile);
                                let _ = event_sender.send(event.clone());
                                let _ = ui_event_sender.send(event);
                            }
                        }
                    }
                    (
                        ProfileCommand::SetProfile(profile),
                        Some(DeviceConnection::Ble(client)),
                    ) => {
                        if let Ok(_) = client.set_profile(profile).await {
                            // After setting, get the new profile to confirm
                            if let Ok(profile) = client.get_profile().await {
                                let event = ProfileEvent::Changed(profile);
                                let _ = event_sender.send(event.clone());
                                let _ = ui_event_sender.send(event);
                            }
                        } else {
                            println!("Failed to set profile with Ble Client!");
                        }
                    }
                    _ => {}
                }
            }
        }));
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        // Process any pending events from the background task
        while let Ok(event) = self.event_receiver.try_recv() {
            match event {
                ProfileEvent::Changed(profile) => {
                    self.profile = Some(profile);
                }
            }
        }

        ui.vertical(|ui| {
            ui.heading("Profile Management");
            ui.separator();

            if let Some(current_profile) = self.profile {
                ui.horizontal(|ui| {
                    ui.label("Current Profile:");
                    ui.label(
                        RichText::new(format!("{}", current_profile))
                            .monospace(),
                    );
                });

                ui.horizontal(|ui| {
                    ui.label("Select Profile:");
                    for profile in 0..MAX_PROFILES {
                        let text = format!("{}", profile);
                        if ui
                            .selectable_label(current_profile == profile, text)
                            .clicked()
                        {
                            let _ = self
                                .command_sender
                                .send(ProfileCommand::SetProfile(profile));
                        }
                    }
                });
            } else {
                ui.label(
                    RichText::new("Profile information unavailable")
                        .color(Color32::GRAY),
                );
            }
        });
    }

    pub fn refresh(&mut self) {
        // Send command to get latest profile
        let _ = self.command_sender.send(ProfileCommand::GetProfile);
    }
}

impl Drop for ProfilePanel {
    fn drop(&mut self) {
        if let Some(task) = self.background_task.take() {
            task.abort();
        }
    }
}
