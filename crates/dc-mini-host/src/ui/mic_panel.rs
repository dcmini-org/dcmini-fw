use crate::icd::{self, MicConfig, MicSampleRate};
use crate::{DeviceConnection, MicDataFrames};
use egui::{Color32, RichText};
use futures::StreamExt;
use prost::Message as ProtoMessage;
use std::sync::{Arc, Mutex};
use tokio::runtime::Handle;
use tokio::sync::mpsc;

#[derive(Clone)]
pub enum MicMessage {
    Refresh,
    GainDb(i8),
    SampleRate(MicSampleRate),
    Command(u8), // 0=Start, 1=Stop
}

pub struct MicPanel {
    client_tx_task: Option<tokio::task::JoinHandle<()>>,
    stream_task: Option<tokio::task::JoinHandle<()>>,
    update_rx: mpsc::UnboundedReceiver<MicConfig>,
    config_tx: mpsc::UnboundedSender<MicMessage>,
    config: Option<MicConfig>,
    status: bool,
}

impl MicPanel {
    pub fn new(
        client: Arc<Mutex<Option<DeviceConnection>>>,
        rt: Handle,
        stream_callback: Option<Box<dyn Fn(MicDataFrames) + Send>>,
    ) -> Self {
        let (config_tx, config_rx) = mpsc::unbounded_channel();
        let (update_tx, update_rx) = mpsc::unbounded_channel();

        let mut panel = Self {
            client_tx_task: None,
            stream_task: None,
            update_rx,
            config_tx,
            config: None,
            status: false,
        };

        panel.client_tx_task = Some(rt.spawn(Self::handle_config_updates(
            config_rx,
            update_tx,
            client.clone(),
        )));

        if let Some(callback) = stream_callback {
            panel.stream_task = Some(rt.spawn(Self::stream_data(
                callback,
                client.clone(),
            )));
        }

        panel
    }

    async fn stream_data(
        callback: Box<dyn Fn(MicDataFrames) + Send>,
        client: Arc<Mutex<Option<DeviceConnection>>>,
    ) {
        loop {
            let connection = {
                client.lock().unwrap().as_ref().cloned()
            };

            if let Some(conn) = connection {
                match conn {
                    DeviceConnection::Ble(ble_client) => {
                        let mut stream = ble_client.notify_mic_stream().await;
                        println!("Waiting for mic data stream updates");

                        while let Some(data) = stream.next().await {
                            match data {
                                Ok(data) => {
                                    if let Ok(frame) =
                                        icd::mic_proto::MicDataFrame::decode(
                                            &data[..],
                                        )
                                    {
                                        callback(MicDataFrames::Proto(frame));
                                    }
                                }
                                Err(e) => {
                                    println!("Mic data stream error: {:?}", e);
                                    if e.kind()
                                        == bluest::error::ErrorKind::NotConnected
                                    {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    DeviceConnection::Usb(usb_client) => {
                        let sub = usb_client
                            .client
                            .subscribe_multi::<icd::MicTopic>(8)
                            .await;

                        if let Ok(mut sub) = sub {
                            while let Ok(frame) = sub.recv().await {
                                callback(MicDataFrames::Icd(frame));
                            }
                        } else {
                            tokio::time::sleep(
                                tokio::time::Duration::from_secs(1),
                            )
                            .await;
                        }
                    }
                }
            } else {
                tokio::time::sleep(tokio::time::Duration::from_millis(500))
                    .await;
            }
        }
    }

    async fn handle_config_updates(
        mut config_rx: mpsc::UnboundedReceiver<MicMessage>,
        update_tx: mpsc::UnboundedSender<MicConfig>,
        client: Arc<Mutex<Option<DeviceConnection>>>,
    ) {
        while let Some(update) = config_rx.recv().await {
            let connection = {
                client.lock().unwrap().as_ref().cloned()
            };

            if let Some(conn) = connection {
                match conn {
                    DeviceConnection::Ble(client) => match update {
                        MicMessage::Refresh => {
                            if let Ok(config) = client.get_mic_config().await {
                                let _ = update_tx.send(config);
                            }
                        }
                        MicMessage::Command(cmd) => match cmd {
                            0 => {
                                let _ = client.start_mic_streaming().await;
                            }
                            1 => {
                                let _ = client.stop_mic_streaming().await;
                            }
                            _ => {}
                        },
                        MicMessage::GainDb(gain) => {
                            if let Ok(current) = client.get_mic_config().await
                            {
                                let new_config = MicConfig {
                                    gain_db: gain,
                                    sample_rate: current.sample_rate,
                                };
                                if client
                                    .set_mic_config(&new_config)
                                    .await
                                    .is_ok()
                                {
                                    let _ = update_tx.send(new_config);
                                }
                            }
                        }
                        MicMessage::SampleRate(rate) => {
                            if let Ok(current) = client.get_mic_config().await
                            {
                                let new_config = MicConfig {
                                    gain_db: current.gain_db,
                                    sample_rate: rate,
                                };
                                if client
                                    .set_mic_config(&new_config)
                                    .await
                                    .is_ok()
                                {
                                    let _ = update_tx.send(new_config);
                                }
                            }
                        }
                    },
                    DeviceConnection::Usb(client) => match update {
                        MicMessage::Refresh => {
                            if let Ok(config) = client.get_mic_config().await {
                                let _ = update_tx.send(config);
                            }
                        }
                        MicMessage::Command(cmd) => match cmd {
                            0 => {
                                let _ = client.start_mic_streaming().await;
                            }
                            1 => {
                                let _ = client.stop_mic_streaming().await;
                            }
                            _ => {}
                        },
                        MicMessage::GainDb(gain) => {
                            if let Ok(current) = client.get_mic_config().await
                            {
                                let new_config = MicConfig {
                                    gain_db: gain,
                                    sample_rate: current.sample_rate,
                                };
                                if let Ok(true) = client
                                    .set_mic_config(new_config.clone())
                                    .await
                                {
                                    let _ = update_tx.send(new_config);
                                }
                            }
                        }
                        MicMessage::SampleRate(rate) => {
                            if let Ok(current) = client.get_mic_config().await
                            {
                                let new_config = MicConfig {
                                    gain_db: current.gain_db,
                                    sample_rate: rate,
                                };
                                if let Ok(true) = client
                                    .set_mic_config(new_config.clone())
                                    .await
                                {
                                    let _ = update_tx.send(new_config);
                                }
                            }
                        }
                    },
                }
            }
        }
    }

    fn send_message(&self, message: MicMessage) {
        let _ = self.config_tx.send(message);
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        if let Ok(config) = self.update_rx.try_recv() {
            self.config = Some(config);
        }

        ui.vertical(|ui| {
            ui.heading("Microphone");
            ui.separator();

            // Start/Stop Streaming
            ui.horizontal(|ui| {
                let status_text = if self.status { "Stop" } else { "Start" };
                if ui.button(status_text).clicked() {
                    self.status = !self.status;
                    if self.status {
                        self.send_message(MicMessage::Command(0));
                    } else {
                        self.send_message(MicMessage::Command(1));
                    }
                }
                ui.label(if self.status {
                    RichText::new("Streaming").color(Color32::GREEN)
                } else {
                    RichText::new("Stopped").color(Color32::RED)
                });
            });

            ui.separator();

            if let Some(config) = &self.config {
                let mut config = config.clone();

                // Gain slider
                ui.horizontal(|ui| {
                    ui.label("Gain (dB):");
                    let mut gain = config.gain_db as f32;
                    if ui
                        .add(egui::Slider::new(&mut gain, -20.0..=20.0).step_by(1.0))
                        .changed()
                    {
                        self.send_message(MicMessage::GainDb(gain as i8));
                        config.gain_db = gain as i8;
                    }
                });

                // Sample rate dropdown
                ui.horizontal(|ui| {
                    ui.label("Sample Rate:");
                    egui::ComboBox::from_id_salt("mic_sample_rate")
                        .selected_text(match config.sample_rate {
                            MicSampleRate::Rate16000 => "16 kHz",
                            MicSampleRate::Rate12800 => "12.8 kHz",
                            MicSampleRate::Rate20000 => "20 kHz",
                        })
                        .show_ui(ui, |ui| {
                            for (rate, label) in [
                                (MicSampleRate::Rate16000, "16 kHz"),
                                (MicSampleRate::Rate12800, "12.8 kHz"),
                                (MicSampleRate::Rate20000, "20 kHz"),
                            ] {
                                if ui
                                    .selectable_value(
                                        &mut config.sample_rate,
                                        rate,
                                        label,
                                    )
                                    .clicked()
                                {
                                    self.send_message(MicMessage::SampleRate(
                                        rate,
                                    ));
                                }
                            }
                        });
                });

                self.config = Some(config);
            } else {
                ui.label(
                    RichText::new("Waiting for configuration...")
                        .color(Color32::GRAY),
                );
            }
        });
    }

    pub fn refresh(&mut self) {
        self.config = None;
        self.status = false;
        self.send_message(MicMessage::Refresh);
    }
}

impl Drop for MicPanel {
    fn drop(&mut self) {
        if let Some(task) = self.client_tx_task.take() {
            task.abort();
        }
        if let Some(task) = self.stream_task.take() {
            task.abort();
        }
    }
}
