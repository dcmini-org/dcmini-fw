use crate::{icd, DeviceConnection};
use egui::RichText;
use std::sync::{Arc, Mutex};
use tokio::{runtime::Handle, sync::mpsc};

#[derive(Debug, Clone)]
pub enum DeviceInfoCommand {
    GetInfo,
}

#[derive(Debug, Clone)]
pub enum DeviceInfoEvent {
    InfoChanged(icd::DeviceInfo),
}

pub struct DeviceInfoPanel {
    info: Option<icd::DeviceInfo>,
    client: Arc<Mutex<Option<DeviceConnection>>>,
    command_sender: mpsc::UnboundedSender<DeviceInfoCommand>,
    event_receiver: mpsc::UnboundedReceiver<DeviceInfoEvent>,
    background_task: Option<tokio::task::JoinHandle<()>>,
    rt: Handle,
}

impl DeviceInfoPanel {
    pub fn new(
        client: Arc<Mutex<Option<DeviceConnection>>>,
        rt: Handle,
    ) -> Self {
        // Channel for sending commands to the background task
        let (command_sender, command_receiver) = mpsc::unbounded_channel();
        // Channel for receiving events from the background task
        let (event_sender, event_receiver) = mpsc::unbounded_channel();

        let mut panel = Self {
            info: None,
            client,
            command_sender,
            event_receiver,
            background_task: None,
            rt,
        };

        // Start background task that:
        // 1. Receives commands from UI via command_receiver
        // 2. Processes commands and gets device info
        // 3. Sends events back to UI via event_sender
        panel.start_background_task(command_receiver, event_sender);
        panel
    }

    fn start_background_task(
        &mut self,
        mut command_receiver: mpsc::UnboundedReceiver<DeviceInfoCommand>,
        event_sender: mpsc::UnboundedSender<DeviceInfoEvent>,
    ) {
        let client = self.client.clone();

        self.background_task = Some(self.rt.spawn(async move {
            while let Some(command) = command_receiver.recv().await {
                match command {
                    DeviceInfoCommand::GetInfo => {
                        let connection =
                            client.lock().ok().and_then(|guard| guard.clone());

                        match connection {
                            Some(DeviceConnection::Usb(client)) => {
                                if let Ok(info) =
                                    client.get_device_info().await
                                {
                                    let _ = event_sender.send(
                                        DeviceInfoEvent::InfoChanged(info),
                                    );
                                }
                            }
                            Some(DeviceConnection::Ble(client)) => {
                                if let Ok(info) =
                                    client.get_device_info().await
                                {
                                    let _ = event_sender.send(
                                        DeviceInfoEvent::InfoChanged(info),
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
                DeviceInfoEvent::InfoChanged(info) => {
                    self.info = Some(info);
                }
            }
        }

        ui.vertical(|ui| {
            ui.heading("Device Info");
            ui.separator();

            if let Some(info) = &self.info {
                ui.horizontal(|ui| {
                    ui.label("Software Version:");
                    ui.label(
                        RichText::new(format!("{}", info.software_revision,))
                            .monospace(),
                    );
                });

                ui.horizontal(|ui| {
                    ui.label("Hardware Version:");
                    ui.label(
                        RichText::new(format!("{}", info.hardware_revision,))
                            .monospace(),
                    );
                });

                ui.horizontal(|ui| {
                    ui.label("Manufacturer: ");
                    ui.label(
                        RichText::new(format!("{}", info.manufacturer_name,))
                            .monospace(),
                    );
                });
            } else {
                ui.label("Device information unavailable");
            }
        });
    }

    pub fn refresh(&mut self) {
        self.info = None;
        // Send command to get latest device info
        let _ = self.command_sender.send(DeviceInfoCommand::GetInfo);
    }
}

impl Drop for DeviceInfoPanel {
    fn drop(&mut self) {
        if let Some(task) = self.background_task.take() {
            task.abort();
        }
    }
}
