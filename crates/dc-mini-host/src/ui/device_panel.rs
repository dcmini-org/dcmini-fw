use crate::ui::{
    AdsPanel, BatteryPanel, DeviceInfoPanel, ProfileEvent, ProfilePanel,
    SessionPanel,
};
use crate::{AdsDataFrames, DeviceConnection};
use crate::{BleClient, UsbClient};
use dc_mini_icd::SampleRate;
use egui::{Color32, RichText};
use std::sync::{Arc, Mutex};
use tokio::{
    runtime::Handle,
    sync::mpsc,
    task::JoinHandle,
    time::{sleep, Duration},
};

#[derive(Debug, Clone)]
enum DetectedDevice {
    Usb,
    Ble,
}

#[derive(Clone)]
pub enum ConnectionEvent {
    Connected(DeviceConnection),
    Disconnected,
}

pub struct DevicePanel {
    connection: Option<DeviceConnection>,
    detected_devices: Arc<Mutex<Vec<DetectedDevice>>>,
    is_scanning: Arc<Mutex<bool>>,
    is_connecting: bool,
    selected_device: Option<usize>,
    connection_sender: mpsc::UnboundedSender<Option<DeviceConnection>>,
    connection_receiver: mpsc::UnboundedReceiver<Option<DeviceConnection>>,
    connection_event_sender: mpsc::UnboundedSender<ConnectionEvent>,
    rt: Handle,
    scan_task: Option<JoinHandle<()>>,
    health_check_task: Option<JoinHandle<()>>,
    // Shared client for child panels
    client: Arc<Mutex<Option<DeviceConnection>>>,
    // Child panels
    battery_panel: BatteryPanel,
    device_info_panel: DeviceInfoPanel,
    profile_panel: ProfilePanel,
    session_panel: SessionPanel,
    ads_panel: AdsPanel,
    // Event receiver for profile changes
    profile_event_receiver: mpsc::UnboundedReceiver<ProfileEvent>,
}

impl DevicePanel {
    pub fn new(
        rt: Handle,
        stream_callback: Option<Box<dyn Fn(SampleRate, AdsDataFrames) + Send>>,
    ) -> Self {
        let (connection_sender, connection_receiver) =
            mpsc::unbounded_channel();
        let (connection_event_sender, _) = mpsc::unbounded_channel();
        let client = Arc::new(Mutex::new(None));

        // Create child panels
        let battery_panel = BatteryPanel::new(client.clone(), rt.clone());
        let device_info_panel =
            DeviceInfoPanel::new(client.clone(), rt.clone());
        let (profile_panel, profile_event_receiver) =
            ProfilePanel::new(client.clone(), rt.clone());
        let session_panel = SessionPanel::new(client.clone(), rt.clone());
        let ads_panel =
            AdsPanel::new(client.clone(), rt.clone(), stream_callback);

        Self {
            connection: None,
            detected_devices: Arc::new(Mutex::new(Vec::new())),
            is_scanning: Arc::new(Mutex::new(false)),
            is_connecting: false,
            selected_device: None,
            connection_sender,
            connection_receiver,
            connection_event_sender,
            rt,
            scan_task: None,
            health_check_task: None,
            // Shared client
            client,
            // Child panels
            battery_panel,
            device_info_panel,
            profile_panel,
            session_panel,
            ads_panel,
            // Event receiver
            profile_event_receiver,
        }
    }

    pub fn connection(&self) -> Option<DeviceConnection> {
        self.connection.clone()
    }

    fn start_scan(&mut self) {
        println!("Starting scan!");
        if *self.is_scanning.lock().unwrap() {
            return;
        }

        // Detach current client if one exists
        if self.connection.is_some() {
            let connection_sender = self.connection_sender.clone();
            self.rt.spawn(async move {
                let _ = connection_sender.send(None);
            });
            self.connection = None;
        }

        let is_scanning = self.is_scanning.clone();
        let detected_devices = self.detected_devices.clone();

        {
            let mut scanning = is_scanning.lock().unwrap();
            *scanning = true;
            detected_devices.lock().unwrap().clear();
        }
        self.selected_device = None;

        // Spawn detection task
        self.scan_task = Some(self.rt.spawn(async move {
            // Allow time for previous interface to properly release (necessary for nusb).
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            let mut devices = Vec::new();
            // Try USB detection
            println!("Scanning Usb!");
            if let Ok(_) = UsbClient::try_new() {
                devices.push(DetectedDevice::Usb);
            }

            // Try BLE detection
            println!("Scanning Ble!");
            if let Ok(_) = tokio::time::timeout(
                tokio::time::Duration::from_secs(8),
                BleClient::new(),
            )
            .await
            {
                devices.push(DetectedDevice::Ble);
            }

            println!("Found {:?}", devices);
            {
                let mut scanning = is_scanning.lock().unwrap();
                *scanning = false;
                *detected_devices.lock().unwrap() = devices;
            }
        }));
    }

    fn start_health_check(&mut self) {
        // Cancel any existing health check task
        if let Some(task) = self.health_check_task.take() {
            task.abort();
        }

        let connection_sender = self.connection_sender.clone();
        let client = self.client.clone();

        // Start a new health check task
        self.health_check_task = Some(self.rt.spawn(async move {
            loop {
                sleep(Duration::from_millis(500)).await;

                let connection =
                    client.lock().ok().and_then(|guard| guard.clone());

                if let Some(connection) = connection {
                    let is_alive = match connection {
                        DeviceConnection::Ble(client) => {
                            client.is_connected().await
                        }
                        DeviceConnection::Usb(client) => client.is_connected(),
                    };
                    if !is_alive {
                        let _ = connection_sender.send(None);
                        break;
                    }
                } else {
                    break;
                }
            }
        }));
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        // Handle connection events
        while let Ok(connection) = self.connection_receiver.try_recv() {
            self.connection = connection.clone();
            // Update the shared client that child panels use and get a handle to the previous
            // connection.
            let previous_connection = {
                if let Ok(mut client) = self.client.lock() {
                    let c = client.as_ref().cloned();
                    *client = self.connection.clone();
                    c
                } else {
                    None
                }
            };
            if let Some(connection) = connection {
                self.start_health_check();
                let _ = self
                    .connection_event_sender
                    .send(ConnectionEvent::Connected(connection));
                // Refresh all panels on connection
                self.ads_panel.refresh();
                self.battery_panel.refresh();
                self.session_panel.refresh();
                self.device_info_panel.refresh();
                self.profile_panel.refresh();
            } else {
                // Explicitly disconnect the client
                println!("Refreshing panels and dropping connection!");
                if let Some(c) = previous_connection {
                    match c {
                        DeviceConnection::Usb(c) => c.client.close(),
                        DeviceConnection::Ble(c) => {
                            self.rt
                                .block_on(c.close())
                                .expect("Failed to close BleClient.");
                        }
                    }
                }
                // Refresh all panels on connection
                self.ads_panel.refresh();
                self.battery_panel.refresh();
                self.session_panel.refresh();
                self.device_info_panel.refresh();
                self.profile_panel.refresh();

                let _ = self
                    .connection_event_sender
                    .send(ConnectionEvent::Disconnected);
                // Reset device selection on disconnection
                self.selected_device = None;
            }
            // Reset connecting state
            self.is_connecting = false;
        }

        // Handle profile events
        while let Ok(event) = self.profile_event_receiver.try_recv() {
            match event {
                ProfileEvent::Changed(_) => {
                    // When profile changes, refresh panels that depend on profile
                    self.ads_panel.refresh();
                    self.session_panel.refresh();
                }
            }
        }

        // Show connection UI
        ui.vertical(|ui| {
            ui.heading("Device Connection");
            ui.separator();

            // Show current connection status
            ui.horizontal(|ui| {
                ui.label("Status:");
                match &self.connection {
                    None => {
                        ui.label(
                            RichText::new("Disconnected").color(Color32::RED),
                        );
                    }
                    Some(DeviceConnection::Usb(_)) => {
                        ui.label(
                            RichText::new("Connected (USB)")
                                .color(Color32::GREEN),
                        );
                    }
                    Some(DeviceConnection::Ble(_)) => {
                        ui.label(
                            RichText::new("Connected (BLE)")
                                .color(Color32::GREEN),
                        );
                    }
                }
            });

            ui.separator();

            // Device detection and selection
            ui.horizontal(|ui| {
                if *self.is_scanning.lock().unwrap() {
                    ui.spinner();
                    ui.label("Scanning for devices...");
                    if ui.button("Stop").clicked() {
                        if let Some(task) = self.scan_task.take() {
                            task.abort();
                        }
                        *self.is_scanning.lock().unwrap() = false;
                    }
                } else {
                    if ui.button("Detect Devices").clicked() {
                        self.start_scan();
                    }
                }
            });

            let detected_devices = self.detected_devices.lock().unwrap();
            if !detected_devices.is_empty() {
                ui.horizontal(|ui| {
                    ui.label("Available Devices:");
                    egui::ComboBox::from_label("")
                        .selected_text(match self.selected_device {
                            Some(idx) => match &detected_devices[idx] {
                                DetectedDevice::Usb => "USB Device",
                                DetectedDevice::Ble => "BLE Device",
                            },
                            None => "Select a device",
                        })
                        .show_ui(ui, |ui| {
                            for (idx, device) in
                                detected_devices.iter().enumerate()
                            {
                                let text = match device {
                                    DetectedDevice::Usb => "USB Device",
                                    DetectedDevice::Ble => "BLE Device",
                                };
                                if ui
                                    .selectable_value(
                                        &mut self.selected_device,
                                        Some(idx),
                                        text,
                                    )
                                    .clicked()
                                {
                                    // Connect to the selected device
                                    let device = device.clone();
                                    let connection_sender =
                                        self.connection_sender.clone();
                                    let rt = self.rt.clone();
                                    self.is_connecting = true;
                                    rt.spawn(async move {
                                        match device {
                                            DetectedDevice::Usb => {
                                                if let Ok(client) =
                                                    UsbClient::try_new()
                                                {
                                                    let _ = connection_sender
                                                        .send(Some(
                                                        DeviceConnection::Usb(
                                                            Arc::new(client),
                                                        ),
                                                    ));
                                                } else {
                                                    let _ = connection_sender
                                                        .send(None);
                                                }
                                            }
                                            DetectedDevice::Ble => {
                                                if let Ok(client) =
                                                    BleClient::new().await
                                                {
                                                    let _ = connection_sender
                                                        .send(Some(
                                                        DeviceConnection::Ble(
                                                            Arc::new(client),
                                                        ),
                                                    ));
                                                } else {
                                                    let _ = connection_sender
                                                        .send(None);
                                                }
                                            }
                                        }
                                    });
                                }
                            }
                        });

                    // Show spinner while connecting
                    if self.is_connecting {
                        ui.spinner();
                    }
                });
            }

            // Disconnect button
            if self.connection.is_some() {
                if ui.button("Disconnect").clicked() {
                    let connection_sender = self.connection_sender.clone();
                    let rt = self.rt.clone();
                    rt.spawn(async move {
                        let _ = connection_sender.send(None);
                    });
                    // Reset the selected device when disconnecting
                    self.selected_device = None;
                }
            }

            // Show child panels when connected
            if self.connection.is_some() {
                self.battery_panel.show(ui);
                ui.separator();

                self.device_info_panel.show(ui);
                ui.separator();

                self.profile_panel.show(ui);
                ui.separator();

                self.session_panel.show(ui);
                ui.separator();

                self.ads_panel.show(ui);
            }
        });
    }
}

impl Drop for DevicePanel {
    fn drop(&mut self) {
        if let Some(task) = self.scan_task.take() {
            task.abort();
        }
        if let Some(task) = self.health_check_task.take() {
            task.abort();
        }
    }
}
