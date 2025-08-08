use crate::{icd, DeviceConnection};
use egui::{Color32, RichText};
use std::sync::{Arc, Mutex};
use tokio::{runtime::Handle, sync::mpsc};

#[derive(Debug, Clone)]
pub enum BatteryCommand {
    GetLevel,
}

#[derive(Debug, Clone)]
pub enum BatteryEvent {
    LevelChanged(u8),
}

pub struct BatteryPanel {
    level: Option<icd::BatteryLevel>,
    client: Arc<Mutex<Option<DeviceConnection>>>,
    command_sender: mpsc::UnboundedSender<BatteryCommand>,
    event_receiver: mpsc::UnboundedReceiver<BatteryEvent>,
    background_task: Option<tokio::task::JoinHandle<()>>,
    rt: Handle,
}

impl BatteryPanel {
    pub fn new(
        client: Arc<Mutex<Option<DeviceConnection>>>,
        rt: Handle,
    ) -> Self {
        // Channel for sending commands to the background task
        let (command_sender, command_receiver) = mpsc::unbounded_channel();
        // Channel for receiving events from the background task
        let (event_sender, event_receiver) = mpsc::unbounded_channel();

        let mut panel = Self {
            level: None,
            client,
            command_sender,
            event_receiver,
            background_task: None,
            rt,
        };

        // Start background task that:
        // 1. Receives commands from UI via command_receiver
        // 2. Processes commands and gets battery data
        // 3. Sends events back to UI via event_sender
        panel.start_background_task(command_receiver, event_sender);
        panel
    }

    fn start_background_task(
        &mut self,
        mut command_receiver: mpsc::UnboundedReceiver<BatteryCommand>,
        event_sender: mpsc::UnboundedSender<BatteryEvent>,
    ) {
        let client = self.client.clone();

        self.background_task = Some(self.rt.spawn(async move {
            while let Some(command) = command_receiver.recv().await {
                match command {
                    BatteryCommand::GetLevel => {
                        let connection =
                            client.lock().ok().and_then(|guard| guard.clone());

                        match connection {
                            Some(DeviceConnection::Usb(client)) => {
                                if let Ok(level) =
                                    client.get_battery_level().await
                                {
                                    let _ = event_sender.send(
                                        BatteryEvent::LevelChanged(level.0),
                                    );
                                }
                            }
                            Some(DeviceConnection::Ble(client)) => {
                                if let Ok(level) =
                                    client.get_battery_level().await
                                {
                                    let _ = event_sender.send(
                                        BatteryEvent::LevelChanged(level.0),
                                    );
                                }
                            }
                            None => {}
                        }
                    }
                }
            }
        }));
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        // Process any pending events from the background task
        while let Ok(event) = self.event_receiver.try_recv() {
            match event {
                BatteryEvent::LevelChanged(level) => {
                    self.level = Some(icd::BatteryLevel(level));
                }
            }
        }

        ui.vertical(|ui| {
            ui.heading("Battery Status");
            ui.separator();

            if let Some(level) = &self.level {
                let percentage = level.0;
                let color = if percentage > 60 {
                    Color32::GREEN
                } else if percentage > 20 {
                    Color32::YELLOW
                } else {
                    Color32::RED
                };
                ui.label(
                    RichText::new(format!("Battery: {}%", percentage))
                        .color(color),
                );
            } else {
                ui.label(
                    RichText::new("Battery level unknown")
                        .color(Color32::GRAY),
                );
            }
        });
    }

    pub fn refresh(&mut self) {
        self.level = None;
        // Send command to get latest battery level
        let _ = self.command_sender.send(BatteryCommand::GetLevel);
    }
}

impl Drop for BatteryPanel {
    fn drop(&mut self) {
        if let Some(task) = self.background_task.take() {
            task.abort();
        }
    }
}
