use super::{gatt::Server, ATT_MTU};
use crate::prelude::{info, unwrap};
use crate::tasks::ble::ads_stream::{self, AdsStreamNotifier};
use dc_mini_icd::{AdsConfig, ADS_MAX_CHANNELS};
use heapless::Vec;
use trouble_host::prelude::*;

#[gatt_service(uuid = "32100000-af46-43af-a0ba-4dbeb457f51c")]
pub struct AdsService {
    #[characteristic(
        uuid = "32000000-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    pub daisy_en: bool,
    #[characteristic(
        uuid = "32000001-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    pub clk_en: bool,
    #[characteristic(
        uuid = "32000002-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    pub sample_rate: u8,
    #[characteristic(
        uuid = "32000003-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    pub internal_calibration: bool,
    #[characteristic(
        uuid = "32000004-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    pub calibration_amplitude: bool,
    #[characteristic(
        uuid = "32000005-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    pub calibration_frequency: u8,
    #[characteristic(
        uuid = "32000006-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    pub pd_refbuf: bool,
    #[characteristic(
        uuid = "32000007-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    pub bias_meas: bool,
    #[characteristic(
        uuid = "32000008-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    pub biasref_int: bool,
    #[characteristic(
        uuid = "32000009-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    pub pd_bias: bool,
    #[characteristic(
        uuid = "3200000a-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    pub bias_loff_sens: bool,
    #[characteristic(
        uuid = "3200000b-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    pub bias_stat: bool,
    #[characteristic(
        uuid = "3200000c-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    pub comparator_threshold_pos: u8,
    #[characteristic(
        uuid = "3200000d-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    pub lead_off_current: u8,
    #[characteristic(
        uuid = "3200000e-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    pub lead_off_frequency: u8,
    // #[characteristic(
    //     uuid = "b457f51c-af46-43af-a0ba-4dbe3200000f",
    //     read,
    //     write
    // )]
    // gpioc: [bool; 4],
    #[characteristic(
        uuid = "32000010-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    pub srb1: bool,
    #[characteristic(
        uuid = "32000011-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    pub single_shot: bool,
    #[characteristic(
        uuid = "32000012-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    pub pd_loff_comp: bool,
    #[characteristic(
        uuid = "32000100-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    pub power_down: Vec<u8, ADS_MAX_CHANNELS>,
    #[characteristic(
        uuid = "32000101-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    pub gain: Vec<u8, ADS_MAX_CHANNELS>,
    #[characteristic(
        uuid = "32000102-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    pub srb2: Vec<u8, ADS_MAX_CHANNELS>,
    #[characteristic(
        uuid = "32000103-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    pub mux: Vec<u8, ADS_MAX_CHANNELS>,
    #[characteristic(
        uuid = "32000104-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    pub bias_sensp: Vec<u8, ADS_MAX_CHANNELS>,
    #[characteristic(
        uuid = "32000105-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    pub bias_sensn: Vec<u8, ADS_MAX_CHANNELS>,
    #[characteristic(
        uuid = "32000106-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    pub lead_off_sensp: Vec<u8, ADS_MAX_CHANNELS>,
    #[characteristic(
        uuid = "32000107-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    pub lead_off_sensn: Vec<u8, ADS_MAX_CHANNELS>,
    #[characteristic(
        uuid = "32000108-af46-43af-a0ba-4dbeb457f51c",
        read,
        write
    )]
    pub lead_off_flip: Vec<u8, ADS_MAX_CHANNELS>,
    #[characteristic(
        uuid = "32000200-af46-43af-a0ba-4dbeb457f51c",
        read,
        notify
    )]
    pub data_stream: Vec<u8, ATT_MTU>,
    #[characteristic(uuid = "32000300-af46-43af-a0ba-4dbeb457f51c", write)]
    pub command: u8,
}

/// Notifier that holds only the characteristic handle (Copy) and a borrow
/// of the GattConnection. No reference to Server needed, avoiding coupled lifetimes.
struct TroubleNotifier<'a, 'b, 'c, P: PacketPool> {
    handle: Characteristic<Vec<u8, ATT_MTU>>,
    conn: &'a GattConnection<'b, 'c, P>,
}

impl<P: PacketPool> AdsStreamNotifier for TroubleNotifier<'_, '_, '_, P> {
    async fn notify_data_stream(
        &self,
        data: &Vec<u8, ATT_MTU>,
    ) -> Result<(), super::Error> {
        self.handle.notify(self.conn, data).await?;
        Ok(())
    }
}

pub async fn ads_stream_notify<P: PacketPool>(
    server: &Server<'_>,
    conn: &GattConnection<'_, '_, P>,
) {
    let notifier =
        TroubleNotifier { handle: server.ads.data_stream.clone(), conn };

    // Wait for ATT MTU exchange to complete before querying the negotiated value.
    embassy_time::Timer::after_secs(1).await;

    let att_mtu = conn.raw().att_mtu() as usize;
    // Subtract ATT notification header (1 opcode + 2 handle) to get max value size.
    let mtu = att_mtu - 3;
    info!("ADS ATT mtu = {}, max notify value = {}", att_mtu, mtu);

    ads_stream::ads_stream_notify(&notifier, mtu).await
}

pub async fn update_ads_characteristics(
    server: &Server<'_>,
    config: &AdsConfig,
) {
    fn channel_values(
        config: &AdsConfig,
        map: impl Fn(&dc_mini_icd::ChannelConfig) -> u8,
    ) -> Vec<u8, ADS_MAX_CHANNELS> {
        let mut values = Vec::new();
        for channel in config.channels.iter() {
            unwrap!(values.push(map(channel)));
        }
        values
    }

    unwrap!(server.set(&server.ads.daisy_en, &config.daisy_en));
    unwrap!(server.set(&server.ads.clk_en, &config.clk_en));
    unwrap!(server.set(&server.ads.sample_rate, &(config.sample_rate as u8),));
    unwrap!(server
        .set(&server.ads.internal_calibration, &config.internal_calibration,));
    unwrap!(server.set(
        &server.ads.calibration_amplitude,
        &config.calibration_amplitude,
    ));
    unwrap!(server.set(
        &server.ads.calibration_frequency,
        &(config.calibration_frequency as u8),
    ));
    unwrap!(server.set(&server.ads.pd_refbuf, &config.pd_refbuf));
    unwrap!(server.set(&server.ads.bias_meas, &config.bias_meas));
    unwrap!(server.set(&server.ads.biasref_int, &config.biasref_int));
    unwrap!(server.set(&server.ads.pd_bias, &config.pd_bias));
    unwrap!(server.set(&server.ads.bias_loff_sens, &config.bias_loff_sens,));
    unwrap!(server.set(&server.ads.bias_stat, &config.bias_stat));
    unwrap!(server.set(
        &server.ads.comparator_threshold_pos,
        &(config.comparator_threshold_pos as u8),
    ));
    unwrap!(server
        .set(&server.ads.lead_off_current, &(config.lead_off_current as u8),));
    unwrap!(server.set(
        &server.ads.lead_off_frequency,
        &(config.lead_off_frequency as u8),
    ));
    unwrap!(server.set(&server.ads.srb1, &config.srb1));
    unwrap!(server.set(&server.ads.single_shot, &config.single_shot));
    unwrap!(server.set(&server.ads.pd_loff_comp, &config.pd_loff_comp));

    unwrap!(server.set(
        &server.ads.power_down,
        &channel_values(config, |ch| ch.power_down as u8),
    ));
    unwrap!(server
        .set(&server.ads.gain, &channel_values(config, |ch| ch.gain as u8),));
    unwrap!(server
        .set(&server.ads.srb2, &channel_values(config, |ch| ch.srb2 as u8),));
    unwrap!(server
        .set(&server.ads.mux, &channel_values(config, |ch| ch.mux as u8),));
    unwrap!(server.set(
        &server.ads.bias_sensp,
        &channel_values(config, |ch| ch.bias_sensp as u8),
    ));
    unwrap!(server.set(
        &server.ads.bias_sensn,
        &channel_values(config, |ch| ch.bias_sensn as u8),
    ));
    unwrap!(server.set(
        &server.ads.lead_off_sensp,
        &channel_values(config, |ch| ch.lead_off_sensp as u8),
    ));
    unwrap!(server.set(
        &server.ads.lead_off_sensn,
        &channel_values(config, |ch| ch.lead_off_sensn as u8),
    ));
    unwrap!(server.set(
        &server.ads.lead_off_flip,
        &channel_values(config, |ch| ch.lead_off_flip as u8),
    ));
}
