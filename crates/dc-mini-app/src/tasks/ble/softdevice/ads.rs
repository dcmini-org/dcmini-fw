extern crate alloc;

use super::{
    super::ads_stream::{self, AdsStreamNotifier},
    Server, ATT_MTU,
};
use crate::prelude::*;
use embassy_futures::select::select;
use embassy_sync::{channel::Receiver, signal::Signal};
use heapless::Vec;
use nrf_softdevice::ble::Connection;

static NOTIFICATIONS_STATUS: Signal<ThreadModeRawMutex, bool> = Signal::new();

#[nrf_softdevice::gatt_service(uuid = "32100000-af46-43af-a0ba-4dbeb457f51c")]
pub struct AdsService {
    #[characteristic(
        uuid = "32000000-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    daisy_en: bool,
    #[characteristic(
        uuid = "32000001-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    clk_en: bool,
    #[characteristic(
        uuid = "32000002-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    sample_rate: u8,
    #[characteristic(
        uuid = "32000003-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    internal_calibration: bool,
    #[characteristic(
        uuid = "32000004-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    calibration_amplitude: bool,
    #[characteristic(
        uuid = "32000005-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    calibration_frequency: u8,
    #[characteristic(
        uuid = "32000006-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    pd_refbuf: bool,
    #[characteristic(
        uuid = "32000007-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    bias_meas: bool,
    #[characteristic(
        uuid = "32000008-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    biasref_int: bool,
    #[characteristic(
        uuid = "32000009-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    pd_bias: bool,
    #[characteristic(
        uuid = "3200000a-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    bias_loff_sens: bool,
    #[characteristic(
        uuid = "3200000b-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    bias_stat: bool,
    #[characteristic(
        uuid = "3200000c-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    comparator_threshold_pos: u8,
    #[characteristic(
        uuid = "3200000d-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    lead_off_current: u8,
    #[characteristic(
        uuid = "3200000e-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    lead_off_frequency: u8,
    #[characteristic(
        uuid = "32000010-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    srb1: bool,
    #[characteristic(
        uuid = "32000011-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    single_shot: bool,
    #[characteristic(
        uuid = "32000012-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    pd_loff_comp: bool,
    #[characteristic(
        uuid = "32000100-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    power_down: Vec<u8, ADS_MAX_CHANNELS>,
    #[characteristic(
        uuid = "32000101-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    gain: Vec<u8, ADS_MAX_CHANNELS>,
    #[characteristic(
        uuid = "32000102-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    srb2: Vec<u8, ADS_MAX_CHANNELS>,
    #[characteristic(
        uuid = "32000103-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    mux: Vec<u8, ADS_MAX_CHANNELS>,
    #[characteristic(
        uuid = "32000104-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    bias_sensp: Vec<u8, ADS_MAX_CHANNELS>,
    #[characteristic(
        uuid = "32000105-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    bias_sensn: Vec<u8, ADS_MAX_CHANNELS>,
    #[characteristic(
        uuid = "32000106-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    lead_off_sensp: Vec<u8, ADS_MAX_CHANNELS>,
    #[characteristic(
        uuid = "32000107-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    lead_off_sensn: Vec<u8, ADS_MAX_CHANNELS>,
    #[characteristic(
        uuid = "32000108-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    lead_off_flip: Vec<u8, ADS_MAX_CHANNELS>,
    #[characteristic(
        uuid = "32000200-af46-43af-a0ba-4dbeb457f51c",
        read,
        notify
    )]
    data_stream: Vec<u8, ATT_MTU>,
    #[characteristic(uuid = "32000300-af46-43af-a0ba-4dbeb457f51c", write)]
    command: u8,
}

macro_rules! handle_event {
    // Single-field update in AdsConfig
    (single, $value:expr, $app_context:expr, $field:ident) => {{
        let mut context = $app_context.lock().await;
        let mut config =
            context.profile_manager.get_ads_config().await.unwrap().clone();
        config.$field = $value;
        context.save_ads_config(config).await;
    }};
    // Single-field update with a transform
    (single, $value:expr, $app_context:expr, $field:ident, $transform:ty) => {{
        let mut context = $app_context.lock().await;
        let mut config =
            context.profile_manager.get_ads_config().await.unwrap().clone();
        config.$field = <$transform>::from($value);
        context.save_ads_config(config).await;
    }};
    // Multi-channel field update in AdsConfig::channels
    (multi, $values:expr, $app_context:expr, $channel_field:ident) => {{
        let mut context = $app_context.lock().await;
        let mut config =
            context.profile_manager.get_ads_config().await.unwrap().clone();
        for (i, &value) in $values.iter().enumerate() {
            if let Some(channel) = config.channels.get_mut(i) {
                channel.$channel_field = value != 0; // Convert `u8` to `bool`
            }
        }
        context.save_ads_config(config).await;
    }};
    // Multi-channel field update with a transform
    (multi, $values:expr, $app_context:expr, $channel_field:ident, $transform:ty) => {{
        let mut context = $app_context.lock().await;
        let mut config =
            context.profile_manager.get_ads_config().await.unwrap().clone();
        for (i, &value) in $values.iter().enumerate() {
            if let Some(channel) = config.channels.get_mut(i) {
                channel.$channel_field = <$transform>::from(value);
            }
        }
        context.save_ads_config(config).await;
    }};
}

impl AdsService {
    pub async fn handle(
        &self,
        rx: Receiver<'_, NoopRawMutex, AdsServiceEvent, 10>,
        app_context: &'static Mutex<CriticalSectionRawMutex, AppContext>,
    ) {
        use dc_mini_icd::{
            CalFreq, CompThreshPos, FLeadOff, Gain, ILeadOff, Mux, SampleRate,
        };
        let evt_sender = {
            let ctx = app_context.lock().await;
            ctx.event_sender
        };
        loop {
            let event = rx.receive().await;
            match event {
                // Single-field updates
                AdsServiceEvent::DaisyEnWrite(value) => {
                    handle_event!(single, value, app_context, daisy_en)
                }
                AdsServiceEvent::ClkEnWrite(value) => {
                    handle_event!(single, value, app_context, clk_en)
                }
                AdsServiceEvent::SampleRateWrite(value) => {
                    handle_event!(
                        single,
                        value,
                        app_context,
                        sample_rate,
                        SampleRate
                    )
                }
                AdsServiceEvent::InternalCalibrationWrite(value) => {
                    handle_event!(
                        single,
                        value,
                        app_context,
                        internal_calibration
                    )
                }
                AdsServiceEvent::CalibrationAmplitudeWrite(value) => {
                    handle_event!(
                        single,
                        value,
                        app_context,
                        calibration_amplitude
                    )
                }
                AdsServiceEvent::CalibrationFrequencyWrite(value) => {
                    handle_event!(
                        single,
                        value,
                        app_context,
                        calibration_frequency,
                        CalFreq
                    )
                }
                AdsServiceEvent::PdRefbufWrite(value) => {
                    handle_event!(single, value, app_context, pd_refbuf)
                }
                AdsServiceEvent::BiasMeasWrite(value) => {
                    handle_event!(single, value, app_context, bias_meas)
                }
                AdsServiceEvent::BiasrefIntWrite(value) => {
                    handle_event!(single, value, app_context, biasref_int)
                }
                AdsServiceEvent::PdBiasWrite(value) => {
                    handle_event!(single, value, app_context, pd_bias)
                }
                AdsServiceEvent::BiasLoffSensWrite(value) => {
                    handle_event!(single, value, app_context, bias_loff_sens)
                }
                AdsServiceEvent::BiasStatWrite(value) => {
                    handle_event!(single, value, app_context, bias_stat)
                }
                AdsServiceEvent::ComparatorThresholdPosWrite(value) => {
                    handle_event!(
                        single,
                        value,
                        app_context,
                        comparator_threshold_pos,
                        CompThreshPos
                    )
                }
                AdsServiceEvent::LeadOffCurrentWrite(value) => {
                    handle_event!(
                        single,
                        value,
                        app_context,
                        lead_off_current,
                        ILeadOff
                    )
                }
                AdsServiceEvent::LeadOffFrequencyWrite(value) => {
                    handle_event!(
                        single,
                        value,
                        app_context,
                        lead_off_frequency,
                        FLeadOff
                    )
                }
                AdsServiceEvent::Srb1Write(value) => {
                    handle_event!(single, value, app_context, srb1)
                }
                AdsServiceEvent::SingleShotWrite(value) => {
                    handle_event!(single, value, app_context, single_shot)
                }
                AdsServiceEvent::PdLoffCompWrite(value) => {
                    handle_event!(single, value, app_context, pd_loff_comp)
                }

                // Multi-channel updates
                AdsServiceEvent::PowerDownWrite(values) => {
                    handle_event!(multi, values, app_context, power_down)
                }
                AdsServiceEvent::GainWrite(values) => {
                    handle_event!(multi, values, app_context, gain, Gain)
                }
                AdsServiceEvent::Srb2Write(values) => {
                    handle_event!(multi, values, app_context, srb2)
                }
                AdsServiceEvent::MuxWrite(values) => {
                    handle_event!(multi, values, app_context, mux, Mux)
                }
                AdsServiceEvent::BiasSenspWrite(values) => {
                    handle_event!(multi, values, app_context, bias_sensp)
                }
                AdsServiceEvent::BiasSensnWrite(values) => {
                    handle_event!(multi, values, app_context, bias_sensn)
                }
                AdsServiceEvent::LeadOffSenspWrite(values) => {
                    handle_event!(multi, values, app_context, lead_off_sensp)
                }
                AdsServiceEvent::LeadOffSensnWrite(values) => {
                    handle_event!(multi, values, app_context, lead_off_sensn)
                }
                AdsServiceEvent::LeadOffFlipWrite(values) => {
                    handle_event!(multi, values, app_context, lead_off_flip)
                }

                // Handle commands
                AdsServiceEvent::CommandWrite(value) => {
                    let evt = AdsEvent::try_from(value);
                    match evt {
                        Ok(e) => evt_sender.send(e.into()).await,
                        Err(e) => warn!("{:?}", e),
                    };
                }

                AdsServiceEvent::DataStreamCccdWrite { notifications } => {
                    info!("Client notifications = {:?}", notifications);
                    NOTIFICATIONS_STATUS.signal(notifications);
                }
            }
        }
    }
}

pub async fn update_ads_characteristics(
    app_context: &'static Mutex<CriticalSectionRawMutex, AppContext>,
) {
    let mut app_ctx = app_context.lock().await;
    let server = app_ctx.ble_server;
    let profile_manager = &mut app_ctx.profile_manager;
    let ads_config = unwrap!(profile_manager.get_ads_config().await).clone();

    unwrap!(server.ads.daisy_en_set(&ads_config.daisy_en));
    unwrap!(server.ads.clk_en_set(&ads_config.clk_en));
    unwrap!(server.ads.sample_rate_set(&ads_config.sample_rate.into()));
    unwrap!(server
        .ads
        .internal_calibration_set(&ads_config.internal_calibration));
    unwrap!(server
        .ads
        .calibration_amplitude_set(&ads_config.calibration_amplitude));
    unwrap!(server
        .ads
        .calibration_frequency_set(&ads_config.calibration_frequency.into()));
    unwrap!(server.ads.pd_refbuf_set(&ads_config.pd_refbuf));
    unwrap!(server.ads.bias_meas_set(&ads_config.bias_meas));
    unwrap!(server.ads.biasref_int_set(&ads_config.biasref_int));
    unwrap!(server.ads.pd_bias_set(&ads_config.pd_bias));
    unwrap!(server.ads.bias_loff_sens_set(&ads_config.bias_loff_sens));
    unwrap!(server.ads.bias_stat_set(&ads_config.bias_stat));
    unwrap!(server.ads.comparator_threshold_pos_set(
        &ads_config.comparator_threshold_pos.into()
    ));
    unwrap!(server
        .ads
        .lead_off_current_set(&ads_config.lead_off_current.into()));
    unwrap!(server
        .ads
        .lead_off_frequency_set(&ads_config.lead_off_frequency.into()));
    unwrap!(server.ads.srb1_set(&ads_config.srb1.into()));
    unwrap!(server.ads.single_shot_set(&ads_config.single_shot.into()));
    unwrap!(server.ads.pd_loff_comp_set(&ads_config.pd_loff_comp.into()));

    // Handle channel-specific characteristics with heapless::Vec
    let mut power_down = Vec::new();
    let mut gain = Vec::new();
    let mut srb2 = Vec::new();
    let mut mux = Vec::new();
    let mut bias_sensp = Vec::new();
    let mut bias_sensn = Vec::new();
    let mut lead_off_sensp = Vec::new();
    let mut lead_off_sensn = Vec::new();
    let mut lead_off_flip = Vec::new();

    for channel in ads_config.channels.iter() {
        power_down.push(channel.power_down.into()).unwrap();
        gain.push(channel.gain.into()).unwrap();
        srb2.push(channel.srb2.into()).unwrap();
        mux.push(channel.mux.into()).unwrap();
        bias_sensp.push(channel.bias_sensp.into()).unwrap();
        bias_sensn.push(channel.bias_sensn.into()).unwrap();
        lead_off_sensp.push(channel.lead_off_sensp.into()).unwrap();
        lead_off_sensn.push(channel.lead_off_sensn.into()).unwrap();
        lead_off_flip.push(channel.lead_off_flip.into()).unwrap();
    }

    // Set channel-specific fields on the GATT server
    unwrap!(server.ads.gain_set(&gain));
    unwrap!(server.ads.power_down_set(&power_down));
    unwrap!(server.ads.srb2_set(&srb2));
    unwrap!(server.ads.mux_set(&mux));
    unwrap!(server.ads.bias_sensp_set(&bias_sensp));
    unwrap!(server.ads.bias_sensn_set(&bias_sensn));
    unwrap!(server.ads.lead_off_sensp_set(&lead_off_sensp));
    unwrap!(server.ads.lead_off_sensn_set(&lead_off_sensn));
    unwrap!(server.ads.lead_off_flip_set(&lead_off_flip));
}

struct SoftdeviceNotifier<'a> {
    server: &'a Server,
    connection: &'a Connection,
}

impl<'a> AdsStreamNotifier for SoftdeviceNotifier<'a> {
    async fn notify_data_stream(
        &self,
        data: &Vec<u8, ATT_MTU>,
    ) -> Result<(), super::Error> {
        match self.server.ads.data_stream_notify(self.connection, data) {
            Ok(_) => Ok(()),
            Err(_) => {
                // If notification fails, attempt to write directly
                self.server.ads.data_stream_set(data)?;
                Ok(())
            }
        }
    }
}

pub async fn ads_stream_notify(server: &Server, connection: &Connection) {
    let notifier = SoftdeviceNotifier { server, connection };

    // Let's give our gatt server a bit of time to negotiate MTU size.
    Timer::after_secs(1).await;
    let mtu = connection.att_mtu() as usize;
    info!("MTU negotiated to be {:?}", mtu);

    loop {
        let status = NOTIFICATIONS_STATUS.wait().await;
        if status {
            NOTIFICATIONS_STATUS.reset();
            select(
                ads_stream::ads_stream_notify(&notifier, mtu),
                NOTIFICATIONS_STATUS.wait(),
            )
            .await;
        }
    }
}
