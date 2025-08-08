use crate::DeviceConnection;
use egui::{Color32, RichText};
use std::sync::{Arc, Mutex};
use tokio::{runtime::Handle, sync::mpsc};

#[derive(Debug, Clone)]
pub enum SessionCommand {
    GetId,
    SetId(String),
    Command(u8), // 0 = Start, 1 = Stop
}

#[derive(Debug, Clone)]
pub enum SessionEvent {
    IdChanged(String),
}

pub struct SessionPanel {
    id: Option<String>,
    new_id: String,
    is_running: bool, // Track if session is running
    client: Arc<Mutex<Option<DeviceConnection>>>,
    command_sender: mpsc::UnboundedSender<SessionCommand>,
    event_receiver: mpsc::UnboundedReceiver<SessionEvent>,
    background_task: Option<tokio::task::JoinHandle<()>>,
    rt: Handle,
}

impl SessionPanel {
    pub fn new(
        client: Arc<Mutex<Option<DeviceConnection>>>,
        rt: Handle,
    ) -> Self {
        // Channel for sending commands to the background task
        let (command_sender, command_receiver) = mpsc::unbounded_channel();
        // Channel for receiving events from the background task
        let (event_sender, event_receiver) = mpsc::unbounded_channel();

        let mut panel = Self {
            id: None,
            is_running: false,
            client,
            command_sender,
            event_receiver,
            background_task: None,
            rt,
            new_id: String::new(),
        };

        // Start background task that:
        // 1. Receives commands from UI via command_receiver
        // 2. Processes commands and gets/sets session data
        // 3. Sends events back to UI via event_sender
        panel.start_background_task(command_receiver, event_sender);
        panel
    }

    fn start_background_task(
        &mut self,
        mut command_receiver: mpsc::UnboundedReceiver<SessionCommand>,
        event_sender: mpsc::UnboundedSender<SessionEvent>,
    ) {
        let client = self.client.clone();

        self.background_task = Some(self.rt.spawn(async move {
            while let Some(command) = command_receiver.recv().await {
                let connection =
                    client.lock().ok().and_then(|guard| guard.clone());

                match (command, connection) {
                    (
                        SessionCommand::GetId,
                        Some(DeviceConnection::Usb(client)),
                    ) => {
                        if let Ok(id) = client.get_session_id().await {
                            let _ =
                                event_sender.send(SessionEvent::IdChanged(id));
                        }
                    }
                    (
                        SessionCommand::GetId,
                        Some(DeviceConnection::Ble(client)),
                    ) => {
                        if let Ok(id) = client.get_session_id().await {
                            let _ =
                                event_sender.send(SessionEvent::IdChanged(id));
                        }
                    }
                    (
                        SessionCommand::SetId(new_id),
                        Some(DeviceConnection::Usb(client)),
                    ) => {
                        if let Ok(_) = client.set_session_id(new_id).await {
                            // After setting, get the new ID to confirm
                            if let Ok(id) = client.get_session_id().await {
                                let _ = event_sender
                                    .send(SessionEvent::IdChanged(id));
                            }
                        }
                    }
                    (
                        SessionCommand::SetId(new_id),
                        Some(DeviceConnection::Ble(client)),
                    ) => {
                        if let Ok(_) = client.set_session_id(&new_id).await {
                            // After setting, get the new ID to confirm
                            if let Ok(id) = client.get_session_id().await {
                                let _ = event_sender
                                    .send(SessionEvent::IdChanged(id));
                            }
                        }
                    }
                    (
                        SessionCommand::Command(cmd),
                        Some(DeviceConnection::Ble(client)),
                    ) => {
                        let _ = client.send_session_command(cmd).await;
                    }
                    (
                        SessionCommand::Command(cmd),
                        Some(DeviceConnection::Usb(client)),
                    ) => {
                        // TODO: Implement USB client endpoint
                        if cmd == 0 {
                            let _ = client.start_session().await;
                        } else if cmd == 1 {
                            let _ = client.stop_session().await;
                        } else {
                            println!("INVALID SESSION COMMAND: {:?}", cmd);
                        }
                    }
                    (_, _) => {}
                }
            }
        }));
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        // Process any pending events from the background task
        while let Ok(event) = self.event_receiver.try_recv() {
            match event {
                SessionEvent::IdChanged(id) => {
                    self.id = Some(id);
                }
            }
        }

        ui.vertical(|ui| {
            ui.heading("Session Management");
            ui.separator();

            // Start/Stop Session
            ui.horizontal(|ui| {
                let status_text =
                    if self.is_running { "Stop" } else { "Start" };
                if ui.button(status_text).clicked() {
                    self.is_running = !self.is_running;
                    if self.is_running {
                        // start
                        let _ = self
                            .command_sender
                            .send(SessionCommand::Command(0));
                    } else {
                        // stop
                        let _ = self
                            .command_sender
                            .send(SessionCommand::Command(1));
                    }
                }
                ui.label(if self.is_running {
                    RichText::new("Session Running").color(Color32::GREEN)
                } else {
                    RichText::new("Session Stopped").color(Color32::RED)
                });
            });

            ui.separator();

            if let Some(id) = &self.id {
                // Session ID
                ui.horizontal(|ui| {
                    ui.label("Current Session ID:");
                    ui.label(RichText::new(id).monospace());
                });

                // Session ID Input
                ui.horizontal(|ui| {
                    ui.label("New Session ID:");
                    let response = ui.text_edit_singleline(&mut self.new_id);

                    if response.lost_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter))
                    {
                        let _ = self
                            .command_sender
                            .send(SessionCommand::SetId(self.new_id.clone()));
                        self.new_id.clear();
                    }
                });
            } else {
                ui.label(
                    RichText::new("Session information unavailable")
                        .color(Color32::GRAY),
                );
            }
        });
    }

    pub fn refresh(&mut self) {
        self.id = None;
        self.is_running = false; // Reset running state
        let _ = self.command_sender.send(SessionCommand::GetId);
    }
}

impl Drop for SessionPanel {
    fn drop(&mut self) {
        if let Some(task) = self.background_task.take() {
            task.abort();
        }
    }
}
