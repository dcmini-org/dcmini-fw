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

pub struct AdsPanel {
    client_tx_task: Option<tokio::task::JoinHandle<()>>,
    stream_task: Option<tokio::task::JoinHandle<()>>,
    update_rx: mpsc::UnboundedReceiver<AdsConfig>,
    config_tx: mpsc::UnboundedSender<Message>,
    watch_tx: Option<watch::Sender<Option<AdsConfig>>>,
    config: Option<AdsConfig>,
    status: bool,
}

impl AdsPanel {
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
            // Start the config update task
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

    // Add this method to handle UI events
    fn handle_ui_event(&self, event: Message) {
        self.send_message(event);
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        if let Ok(config) = self.update_rx.try_recv() {
            self.config = Some(config);
            if let Some(ch) = &self.watch_tx {
                let _ = ch.send(self.config.clone());
            }
        }

        ui.vertical(|ui| {
            ui.heading("ADS Configuration");
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
                // Global Settings
                ui.collapsing("Global Settings", |ui| {
                    if ui
                        .checkbox(&mut config.daisy_en, "Multiple Readback Mode")
                        .on_hover_ui(|ui| {
                            ui.label(RichText::new("CONFIG1: ~DAISY_EN").color(Color32::RED));
                            ui.label("Controls which multi-ADS1299 mode is enabled.");
                            ui.label(" DCMini schematic is set up for multiple readback mode.");
                            ui.hyperlink_to(
                                "(See ADS1299 Datasheet 10.1.4.2.)", 
                                "https://www.ti.com/document-viewer/ADS1299/datasheet#applications-and-implementation/SBAS459200"
                            );
                            ui.label("☑: Multiple readback mode **Recommended");
                            ui.label("☐: Daisy-chain mode");
                        })
                        .changed() {
                        self.handle_ui_event(Message::DiasyEn(config.daisy_en));
                    }

                    if ui
                        .checkbox(&mut config.clk_en, "Clock Output")
                        .on_hover_ui(|ui|{
                            ui.label(RichText::new("CONFIG1: CLK_EN").color(Color32::RED));
                            ui.label("Enables clock output driver on base ADS1299 for multiple ADS1299 configurations");
                            ui.label("NB: On DCMini, CLKSEL pin is pulled high on base AND daisy, but clock output is disabled on daisy by firmware.");
                            ui.hyperlink_to(
                                "(See ADS1299 Datasheet 9.3.2.2)", 
                                "https://www.ti.com/document-viewer/ADS1299/datasheet#detailed-description/SBAS4597213"
                            );
                            ui.label("☑: Oscillator clock output enabled; CLK pin is output **Recommended");
                            ui.label("☐: Oscilattor clock output disabled; CLK pin is tri-state input.");
                        })
                        .changed() {
                        self.handle_ui_event(Message::ClkEn(config.clk_en));
                    }

                    ui.horizontal(|ui| {
                        ui.label("Sampling Rate:");
                        egui::ComboBox::new("sample_rate", "")
                            .selected_text(format!("{:?}", config.sample_rate))
                            .show_ui(ui, |ui| {
                                for rate in [
                                    icd::SampleRate::Sps250,
                                    icd::SampleRate::Sps500,
                                    icd::SampleRate::KSps1,
                                    icd::SampleRate::KSps2,
                                    icd::SampleRate::KSps4,
                                    icd::SampleRate::KSps8,
                                    icd::SampleRate::KSps16,
                                ] {
                                    if ui.selectable_value(
                                        &mut config.sample_rate,
                                        rate,
                                        format!("{:?}", rate)
                                    ).clicked() {
                                        self.handle_ui_event(Message::SamplingRate(config.sample_rate));
                                    }
                                }
                            })
                    });

                    if ui
                        .checkbox(
                            &mut config.internal_calibration,
                            "Internal Test Signal Generation",
                        )
                        .on_hover_ui(|ui| {
                            ui.label(RichText::new("CONFIG2: INT_CAL").color(Color32::RED));
                            ui.label("Source for the test signal (when channels are mux'd to TestSignal");
                            ui.label("☑: Test signals are generated internally **Recommended");
                            ui.label("☐: Test signals are driven externally");
                        }).changed() {
                            self.handle_ui_event(Message::InternalCalibration(config.internal_calibration));
                    }

                    if ui
                        .checkbox(
                            &mut config.calibration_amplitude,
                            "2X Calibration Amplitude",
                        )
                        .on_hover_ui(|ui|{
                            ui.label(RichText::new("CONFIG2: CAL_AMP").color(Color32::RED));
                            ui.label("Test signal amplitude");
                            ui.label("(On DCMini, VREFP = 2.5V, VREFN = -2.5V)");
                            ui.label("☑: 4.1666 mV **Recommended");
                            ui.label("☐: 2.0833 mV");
                        }).changed() {
                                        self.handle_ui_event(Message::CalibrationAmplitude(config.calibration_amplitude));
                    }

                    ui.horizontal(|ui| {
                        ui.label("Calibration Frequency:");
                        egui::ComboBox::new("calibration_frequency", "")
                            .selected_text(format!("{:?}", config.calibration_frequency))
                            .show_ui(ui, |ui| {
                                for freq in [
                                    icd::CalFreq::FclkBy21,
                                    icd::CalFreq::FclkBy20,
                                    icd::CalFreq::DoNotUse,
                                    icd::CalFreq::DC,
                                ] {
                                    if ui.selectable_value(
                                        &mut config.calibration_frequency,
                                        freq,
                                        format!("{:?}", freq)
                                    ).clicked() {
                                        self.handle_ui_event(Message::CalibrationFrequency(config.calibration_frequency));
                                    }
                                }
                            })
                    });

                    if ui
                        .checkbox(&mut config.pd_refbuf, "Enable Internal Reference Buffer")
                        .on_hover_ui(|ui|{
                            ui.label(RichText::new("CONFIG3: ~PD_REFBUF").color(Color32::RED));
                            ui.label("Power-down Reference Buffer");
                            ui.label("☑: Enable internal reference buffer **Recommended");
                            ui.label("☐: Power-down internal reference buffer");
                        }).changed() {
                            self.handle_ui_event(Message::PdRefBuf(config.pd_refbuf));
                    }

                    if ui
                        .checkbox(&mut config.bias_meas, "Enable Bias Measurement")
                        .on_hover_ui(|ui|{
                            ui.label(RichText::new("CONFIG3: BIAS_MEAS").color(Color32::RED));
                            ui.label("Enable routing of RldMeasure to channels");
                            ui.label("☑: BIAS_IN signal is routed to (the?) channel(s?) mux'd to RldMeasure");
                            ui.label("☐: Don't route BIAS_IN to any channels");
                        }).changed() {
                            self.handle_ui_event(Message::BiasMeas(config.bias_meas));
                    }

                    if ui
                        .checkbox(&mut config.biasref_int, "Internal Bias Reference Generation")
                        .on_hover_ui(|ui|{
                            ui.label(RichText::new("CONFIG3: BIASREF_INT").color(Color32::RED));
                            ui.label("BIASREF signal source selection");
                            ui.label("☑: Enable internal Bias reference signal @ ~0V **Recommended");
                            ui.label("☐: Bias reference signal is fed externally");
                        }).changed() {
                            self.handle_ui_event(Message::BiasRefInt(config.biasref_int));
                    }

                    if ui
                        .checkbox(&mut config.pd_bias, "Enable Bias")
                        .on_hover_ui(|ui|{
                            ui.label(RichText::new("CONFIG3: ~PD_BIAS").color(Color32::RED));
                            ui.label("Power-down Bias Buffer");
                            ui.label("☑: Enable Bias");
                            ui.label("☐: Power-down Bias Buffer");
                        }).changed() {
                            self.handle_ui_event(Message::PdBias(config.pd_bias));
                    }

                    if ui
                        .checkbox(&mut config.bias_loff_sens, "Enable Bias Lead-Off Sense")
                        .on_hover_ui(|ui|{
                            ui.label(RichText::new("CONFIG3: BIAS_LOFF_SENS").color(Color32::RED));
                            ui.label("Enable Bias Sense function");
                            ui.label("☑: Bias Lead-Off Sensing is Enabled");
                            ui.label("☐: Bias Lead-Off Sensing is Disabled");
                        }).changed() {
                            self.handle_ui_event(Message::BiasLoffSens(config.bias_loff_sens));
                    }

                    if ui
                        .checkbox(&mut config.srb1, "Connect SRB1")
                        .on_hover_ui(|ui|{
                            ui.label(RichText::new("MISC1: SRB1").color(Color32::RED));
                            ui.label("Connect SRB1 to ALL inverting (IN1N, IN2N, ...) inputs.");
                            ui.label("☑: SRB1 Connected to ALL inverting inputs");
                            ui.label("☐: Disconnect SRB1 **Recommended");
                        }).changed() {
                            self.handle_ui_event(Message::Srb1(config.srb1));
                    }

                    if ui
                        .checkbox(&mut config.single_shot, "Single Shot Mode")
                        .on_hover_ui(|ui|{
                            ui.label(RichText::new("CONFIG4: SINGLE_SHOT").color(Color32::RED));
                            ui.label("Set conversion mode");
                            ui.label("☑: Perform a single conversion then stop");
                            ui.label("☐: Enable continuous conversion (streaming) mode **Recommended");
                        }).changed() {
                            self.handle_ui_event(Message::SingleShot(config.single_shot));
                    }

                    if ui
                        .checkbox(
                            &mut config.pd_loff_comp,
                            "Enable Lead-Off Comparators",
                        )
                        .on_hover_ui(|ui|{
                            ui.label(RichText::new("CONFIG4: ~PD_LOFF_COMP").color(Color32::RED));
                            ui.label("Enable Lead-off (channel disconnected) comparators");
                            ui.label("☑: Lead-off comparators enabled");
                            ui.label("☐: Lead-off comparators disabled");
                        }).changed() {
                            self.handle_ui_event(Message::PdLoffComp(config.pd_loff_comp));
                    }
                });

                    // Lead-off settings
                ui.collapsing("Lead-Off Settings", |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Lead-Off Current:");
                        egui::ComboBox::new("lead_off_current", "")
                            .selected_text(format!("{:?}", config.lead_off_current))
                            .show_ui(ui, |ui| {
                                for current in [
                                    icd::ILeadOff::_6nA,
                                    icd::ILeadOff::_24nA,
                                    icd::ILeadOff::_6uA,
                                    icd::ILeadOff::_24uA,
                                ] {
                                        if ui.selectable_value(
                                            &mut config.lead_off_current,
                                            current,
                                            format!("{:?}", current),
                                        ).clicked()
                                        {
                                            self.handle_ui_event(Message::LeadOffCurrent(config.lead_off_current));
                                        }
                                    }})
                    });

                        ui.horizontal(|ui| {
                            ui.label("Lead-Off Frequency:");
                            egui::ComboBox::new("lead_off_freq", "")
                                .selected_text(format!("{:?}", config.lead_off_frequency))
                                .show_ui(ui, |ui| {
                                    for freq in [
                                        icd::FLeadOff::Dc,
                                        icd::FLeadOff::Ac7_8,
                                        icd::FLeadOff::Ac31_2,
                                        icd::FLeadOff::AcFdrBy4,
                                    ] {
                                        if ui.selectable_value(
                                            &mut config.lead_off_frequency,
                                            freq,
                                            format!("{:?}", freq),
                                        ).clicked() {
                                self.handle_ui_event(Message::LeadOffFrequency(config.lead_off_frequency));
                            }
                                    }
                                })
                        });

                        ui.horizontal(|ui| {
                            ui.label("Comparator Threshold:");
                            egui::ComboBox::new("comp_thresh", "")
                                .selected_text(format!("{:?}", config.comparator_threshold_pos))
                                .show_ui(ui, |ui| {
                                    for thresh in [
                                        icd::CompThreshPos::_95,
                                        icd::CompThreshPos::_92_5,
                                        icd::CompThreshPos::_90,
                                        icd::CompThreshPos::_87_5,
                                        icd::CompThreshPos::_85,
                                        icd::CompThreshPos::_80,
                                        icd::CompThreshPos::_75,
                                        icd::CompThreshPos::_70,
                                    ] {
                                        if ui.selectable_value(
                                            &mut config.comparator_threshold_pos,
                                            thresh,
                                            format!("{:?}", thresh),
                                        )                            .clicked() {
                                self.handle_ui_event(Message::ComparatorThresholdPos(config.comparator_threshold_pos));
                            }

                                    }
                                })
                        });
                    });

                    // GPIO Configuration
                    ui.collapsing("GPIO Configuration", |ui| {
                        let mut gpioc = config.gpioc;
                        let mut changed = false;
                        for (i, gpio) in gpioc.iter_mut().enumerate() {
                            if ui
                                .checkbox(gpio, format!("GPIO {} is Input", i))
                                .on_hover_ui(|ui|{
                                    ui.label(RichText::new(format!("GPIO: GPIOC{}", i)).color(Color32::RED));
                                    ui.label(format!("Set if corresponding GPIOD{} is input or output", i));
                                    ui.label(format!("☑: GPIO{} is input", i));
                                    ui.label(format!("☐: GPIO{} is output", i));
                                })
                                .changed() {
                                changed = true;
                            }
                        }
                        if changed {
                            config.gpioc = gpioc;
                            self.handle_ui_event(Message::Gpioc(gpioc));
                        }
                    });

                // Channel Configuration
                for i in 0..config.channels.len() {
                    ui.collapsing(format!("Channel {}", i), |ui| {
                        if ui
                            .checkbox(&mut config.channels[i].power_down, "Disabled")
                            .on_hover_ui(|ui|{
                                ui.label(RichText::new(format!("CH{}SET: PDn", i)).color(Color32::RED));
                                ui.label(format!("Power-down channel {}", i));
                                ui.label("NB: It's recommended to mux disabled channels to InputShorted");
                                ui.label(format!("☑: Disable/Power-down Channel {}", i));
                                ui.label("☐: Normal Operation");
                            })
                            .changed() {
                            self.send_message(Message::PowerDown((i as u8, config.channels[i].power_down)));
                        }

                        ui.horizontal(|ui| {
                            ui.label("Gain:");
                            egui::ComboBox::new(format!("gain_{}", i), "")
                                .selected_text(format!("{:?}", config.channels[i].gain))
                                .show_ui(ui, |ui| {
                                    for g in [
                                        icd::Gain::X1,
                                        icd::Gain::X2,
                                        icd::Gain::X4,
                                        icd::Gain::X6,
                                        icd::Gain::X8,
                                        icd::Gain::X12,
                                        icd::Gain::X24,
                                    ] {
                                        if ui.selectable_value(
                                            &mut config.channels[i].gain,
                                            g,
                                            format!("{:?}", g),
                                        )
                                .clicked() {
                                self.send_message(Message::Gain((i as u8, config.channels[i].gain)));
                            }
                                    }
                                })
                        });

                        ui.horizontal(|ui| {
                            ui.label("Mux:");
                            egui::ComboBox::new(format!("mux_{}", i), "")
                                .selected_text(format!("{:?}", config.channels[i].mux))
                                .show_ui(ui, |ui| {
                                    for m in [
                                        icd::Mux::NormalElectrodeInput,
                                        icd::Mux::InputShorted,
                                        icd::Mux::RldMeasure,
                                        icd::Mux::MVDD,
                                        icd::Mux::TemperatureSensor,
                                        icd::Mux::TestSignal,
                                        icd::Mux::RldDrp,
                                        icd::Mux::RldDrn,
                                    ] {
                                        if ui.selectable_value(
                                            &mut config.channels[i].mux,
                                            m,
                                            format!("{:?}", m),
                                        )
                                .clicked() {
                                self.send_message(Message::Mux((i as u8, config.channels[i].mux)));
                            }
                                    }
                                })
                        });

                        if ui
                            .checkbox(&mut config.channels[i].bias_sensp, "Bias Sense on Positive Input")
                            .on_hover_ui(|ui|{
                                ui.label(RichText::new(format!("BIAS_SENSP: BIASP{}", i)).color(Color32::RED));
                                ui.label(format!("Include Channel {} Positive lead for Bias Calculation", i));
                                ui.label(format!("☑: Add IN{}P to Bias Calculation", i));
                                ui.label(format!("☐: Don't include IN{}P in Bias Calculation", i));
                            })
                            .changed() {
                            self.send_message(Message::BiasSensP((i as u8, config.channels[i].bias_sensp)));
                        }

                        if ui
                            .checkbox(&mut config.channels[i].bias_sensn, "Bias Sense on Negative Input")
                            .on_hover_ui(|ui|{
                                ui.label(RichText::new(format!("BIAS_SENSN: BIASN{}", i)).color(Color32::RED));
                                ui.label(format!("Include Channel {} Negative lead for Bias Calculation", i));
                                ui.label(format!("☑: Add IN{}N to Bias Calculation", i));
                                ui.label(format!("☐: Don't include IN{}N in Bias Calculation", i));
                            })
                            .changed() {
                            self.send_message(Message::BiasSensN((i as u8, config.channels[i].bias_sensn)));
                        }

                        if ui
                            .checkbox(&mut config.channels[i].lead_off_sensp, "Lead-off Sense on Positive Input")
                            .on_hover_ui(|ui|{
                                ui.label(RichText::new(format!("LOFF_SENSP: LOFFP{}", i)).color(Color32::RED));
                                ui.label(format!("☑: Enable Lead-off sensing on IN{}P", i));
                                ui.label(format!("☐: Disable Lead-off sensing on IN{}P", i));
                            })
                            .changed() {
                            self.send_message(Message::LeadOffSensP((i as u8, config.channels[i].lead_off_sensp)));
                        }

                        if ui
                            .checkbox(&mut config.channels[i].lead_off_sensn, "Lead-off Sense on Negative Input")
                            .on_hover_ui(|ui|{
                                ui.label(RichText::new(format!("LOFF_SENSN: LOFFN{}", i)).color(Color32::RED));
                                ui.label(format!("☑: Enable Lead-off sensing on IN{}N", i));
                                ui.label(format!("☐: Disable Lead-off sensing on IN{}N", i));
                            })
                            .changed() {
                            self.send_message(Message::LeadOffSensN((i as u8, config.channels[i].lead_off_sensn)));
                        }

                        if ui
                            .checkbox(&mut config.channels[i].lead_off_flip, "Lead-off Flip")
                            .on_hover_ui(|ui|{
                                ui.label(RichText::new(format!("LOFF_FLIP: LOFFF{}", i)).color(Color32::RED));
                                ui.label(format!("☑: IN{}P is pulled to AVSS and IN{}N pulled to AVDD", i, i));
                                ui.label(format!("☐: IN{}P is pulled to AVDD and IN{}N pulled to AVSS", i, i));
                            })
                            .changed() {
                            self.send_message(Message::LeadOffFlip((i as u8, config.channels[i].lead_off_flip)));
                        }

                        if ui
                            .checkbox(&mut config.channels[i].srb2, "SRB2")
                            .on_hover_ui(|ui|{
                                ui.label(RichText::new(format!("CH{}SET: SRB2", i)).color(Color32::RED));
                                ui.label(format!("Connect SRB2 to positive input (IN{}P); useful for common reference", i));
                                ui.label(format!("☑: Connect SRB2 to IN{}P", i));
                                ui.label(format!("☐: Disconnect SRB2 from IN{}P", i));
                            })
                            .changed() {
                            self.send_message(Message::Srb2((i as u8, config.channels[i].srb2)));
                        }
                    });
                }
                self.config = Some(config);
            } else {
                ui.label(RichText::new("Waiting for configuration...").color(Color32::GRAY));
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

impl Drop for AdsPanel {
    fn drop(&mut self) {
        if let Some(task) = self.client_tx_task.take() {
            task.abort();
        }
        if let Some(task) = self.stream_task.take() {
            task.abort();
        }
    }
}
