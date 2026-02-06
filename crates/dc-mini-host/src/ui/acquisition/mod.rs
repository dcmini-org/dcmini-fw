mod channel;
mod settings;

use crate::icd::{
    self, AdsConfig, CalFreq, CompThreshPos, FLeadOff, Gain, ILeadOff, Mux,
    SampleRate,
};
use crate::{AdsDataFrames, DeviceConnection};
use egui::{Color32, RichText};
use futures::StreamExt;
use prost::Message as ProtoMessage;
use std::sync::{Arc, Mutex};
use tokio::runtime::Handle;
use tokio::sync::{mpsc, watch};

#[derive(Clone)]
pub enum Message {
    Refresh,
    DiasyEn(bool),
    ClkEn(bool),
    SamplingRate(SampleRate),
    InternalCalibration(bool),
    CalibrationAmplitude(bool),
    CalibrationFrequency(CalFreq),
    PdRefBuf(bool),
    BiasMeas(bool),
    BiasRefInt(bool),
    PdBias(bool),
    BiasLoffSens(bool),
    ComparatorThresholdPos(CompThreshPos),
    LeadOffCurrent(ILeadOff),
    LeadOffFrequency(FLeadOff),
    Gpioc([bool; 4]),
    Srb1(bool),
    SingleShot(bool),
    PdLoffComp(bool),
    PowerDown((u8, bool)),
    Gain((u8, Gain)),
    Srb2((u8, bool)),
    Mux((u8, Mux)),
    BiasSensP((u8, bool)),
    BiasSensN((u8, bool)),
    LeadOffSensP((u8, bool)),
    LeadOffSensN((u8, bool)),
    LeadOffFlip((u8, bool)),
    Command(u8),
}

pub struct AcquisitionPanel {
    client_tx_task: Option<tokio::task::JoinHandle<()>>,
    stream_task: Option<tokio::task::JoinHandle<()>>,
    update_rx: mpsc::UnboundedReceiver<AdsConfig>,
    config_tx: mpsc::UnboundedSender<Message>,
    watch_tx: Option<watch::Sender<Option<AdsConfig>>>,
    config: Option<AdsConfig>,
    status: bool,
}

impl AcquisitionPanel {
    pub fn new(
        client: Arc<Mutex<Option<DeviceConnection>>>,
        rt: Handle,
        stream_callback: Option<Box<dyn Fn(SampleRate, AdsDataFrames) + Send>>,
    ) -> Self {
        let (config_tx, config_rx) = mpsc::unbounded_channel();
        let (update_tx, update_rx) = mpsc::unbounded_channel();

        let mut panel = Self {
            client_tx_task: None,
            stream_task: None,
            update_rx,
            config_tx,
            watch_tx: None,
            config: None,
            status: false,
        };

        // Start the config update task
        panel.client_tx_task = Some(rt.spawn(Self::handle_config_updates(
            config_rx,
            update_tx,
            client.clone(),
        )));

        if let Some(callback) = stream_callback {
            let (watch_tx, watch_rx) = watch::channel(None);
            // Start the data stream task
            panel.stream_task = Some(rt.spawn(Self::stream_data(
                watch_rx,
                callback,
                client.clone(),
            )));
            panel.watch_tx = Some(watch_tx);
        }

        panel
    }

    async fn stream_data(
        config: tokio::sync::watch::Receiver<Option<AdsConfig>>,
        callback: Box<dyn Fn(SampleRate, AdsDataFrames) + Send>,
        client: Arc<Mutex<Option<DeviceConnection>>>,
    ) {
        loop {
            let connection = {
                // Scope the MutexGuard to drop it before any await points
                client.lock().unwrap().as_ref().cloned()
            };

            if let Some(conn) = connection {
                match conn {
                    DeviceConnection::Ble(ble_client) => {
                        let mut stream = ble_client.notify_ads_stream().await;
                        println!("Waiting for data stream updates");

                        while let Some(data) = stream.next().await {
                            match data {
                                Ok(data) => {
                                    if let Ok(frame) =
                                        icd::proto::AdsDataFrame::decode(
                                            &data[..],
                                        )
                                    {
                                        let active_config =
                                            { config.borrow().clone() };
                                        if let Some(conf) = active_config {
                                            callback(
                                                conf.sample_rate,
                                                AdsDataFrames::Proto(frame),
                                            );
                                        } else {
                                            println!("Tried to send data but AdsConfig not set!");
                                        }
                                    }
                                }
                                Err(e) => {
                                    println!("Data stream error: {:?}", e);
                                    if e.kind() == bluest::error::ErrorKind::NotConnected {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    DeviceConnection::Usb(usb_client) => {
                        let sub = usb_client
                            .client
                            .subscribe_multi::<icd::AdsTopic>(8)
                            .await;

                        if let Ok(mut sub) = sub {
                            while let Ok(frame) = sub.recv().await {
                                let active_config =
                                    { config.borrow().clone() };
                                if let Some(conf) = active_config {
                                    callback(
                                        conf.sample_rate,
                                        AdsDataFrames::Icd(frame),
                                    );
                                } else {
                                    println!("Tried to send data but AdsConfig not set!");
                                }
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
                // Sleep to wait for valid client.
                tokio::time::sleep(tokio::time::Duration::from_millis(500))
                    .await;
            }
        }
    }

    async fn handle_config_updates(
        mut config_rx: mpsc::UnboundedReceiver<Message>,
        update_tx: mpsc::UnboundedSender<AdsConfig>,
        client: Arc<Mutex<Option<DeviceConnection>>>,
    ) {
        let mut current_config = AdsConfig::default();

        while let Some(update) = config_rx.recv().await {
            let connection = {
                // Scope the MutexGuard to drop it before any await points
                client.lock().unwrap().as_ref().cloned()
            };

            if let Some(conn) = connection {
                match conn {
                    DeviceConnection::Ble(client) => match update {
                        Message::Refresh => {
                            println!("Refreshing config...");
                            if let Ok(config) = client.get_ads_config().await {
                                current_config = config.clone();
                                let _ = update_tx.send(config);
                            }
                        }
                        Message::Command(cmd) => match cmd {
                            0 => {
                                let _ = client.start_streaming().await;
                            }
                            1 => {
                                let _ = client.stop_streaming().await;
                            }
                            2 => {
                                let _ = client.reset_config().await;
                                tokio::time::sleep(
                                    tokio::time::Duration::from_millis(100),
                                )
                                .await;
                                let new_config =
                                    client.get_ads_config().await.unwrap();
                                current_config = new_config;
                                let _ = update_tx.send(current_config.clone());
                            }
                            _ => {}
                        },
                        // Handle all non-channel-specific configuration parameters
                        Message::DiasyEn(enabled) => {
                            if client.set_daisy_en(enabled).await.is_ok() {
                                let mut new_config = current_config.clone();
                                new_config.daisy_en = enabled;
                                current_config = new_config;
                                let _ = update_tx.send(current_config.clone());
                            }
                        }
                        Message::ClkEn(enabled) => {
                            if client.set_clk_en(enabled).await.is_ok() {
                                let mut new_config = current_config.clone();
                                new_config.clk_en = enabled;
                                current_config = new_config;
                                let _ = update_tx.send(current_config.clone());
                            }
                        }
                        Message::SamplingRate(rate) => {
                            if client.set_sample_rate(rate).await.is_ok() {
                                let mut new_config = current_config.clone();
                                new_config.sample_rate = rate;
                                current_config = new_config;
                                let _ = update_tx.send(current_config.clone());
                            }
                        }
                        Message::InternalCalibration(enabled) => {
                            if client
                                .set_internal_calibration(enabled)
                                .await
                                .is_ok()
                            {
                                let mut new_config = current_config.clone();
                                new_config.internal_calibration = enabled;
                                current_config = new_config;
                                let _ = update_tx.send(current_config.clone());
                            }
                        }
                        Message::CalibrationAmplitude(enabled) => {
                            if client
                                .set_calibration_amplitude(enabled)
                                .await
                                .is_ok()
                            {
                                let mut new_config = current_config.clone();
                                new_config.calibration_amplitude = enabled;
                                current_config = new_config;
                                let _ = update_tx.send(current_config.clone());
                            }
                        }
                        Message::CalibrationFrequency(freq) => {
                            if client
                                .set_calibration_frequency(freq)
                                .await
                                .is_ok()
                            {
                                let mut new_config = current_config.clone();
                                new_config.calibration_frequency = freq;
                                current_config = new_config;
                                let _ = update_tx.send(current_config.clone());
                            }
                        }
                        Message::PdRefBuf(enabled) => {
                            if client.set_pd_refbuf(enabled).await.is_ok() {
                                let mut new_config = current_config.clone();
                                new_config.pd_refbuf = enabled;
                                current_config = new_config;
                                let _ = update_tx.send(current_config.clone());
                            }
                        }
                        Message::BiasMeas(enabled) => {
                            if client.set_bias_meas(enabled).await.is_ok() {
                                let mut new_config = current_config.clone();
                                new_config.bias_meas = enabled;
                                current_config = new_config;
                                let _ = update_tx.send(current_config.clone());
                            }
                        }
                        Message::BiasRefInt(enabled) => {
                            if client.set_biasref_int(enabled).await.is_ok() {
                                let mut new_config = current_config.clone();
                                new_config.biasref_int = enabled;
                                current_config = new_config;
                                let _ = update_tx.send(current_config.clone());
                            }
                        }
                        Message::PdBias(enabled) => {
                            if client.set_pd_bias(enabled).await.is_ok() {
                                let mut new_config = current_config.clone();
                                new_config.pd_bias = enabled;
                                current_config = new_config;
                                let _ = update_tx.send(current_config.clone());
                            }
                        }
                        Message::BiasLoffSens(enabled) => {
                            if client.set_bias_loff_sens(enabled).await.is_ok()
                            {
                                let mut new_config = current_config.clone();
                                new_config.bias_loff_sens = enabled;
                                current_config = new_config;
                                let _ = update_tx.send(current_config.clone());
                            }
                        }
                        Message::ComparatorThresholdPos(threshold) => {
                            if client
                                .set_comparator_threshold(threshold)
                                .await
                                .is_ok()
                            {
                                let mut new_config = current_config.clone();
                                new_config.comparator_threshold_pos =
                                    threshold;
                                current_config = new_config;
                                let _ = update_tx.send(current_config.clone());
                            }
                        }
                        Message::LeadOffCurrent(current) => {
                            if client
                                .set_lead_off_current(current)
                                .await
                                .is_ok()
                            {
                                let mut new_config = current_config.clone();
                                new_config.lead_off_current = current;
                                current_config = new_config;
                                let _ = update_tx.send(current_config.clone());
                            }
                        }
                        Message::LeadOffFrequency(freq) => {
                            if client
                                .set_lead_off_frequency(freq)
                                .await
                                .is_ok()
                            {
                                let mut new_config = current_config.clone();
                                new_config.lead_off_frequency = freq;
                                current_config = new_config;
                                let _ = update_tx.send(current_config.clone());
                            }
                        }
                        Message::Gpioc(gpioc) => {
                            let mut new_config = current_config.clone();
                            new_config.gpioc = gpioc;
                            if let Ok(()) =
                                client.set_ads_config(&new_config).await
                            {
                                current_config = new_config;
                                let _ = update_tx.send(current_config.clone());
                            }
                        }
                        Message::Srb1(enabled) => {
                            if client.set_srb1(enabled).await.is_ok() {
                                let mut new_config = current_config.clone();
                                new_config.srb1 = enabled;
                                current_config = new_config;
                                let _ = update_tx.send(current_config.clone());
                            }
                        }
                        Message::SingleShot(enabled) => {
                            if client.set_single_shot(enabled).await.is_ok() {
                                let mut new_config = current_config.clone();
                                new_config.single_shot = enabled;
                                current_config = new_config;
                                let _ = update_tx.send(current_config.clone());
                            }
                        }
                        Message::PdLoffComp(enabled) => {
                            if client.set_pd_loff_comp(enabled).await.is_ok() {
                                let mut new_config = current_config.clone();
                                new_config.pd_loff_comp = enabled;
                                current_config = new_config;
                                let _ = update_tx.send(current_config.clone());
                            }
                        }
                        Message::PowerDown((channel, enabled)) => {
                            let mut power_down = current_config
                                .channels
                                .iter()
                                .map(|ch| ch.power_down as u8)
                                .collect::<Vec<_>>();
                            if let Some(val) =
                                power_down.get_mut(channel as usize)
                            {
                                *val = enabled as u8;
                            }
                            if client.set_power_down(&power_down).await.is_ok()
                            {
                                if let Some(ch) = current_config
                                    .channels
                                    .get_mut(channel as usize)
                                {
                                    ch.power_down = enabled;
                                    let _ =
                                        update_tx.send(current_config.clone());
                                }
                            }
                        }
                        Message::Gain((channel, gain)) => {
                            let mut gains = current_config
                                .channels
                                .iter()
                                .map(|ch| ch.gain.into())
                                .collect::<Vec<_>>();
                            if let Some(val) = gains.get_mut(channel as usize)
                            {
                                *val = gain.into();
                            }
                            if client.set_gain(&gains).await.is_ok() {
                                if let Some(ch) = current_config
                                    .channels
                                    .get_mut(channel as usize)
                                {
                                    ch.gain = gain;
                                    let _ =
                                        update_tx.send(current_config.clone());
                                }
                            }
                        }
                        Message::Srb2((channel, enabled)) => {
                            let mut srb2 = current_config
                                .channels
                                .iter()
                                .map(|ch| ch.srb2 as u8)
                                .collect::<Vec<_>>();
                            if let Some(val) = srb2.get_mut(channel as usize) {
                                *val = enabled as u8;
                            }
                            if client.set_srb2(&srb2).await.is_ok() {
                                if let Some(ch) = current_config
                                    .channels
                                    .get_mut(channel as usize)
                                {
                                    ch.srb2 = enabled;
                                    let _ =
                                        update_tx.send(current_config.clone());
                                }
                            }
                        }
                        Message::Mux((channel, mux)) => {
                            let mut muxes = current_config
                                .channels
                                .iter()
                                .map(|ch| ch.mux.into())
                                .collect::<Vec<_>>();
                            if let Some(val) = muxes.get_mut(channel as usize)
                            {
                                *val = mux.into();
                            }
                            if client.set_mux(&muxes).await.is_ok() {
                                if let Some(ch) = current_config
                                    .channels
                                    .get_mut(channel as usize)
                                {
                                    ch.mux = mux;
                                    let _ =
                                        update_tx.send(current_config.clone());
                                }
                            }
                        }
                        Message::BiasSensP((channel, enabled)) => {
                            let mut bias_sensp = current_config
                                .channels
                                .iter()
                                .map(|ch| ch.bias_sensp as u8)
                                .collect::<Vec<_>>();
                            if let Some(val) =
                                bias_sensp.get_mut(channel as usize)
                            {
                                *val = enabled as u8;
                            }
                            if client.set_bias_sensp(&bias_sensp).await.is_ok()
                            {
                                if let Some(ch) = current_config
                                    .channels
                                    .get_mut(channel as usize)
                                {
                                    ch.bias_sensp = enabled;
                                    let _ =
                                        update_tx.send(current_config.clone());
                                }
                            }
                        }
                        Message::BiasSensN((channel, enabled)) => {
                            let mut bias_sensn = current_config
                                .channels
                                .iter()
                                .map(|ch| ch.bias_sensn as u8)
                                .collect::<Vec<_>>();
                            if let Some(val) =
                                bias_sensn.get_mut(channel as usize)
                            {
                                *val = enabled as u8;
                            }
                            if client.set_bias_sensn(&bias_sensn).await.is_ok()
                            {
                                if let Some(ch) = current_config
                                    .channels
                                    .get_mut(channel as usize)
                                {
                                    ch.bias_sensn = enabled;
                                    let _ =
                                        update_tx.send(current_config.clone());
                                }
                            }
                        }
                        Message::LeadOffSensP((channel, enabled)) => {
                            let mut lead_off_sensp = current_config
                                .channels
                                .iter()
                                .map(|ch| ch.lead_off_sensp as u8)
                                .collect::<Vec<_>>();
                            if let Some(val) =
                                lead_off_sensp.get_mut(channel as usize)
                            {
                                *val = enabled as u8;
                            }
                            if client
                                .set_lead_off_sensp(&lead_off_sensp)
                                .await
                                .is_ok()
                            {
                                if let Some(ch) = current_config
                                    .channels
                                    .get_mut(channel as usize)
                                {
                                    ch.lead_off_sensp = enabled;
                                    let _ =
                                        update_tx.send(current_config.clone());
                                }
                            }
                        }
                        Message::LeadOffSensN((channel, enabled)) => {
                            let mut lead_off_sensn = current_config
                                .channels
                                .iter()
                                .map(|ch| ch.lead_off_sensn as u8)
                                .collect::<Vec<_>>();
                            if let Some(val) =
                                lead_off_sensn.get_mut(channel as usize)
                            {
                                *val = enabled as u8;
                            }
                            if client
                                .set_lead_off_sensn(&lead_off_sensn)
                                .await
                                .is_ok()
                            {
                                if let Some(ch) = current_config
                                    .channels
                                    .get_mut(channel as usize)
                                {
                                    ch.lead_off_sensn = enabled;
                                    let _ =
                                        update_tx.send(current_config.clone());
                                }
                            }
                        }
                        Message::LeadOffFlip((channel, enabled)) => {
                            let mut lead_off_flip = current_config
                                .channels
                                .iter()
                                .map(|ch| ch.lead_off_flip as u8)
                                .collect::<Vec<_>>();
                            if let Some(val) =
                                lead_off_flip.get_mut(channel as usize)
                            {
                                *val = enabled as u8;
                            }
                            if client
                                .set_lead_off_flip(&lead_off_flip)
                                .await
                                .is_ok()
                            {
                                if let Some(ch) = current_config
                                    .channels
                                    .get_mut(channel as usize)
                                {
                                    ch.lead_off_flip = enabled;
                                    let _ =
                                        update_tx.send(current_config.clone());
                                }
                            }
                        }
                    },
                    DeviceConnection::Usb(client) => match update {
                        Message::Refresh => {
                            if let Ok(config) = client.get_ads_config().await {
                                current_config = config.clone();
                                let _ = update_tx.send(config);
                            }
                        }
                        Message::Command(cmd) => match cmd {
                            0 => {
                                let _ = client.start_streaming().await;
                            }
                            1 => {
                                let _ = client.stop_streaming().await;
                            }
                            2 => {
                                let _ = client.reset_ads_config().await;
                                tokio::time::sleep(
                                    tokio::time::Duration::from_millis(300),
                                )
                                .await;
                                let new_config =
                                    client.get_ads_config().await.unwrap();
                                current_config = new_config;
                                let _ = update_tx.send(current_config.clone());
                            }
                            _ => {}
                        },
                        update => {
                            let mut new_config = current_config.clone();
                            Self::apply_update(&mut new_config, &update);
                            if let Ok(success) =
                                client.set_ads_config(new_config.clone()).await
                            {
                                if success {
                                    current_config = new_config;
                                    let _ =
                                        update_tx.send(current_config.clone());
                                }
                            }
                        }
                    },
                }
            }
        }
    }

    fn apply_update(config: &mut AdsConfig, update: &Message) {
        match update {
            Message::Refresh => {}
            Message::DiasyEn(diasy_en) => config.daisy_en = *diasy_en,
            Message::ClkEn(clk_en) => config.clk_en = *clk_en,
            Message::SamplingRate(sampling_rate) => {
                config.sample_rate = *sampling_rate
            }
            Message::InternalCalibration(internal_calibration) => {
                config.internal_calibration = *internal_calibration
            }
            Message::CalibrationAmplitude(calibration_amplitude) => {
                config.calibration_amplitude = *calibration_amplitude
            }
            Message::CalibrationFrequency(calibration_frequency) => {
                config.calibration_frequency = *calibration_frequency
            }
            Message::PdRefBuf(pd_refbuf) => config.pd_refbuf = *pd_refbuf,
            Message::BiasMeas(bias_meas) => config.bias_meas = *bias_meas,
            Message::BiasRefInt(biasref_int) => {
                config.biasref_int = *biasref_int
            }
            Message::PdBias(pd_bias) => config.pd_bias = *pd_bias,
            Message::BiasLoffSens(bias_loff_sens) => {
                config.bias_loff_sens = *bias_loff_sens
            }
            Message::ComparatorThresholdPos(comparator_threshold_pos) => {
                config.comparator_threshold_pos = *comparator_threshold_pos
            }
            Message::LeadOffCurrent(lead_off_current) => {
                config.lead_off_current = *lead_off_current
            }
            Message::LeadOffFrequency(lead_off_frequency) => {
                config.lead_off_frequency = *lead_off_frequency
            }
            Message::Gpioc(gpioc) => config.gpioc = *gpioc,
            Message::Srb1(srb1) => config.srb1 = *srb1,
            Message::SingleShot(single_shot) => {
                config.single_shot = *single_shot
            }
            Message::PdLoffComp(pd_loff_comp) => {
                config.pd_loff_comp = *pd_loff_comp
            }
            Message::PowerDown((index, power_down)) => {
                if let Some(ch) = config.channels.get_mut(*index as usize) {
                    ch.power_down = *power_down;
                }
            }
            Message::Gain((index, gain)) => {
                if let Some(ch) = config.channels.get_mut(*index as usize) {
                    ch.gain = *gain;
                }
            }
            Message::Srb2((index, srb2)) => {
                if let Some(ch) = config.channels.get_mut(*index as usize) {
                    ch.srb2 = *srb2;
                }
            }
            Message::Mux((index, mux)) => {
                if let Some(ch) = config.channels.get_mut(*index as usize) {
                    ch.mux = *mux;
                }
            }
            Message::BiasSensP((index, bias_sensp)) => {
                if let Some(ch) = config.channels.get_mut(*index as usize) {
                    ch.bias_sensp = *bias_sensp;
                }
            }
            Message::BiasSensN((index, bias_sensn)) => {
                if let Some(ch) = config.channels.get_mut(*index as usize) {
                    ch.bias_sensn = *bias_sensn;
                }
            }
            Message::LeadOffSensP((index, lead_off_sensp)) => {
                if let Some(ch) = config.channels.get_mut(*index as usize) {
                    ch.lead_off_sensp = *lead_off_sensp;
                }
            }
            Message::LeadOffSensN((index, lead_off_sensn)) => {
                if let Some(ch) = config.channels.get_mut(*index as usize) {
                    ch.lead_off_sensn = *lead_off_sensn;
                }
            }
            Message::LeadOffFlip((index, lead_off_flip)) => {
                if let Some(ch) = config.channels.get_mut(*index as usize) {
                    ch.lead_off_flip = *lead_off_flip;
                }
            }
            Message::Command(_) => {}
        }
    }

    fn send_message(&self, message: Message) {
        let _ = self.config_tx.send(message);
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        if let Ok(config) = self.update_rx.try_recv() {
            self.config = Some(config);
            if let Some(ch) = &self.watch_tx {
                let _ = ch.send(self.config.clone());
            }
        }

        ui.vertical(|ui| {
            ui.heading("Signal Acquisition");
            ui.separator();

            // Start/Stop Streaming
            ui.horizontal(|ui| {
                let status_text = if self.status { "Stop" } else { "Start" };
                if ui.button(status_text).clicked() {
                    self.status = !self.status;
                    if self.status {
                        // start
                        self.send_message(Message::Command(0));
                    } else {
                        // stop
                        self.send_message(Message::Command(1));
                    }
                }
                ui.label(if self.status {
                    RichText::new("Streaming").color(Color32::GREEN)
                } else {
                    RichText::new("Stopped").color(Color32::RED)
                });
            });

            ui.separator();

            ui.horizontal(|ui| {
                if ui.button("Reset Config").clicked() {
                    self.send_message(Message::Command(2));
                }
            });

            if let Some(config) = &self.config {
                let mut config = config.clone();
                let sender = |msg: Message| self.send_message(msg);

                settings::show_global_settings(ui, &mut config, &sender);
                settings::show_leadoff_settings(ui, &mut config, &sender);
                settings::show_gpio_config(ui, &mut config, &sender);

                // Channel Configuration
                for i in 0..config.channels.len() {
                    ui.collapsing(format!("Channel {}", i), |ui| {
                        channel::show_channel_config(
                            ui,
                            i,
                            &mut config.channels[i],
                            &sender,
                        );
                    });
                }

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
        // Reset the streaming status
        self.status = false;
        // Request a refresh of the configuration
        self.send_message(Message::Refresh);
    }
}

impl Drop for AcquisitionPanel {
    fn drop(&mut self) {
        if let Some(task) = self.client_tx_task.take() {
            task.abort();
        }
        if let Some(task) = self.stream_task.take() {
            task.abort();
        }
    }
}
