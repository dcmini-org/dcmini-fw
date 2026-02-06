use dc_mini_icd::{
    self as icd, CalFreq, CompThreshPos, FLeadOff, ILeadOff, SampleRate,
};
use futures::Stream;
use futures_lite::StreamExt;
use std::error::Error;
use std::vec::Vec;

mod uuids {
    // Service UUIDs
    pub const ADS_SERVICE_UUID: bluest::Uuid =
        bluest::Uuid::from_u128(0x32100000_af46_43af_a0ba_4dbeb457f51c);
    pub const BATTERY_SERVICE_UUID: bluest::Uuid =
        bluest::btuuid::services::BATTERY;
    pub const DEVICE_INFO_SERVICE_UUID: bluest::Uuid =
        bluest::btuuid::services::DEVICE_INFORMATION;
    pub const PROFILE_SERVICE_UUID: bluest::Uuid =
        bluest::Uuid::from_u128(0x32300000_af46_43af_a0ba_4dbeb457f51c);
    pub const SESSION_SERVICE_UUID: bluest::Uuid =
        bluest::Uuid::from_u128(0x32200000_af46_43af_a0ba_4dbeb457f51c);
    pub const MIC_SERVICE_UUID: bluest::Uuid =
        bluest::Uuid::from_u128(0x33100000_af46_43af_a0ba_4dbeb457f51c);

    // Battery Service Characteristics
    pub const BATTERY_LEVEL_UUID: bluest::Uuid =
        bluest::btuuid::characteristics::BATTERY_LEVEL;

    // Device Info Service Characteristics
    pub const HARDWARE_REV_UUID: bluest::Uuid =
        bluest::btuuid::characteristics::HARDWARE_REVISION_STRING;
    pub const SOFTWARE_REV_UUID: bluest::Uuid =
        bluest::btuuid::characteristics::SOFTWARE_REVISION_STRING;
    pub const MANUFACTURER_NAME_UUID: bluest::Uuid =
        bluest::btuuid::characteristics::MANUFACTURER_NAME_STRING;

    // Profile Service Characteristics
    pub const PROFILE_CURRENT_UUID: bluest::Uuid =
        bluest::Uuid::from_u128(0x32300001_af46_43af_a0ba_4dbeb457f51c);
    pub const PROFILE_COMMAND_UUID: bluest::Uuid =
        bluest::Uuid::from_u128(0x32300002_af46_43af_a0ba_4dbeb457f51c);

    // Session Service Characteristics
    pub const SESSION_ID_UUID: bluest::Uuid =
        bluest::Uuid::from_u128(0x32200001_af46_43af_a0ba_4dbeb457f51c);
    pub const SESSION_STATUS_UUID: bluest::Uuid =
        bluest::Uuid::from_u128(0x32200002_af46_43af_a0ba_4dbeb457f51c);
    pub const SESSION_CMD_UUID: bluest::Uuid =
        bluest::Uuid::from_u128(0x32200004_af46_43af_a0ba_4dbeb457f51c);

    // Mic Service Characteristics
    pub mod mic {
        pub const GAIN_DB_UUID: bluest::Uuid =
            bluest::Uuid::from_u128(0x33000000_af46_43af_a0ba_4dbeb457f51c);
        pub const SAMPLE_RATE_UUID: bluest::Uuid =
            bluest::Uuid::from_u128(0x33000001_af46_43af_a0ba_4dbeb457f51c);
        pub const DATA_STREAM_UUID: bluest::Uuid =
            bluest::Uuid::from_u128(0x33000200_af46_43af_a0ba_4dbeb457f51c);
        pub const COMMAND_UUID: bluest::Uuid =
            bluest::Uuid::from_u128(0x33000300_af46_43af_a0ba_4dbeb457f51c);
    }

    // ADS Service Characteristics
    pub mod ads {
        // Characteristic UUIDs
        pub const DAISY_EN_UUID: bluest::Uuid =
            bluest::Uuid::from_u128(0x32000000_af46_43af_a0ba_4dbeb457f51c);
        pub const CLK_EN_UUID: bluest::Uuid =
            bluest::Uuid::from_u128(0x32000001_af46_43af_a0ba_4dbeb457f51c);
        pub const SAMPLE_RATE_UUID: bluest::Uuid =
            bluest::Uuid::from_u128(0x32000002_af46_43af_a0ba_4dbeb457f51c);
        pub const INTERNAL_CALIBRATION_UUID: bluest::Uuid =
            bluest::Uuid::from_u128(0x32000003_af46_43af_a0ba_4dbeb457f51c);
        pub const CALIBRATION_AMPLITUDE_UUID: bluest::Uuid =
            bluest::Uuid::from_u128(0x32000004_af46_43af_a0ba_4dbeb457f51c);
        pub const CALIBRATION_FREQUENCY_UUID: bluest::Uuid =
            bluest::Uuid::from_u128(0x32000005_af46_43af_a0ba_4dbeb457f51c);
        pub const PD_REFBUF_UUID: bluest::Uuid =
            bluest::Uuid::from_u128(0x32000006_af46_43af_a0ba_4dbeb457f51c);
        pub const BIAS_MEAS_UUID: bluest::Uuid =
            bluest::Uuid::from_u128(0x32000007_af46_43af_a0ba_4dbeb457f51c);
        pub const BIASREF_INT_UUID: bluest::Uuid =
            bluest::Uuid::from_u128(0x32000008_af46_43af_a0ba_4dbeb457f51c);
        pub const PD_BIAS_UUID: bluest::Uuid =
            bluest::Uuid::from_u128(0x32000009_af46_43af_a0ba_4dbeb457f51c);
        pub const BIAS_LOFF_SENS_UUID: bluest::Uuid =
            bluest::Uuid::from_u128(0x3200000a_af46_43af_a0ba_4dbeb457f51c);
        pub const BIAS_STAT_UUID: bluest::Uuid =
            bluest::Uuid::from_u128(0x3200000b_af46_43af_a0ba_4dbeb457f51c);
        pub const COMPARATOR_THRESHOLD_POS_UUID: bluest::Uuid =
            bluest::Uuid::from_u128(0x3200000c_af46_43af_a0ba_4dbeb457f51c);
        pub const LEAD_OFF_CURRENT_UUID: bluest::Uuid =
            bluest::Uuid::from_u128(0x3200000d_af46_43af_a0ba_4dbeb457f51c);
        pub const LEAD_OFF_FREQUENCY_UUID: bluest::Uuid =
            bluest::Uuid::from_u128(0x3200000e_af46_43af_a0ba_4dbeb457f51c);
        pub const SRB1_UUID: bluest::Uuid =
            bluest::Uuid::from_u128(0x32000010_af46_43af_a0ba_4dbeb457f51c);
        pub const SINGLE_SHOT_UUID: bluest::Uuid =
            bluest::Uuid::from_u128(0x32000011_af46_43af_a0ba_4dbeb457f51c);
        pub const PD_LOFF_COMP_UUID: bluest::Uuid =
            bluest::Uuid::from_u128(0x32000012_af46_43af_a0ba_4dbeb457f51c);
        // Channel characteristics
        pub const POWER_DOWN_UUID: bluest::Uuid =
            bluest::Uuid::from_u128(0x32000100_af46_43af_a0ba_4dbeb457f51c);
        pub const GAIN_UUID: bluest::Uuid =
            bluest::Uuid::from_u128(0x32000101_af46_43af_a0ba_4dbeb457f51c);
        pub const SRB2_UUID: bluest::Uuid =
            bluest::Uuid::from_u128(0x32000102_af46_43af_a0ba_4dbeb457f51c);
        pub const MUX_UUID: bluest::Uuid =
            bluest::Uuid::from_u128(0x32000103_af46_43af_a0ba_4dbeb457f51c);
        pub const BIAS_SENSP_UUID: bluest::Uuid =
            bluest::Uuid::from_u128(0x32000104_af46_43af_a0ba_4dbeb457f51c);
        pub const BIAS_SENSN_UUID: bluest::Uuid =
            bluest::Uuid::from_u128(0x32000105_af46_43af_a0ba_4dbeb457f51c);
        pub const LEAD_OFF_SENSP_UUID: bluest::Uuid =
            bluest::Uuid::from_u128(0x32000106_af46_43af_a0ba_4dbeb457f51c);
        pub const LEAD_OFF_SENSN_UUID: bluest::Uuid =
            bluest::Uuid::from_u128(0x32000107_af46_43af_a0ba_4dbeb457f51c);
        pub const LEAD_OFF_FLIP_UUID: bluest::Uuid =
            bluest::Uuid::from_u128(0x32000108_af46_43af_a0ba_4dbeb457f51c);

        // Data and command characteristics
        pub const DATA_STREAM_UUID: bluest::Uuid =
            bluest::Uuid::from_u128(0x32000200_af46_43af_a0ba_4dbeb457f51c);
        pub const COMMAND_UUID: bluest::Uuid =
            bluest::Uuid::from_u128(0x32000300_af46_43af_a0ba_4dbeb457f51c);
    }
}

use uuids::ads::*;

/// BLE client for communicating with the device
pub struct BleClient {
    pub device: bluest::Device,
    characteristics: Vec<bluest::Characteristic>,
    adapter: bluest::Adapter,
}

impl BleClient {
    pub async fn try_new(
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let adapter = bluest::Adapter::default()
            .await
            .ok_or("Bluetooth adapter not found")?;
        println!("Waiting for adapter!");
        adapter.wait_available().await?;

        println!("Discovering devices!");
        let mut devices = adapter
            .discover_devices(&[
                uuids::ADS_SERVICE_UUID,
                uuids::PROFILE_SERVICE_UUID,
                uuids::SESSION_SERVICE_UUID,
                uuids::MIC_SERVICE_UUID,
                // uuids::BATTERY_SERVICE_UUID,
                // uuids::DEVICE_INFO_SERVICE_UUID,
            ])
            .await?;

        let device = devices
            .next()
            .await
            .ok_or("No devices found")?
            .map_err(|e| format!("Device error: {:?}", e))?;

        println!(
            "Found device: {} ({:?})",
            device.name().as_deref().unwrap_or("(unknown)"),
            device.id()
        );

        adapter.connect_device(&device).await?;
        println!("Connected!");

        let mut characteristics = Vec::new();

        // Discover characteristics for each service
        for service_uuid in [
            uuids::ADS_SERVICE_UUID,
            uuids::BATTERY_SERVICE_UUID,
            uuids::DEVICE_INFO_SERVICE_UUID,
            uuids::PROFILE_SERVICE_UUID,
            uuids::SESSION_SERVICE_UUID,
            uuids::MIC_SERVICE_UUID,
        ] {
            if let Ok(service) =
                device.discover_services_with_uuid(service_uuid).await
            {
                if let Some(service) = service.first() {
                    if let Ok(chars) = service.characteristics().await {
                        characteristics.extend(chars);
                    }
                }
            }
        }

        println!("Discovered {} characteristics", characteristics.len());

        Ok(Self { device, characteristics, adapter: adapter.clone() })
    }

    pub async fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>>
    {
        Self::try_new().await
    }

    pub async fn notify_ads_stream(
        &self,
    ) -> impl Stream<Item = bluest::Result<Vec<u8>>> + Send + Unpin + use<'_>
    {
        let characteristic = self
            .get_characteristic(DATA_STREAM_UUID)
            .ok_or("Data stream characteristic not found")
            .unwrap();
        let stream = characteristic.notify().await.unwrap();
        stream
    }

    fn get_characteristic(
        &self,
        uuid: bluest::Uuid,
    ) -> Option<&bluest::Characteristic> {
        self.characteristics.iter().find(|x| x.uuid() == uuid)
    }

    async fn read_characteristic(
        &self,
        uuid: bluest::Uuid,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        let characteristic =
            self.get_characteristic(uuid).ok_or("Characteristic not found")?;
        Ok(characteristic.read().await?)
    }

    async fn write_characteristic(
        &self,
        uuid: bluest::Uuid,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let characteristic =
            self.get_characteristic(uuid).ok_or("Characteristic not found")?;
        characteristic.write(data).await?;
        Ok(())
    }

    // Battery Service Methods
    pub async fn get_battery_level(
        &self,
    ) -> Result<icd::BatteryLevel, Box<dyn std::error::Error + Send + Sync>>
    {
        let level =
            self.read_characteristic(uuids::BATTERY_LEVEL_UUID).await?[0];
        Ok(icd::BatteryLevel(level))
    }

    // Device Info Service Methods
    pub async fn get_device_info(
        &self,
    ) -> Result<icd::DeviceInfo, Box<dyn std::error::Error + Send + Sync>>
    {
        let hw_rev = heapless::String::from_utf8(
            heapless::Vec::from_slice(
                self.read_characteristic(uuids::HARDWARE_REV_UUID)
                    .await?
                    .as_slice(),
            )
            .unwrap(),
        )?;

        let sw_rev = heapless::String::from_utf8(
            heapless::Vec::from_slice(
                self.read_characteristic(uuids::SOFTWARE_REV_UUID)
                    .await?
                    .as_slice(),
            )
            .unwrap(),
        )?;

        let mfr_name = heapless::String::from_utf8(
            heapless::Vec::from_slice(
                self.read_characteristic(uuids::MANUFACTURER_NAME_UUID)
                    .await?
                    .as_slice(),
            )
            .unwrap(),
        )?;

        Ok(icd::DeviceInfo {
            hardware_revision: hw_rev,
            software_revision: sw_rev,
            manufacturer_name: mfr_name,
        })
    }

    // Profile Service Methods
    pub async fn get_profile(
        &self,
    ) -> Result<u8, Box<dyn std::error::Error + Send + Sync>> {
        let profile =
            self.read_characteristic(uuids::PROFILE_CURRENT_UUID).await?[0];
        Ok(profile)
    }

    pub async fn set_profile(
        &self,
        profile: u8,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.write_characteristic(uuids::PROFILE_CURRENT_UUID, &[profile])
            .await
    }

    pub async fn send_profile_command(
        &self,
        cmd: icd::ProfileCommand,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let cmd_byte = match cmd {
            icd::ProfileCommand::Reset => 0,
            icd::ProfileCommand::Next => 1,
            icd::ProfileCommand::Previous => 2,
        };
        self.write_characteristic(uuids::PROFILE_COMMAND_UUID, &[cmd_byte])
            .await
    }

    // Session Service Methods
    pub async fn get_session_status(
        &self,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let status =
            self.read_characteristic(uuids::SESSION_STATUS_UUID).await?[0]
                != 0;
        Ok(status)
    }

    pub async fn get_session_id(
        &self,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let id = String::from_utf8(
            self.read_characteristic(uuids::SESSION_ID_UUID).await?,
        )?;
        Ok(id)
    }

    pub async fn set_session_id(
        &self,
        id: &String,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.write_characteristic(uuids::SESSION_ID_UUID, id.as_bytes()).await
    }

    pub async fn send_session_command(
        &self,
        cmd: u8,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.write_characteristic(uuids::SESSION_CMD_UUID, &[cmd]).await
    }

    // ADS Service Methods
    pub async fn get_ads_config(
        &self,
    ) -> Result<icd::AdsConfig, Box<dyn std::error::Error + Send + Sync>> {
        let mut config = icd::AdsConfig::default();

        // Read all the characteristics and update the config
        let (
            daisy_en,
            clk_en,
            sample_rate,
            internal_calibration,
            calibration_amplitude,
            calibration_frequency,
            pd_refbuf,
            bias_meas,
            biasref_int,
            pd_bias,
            bias_loff_sens,
            bias_stat,
            comparator_threshold_pos,
            lead_off_current,
            lead_off_frequency,
            srb1,
            single_shot,
            pd_loff_comp,
            power_down,
            gain,
            srb2,
            mux,
            bias_sensp,
            bias_sensn,
            lead_off_sensp,
            lead_off_sensn,
            lead_off_flip,
        ) = futures::try_join!(
            self.read_characteristic(DAISY_EN_UUID),
            self.read_characteristic(CLK_EN_UUID),
            self.read_characteristic(SAMPLE_RATE_UUID),
            self.read_characteristic(INTERNAL_CALIBRATION_UUID),
            self.read_characteristic(CALIBRATION_AMPLITUDE_UUID),
            self.read_characteristic(CALIBRATION_FREQUENCY_UUID),
            self.read_characteristic(PD_REFBUF_UUID),
            self.read_characteristic(BIAS_MEAS_UUID),
            self.read_characteristic(BIASREF_INT_UUID),
            self.read_characteristic(PD_BIAS_UUID),
            self.read_characteristic(BIAS_LOFF_SENS_UUID),
            self.read_characteristic(BIAS_STAT_UUID),
            self.read_characteristic(COMPARATOR_THRESHOLD_POS_UUID),
            self.read_characteristic(LEAD_OFF_CURRENT_UUID),
            self.read_characteristic(LEAD_OFF_FREQUENCY_UUID),
            self.read_characteristic(SRB1_UUID),
            self.read_characteristic(SINGLE_SHOT_UUID),
            self.read_characteristic(PD_LOFF_COMP_UUID),
            self.read_characteristic(POWER_DOWN_UUID),
            self.read_characteristic(GAIN_UUID),
            self.read_characteristic(SRB2_UUID),
            self.read_characteristic(MUX_UUID),
            self.read_characteristic(BIAS_SENSP_UUID),
            self.read_characteristic(BIAS_SENSN_UUID),
            self.read_characteristic(LEAD_OFF_SENSP_UUID),
            self.read_characteristic(LEAD_OFF_SENSN_UUID),
            self.read_characteristic(LEAD_OFF_FLIP_UUID),
        )?;

        config.daisy_en = daisy_en[0] != 0;
        config.clk_en = clk_en[0] != 0;
        config.sample_rate = sample_rate[0].into();
        config.internal_calibration = internal_calibration[0] != 0;
        config.calibration_amplitude = calibration_amplitude[0] != 0;
        config.calibration_frequency = calibration_frequency[0].into();
        config.pd_refbuf = pd_refbuf[0] != 0;
        config.bias_meas = bias_meas[0] != 0;
        config.biasref_int = biasref_int[0] != 0;
        config.pd_bias = pd_bias[0] != 0;
        config.bias_loff_sens = bias_loff_sens[0] != 0;
        config.bias_stat = bias_stat[0] != 0;
        config.comparator_threshold_pos = comparator_threshold_pos[0].into();
        config.lead_off_current = lead_off_current[0].into();
        config.lead_off_frequency = lead_off_frequency[0].into();
        config.srb1 = srb1[0] != 0;
        config.single_shot = single_shot[0] != 0;
        config.pd_loff_comp = pd_loff_comp[0] != 0;

        // Update channels
        for i in 0..power_down.len() {
            let channel = icd::ChannelConfig {
                power_down: power_down[i] != 0,
                gain: gain[i].into(),
                srb2: srb2[i] != 0,
                mux: mux[i].into(),
                bias_sensp: bias_sensp[i] != 0,
                bias_sensn: bias_sensn[i] != 0,
                lead_off_sensp: lead_off_sensp[i] != 0,
                lead_off_sensn: lead_off_sensn[i] != 0,
                lead_off_flip: lead_off_flip[i] != 0,
            };
            config.channels.push(channel).unwrap();
        }

        Ok(config)
    }

    pub async fn set_ads_config(
        &self,
        config: &icd::AdsConfig,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Write all single-value characteristics
        self.write_characteristic(DAISY_EN_UUID, &[config.daisy_en as u8])
            .await?;
        self.write_characteristic(CLK_EN_UUID, &[config.clk_en as u8]).await?;
        self.write_characteristic(
            SAMPLE_RATE_UUID,
            &[config.sample_rate.into()],
        )
        .await?;
        self.write_characteristic(
            INTERNAL_CALIBRATION_UUID,
            &[config.internal_calibration as u8],
        )
        .await?;
        self.write_characteristic(
            CALIBRATION_AMPLITUDE_UUID,
            &[config.calibration_amplitude as u8],
        )
        .await?;
        self.write_characteristic(
            CALIBRATION_FREQUENCY_UUID,
            &[config.calibration_frequency.into()],
        )
        .await?;
        self.write_characteristic(PD_REFBUF_UUID, &[config.pd_refbuf as u8])
            .await?;
        self.write_characteristic(BIAS_MEAS_UUID, &[config.bias_meas as u8])
            .await?;
        self.write_characteristic(
            BIASREF_INT_UUID,
            &[config.biasref_int as u8],
        )
        .await?;
        self.write_characteristic(PD_BIAS_UUID, &[config.pd_bias as u8])
            .await?;
        self.write_characteristic(
            BIAS_LOFF_SENS_UUID,
            &[config.bias_loff_sens as u8],
        )
        .await?;
        self.write_characteristic(BIAS_STAT_UUID, &[config.bias_stat as u8])
            .await?;
        self.write_characteristic(
            COMPARATOR_THRESHOLD_POS_UUID,
            &[config.comparator_threshold_pos.into()],
        )
        .await?;
        self.write_characteristic(
            LEAD_OFF_CURRENT_UUID,
            &[config.lead_off_current.into()],
        )
        .await?;
        self.write_characteristic(
            LEAD_OFF_FREQUENCY_UUID,
            &[config.lead_off_frequency.into()],
        )
        .await?;
        self.write_characteristic(SRB1_UUID, &[config.srb1 as u8]).await?;
        self.write_characteristic(
            SINGLE_SHOT_UUID,
            &[config.single_shot as u8],
        )
        .await?;
        self.write_characteristic(
            PD_LOFF_COMP_UUID,
            &[config.pd_loff_comp as u8],
        )
        .await?;

        // Write channel arrays
        let mut power_down = Vec::new();
        let mut gain = Vec::new();
        let mut srb2 = Vec::new();
        let mut mux = Vec::new();
        let mut bias_sensp = Vec::new();
        let mut bias_sensn = Vec::new();
        let mut lead_off_sensp = Vec::new();
        let mut lead_off_sensn = Vec::new();
        let mut lead_off_flip = Vec::new();

        for channel in &config.channels {
            power_down.push(channel.power_down as u8);
            gain.push(channel.gain.into());
            srb2.push(channel.srb2 as u8);
            mux.push(channel.mux.into());
            bias_sensp.push(channel.bias_sensp as u8);
            bias_sensn.push(channel.bias_sensn as u8);
            lead_off_sensp.push(channel.lead_off_sensp as u8);
            lead_off_sensn.push(channel.lead_off_sensn as u8);
            lead_off_flip.push(channel.lead_off_flip as u8);
        }

        self.write_characteristic(POWER_DOWN_UUID, &power_down).await?;
        self.write_characteristic(GAIN_UUID, &gain).await?;
        self.write_characteristic(SRB2_UUID, &srb2).await?;
        self.write_characteristic(MUX_UUID, &mux).await?;
        self.write_characteristic(BIAS_SENSP_UUID, &bias_sensp).await?;
        self.write_characteristic(BIAS_SENSN_UUID, &bias_sensn).await?;
        self.write_characteristic(LEAD_OFF_SENSP_UUID, &lead_off_sensp)
            .await?;
        self.write_characteristic(LEAD_OFF_SENSN_UUID, &lead_off_sensn)
            .await?;
        self.write_characteristic(LEAD_OFF_FLIP_UUID, &lead_off_flip).await?;

        Ok(())
    }

    pub async fn start_streaming(
        &self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.write_characteristic(COMMAND_UUID, &[0]).await // 0 = Start command
    }

    pub async fn stop_streaming(
        &self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.write_characteristic(COMMAND_UUID, &[1]).await // 1 = Stop command
    }

    pub async fn reset_config(
        &self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.write_characteristic(COMMAND_UUID, &[2]).await
    }

    pub async fn set_daisy_en(
        &self,
        value: bool,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.write_characteristic(DAISY_EN_UUID, &[value as u8]).await
    }

    pub async fn set_clk_en(
        &self,
        value: bool,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.write_characteristic(CLK_EN_UUID, &[value as u8]).await
    }

    pub async fn set_sample_rate(
        &self,
        value: SampleRate,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.write_characteristic(SAMPLE_RATE_UUID, &[value.into()]).await
    }

    pub async fn set_internal_calibration(
        &self,
        value: bool,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.write_characteristic(INTERNAL_CALIBRATION_UUID, &[value as u8])
            .await
    }

    pub async fn set_calibration_amplitude(
        &self,
        value: bool,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.write_characteristic(CALIBRATION_AMPLITUDE_UUID, &[value as u8])
            .await
    }

    pub async fn set_calibration_frequency(
        &self,
        value: CalFreq,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.write_characteristic(CALIBRATION_FREQUENCY_UUID, &[value.into()])
            .await
    }

    pub async fn set_pd_refbuf(
        &self,
        value: bool,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.write_characteristic(PD_REFBUF_UUID, &[value as u8]).await
    }

    pub async fn set_bias_meas(
        &self,
        value: bool,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.write_characteristic(BIAS_MEAS_UUID, &[value as u8]).await
    }

    pub async fn set_biasref_int(
        &self,
        value: bool,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.write_characteristic(BIASREF_INT_UUID, &[value as u8]).await
    }

    pub async fn set_pd_bias(
        &self,
        value: bool,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.write_characteristic(PD_BIAS_UUID, &[value as u8]).await
    }

    pub async fn set_bias_loff_sens(
        &self,
        value: bool,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.write_characteristic(BIAS_LOFF_SENS_UUID, &[value as u8]).await
    }

    pub async fn set_bias_stat(
        &self,
        value: bool,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.write_characteristic(BIAS_STAT_UUID, &[value as u8]).await
    }

    pub async fn set_comparator_threshold(
        &self,
        value: CompThreshPos,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.write_characteristic(
            COMPARATOR_THRESHOLD_POS_UUID,
            &[value.into()],
        )
        .await
    }

    pub async fn set_lead_off_current(
        &self,
        value: ILeadOff,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.write_characteristic(LEAD_OFF_CURRENT_UUID, &[value.into()]).await
    }

    pub async fn set_lead_off_frequency(
        &self,
        value: FLeadOff,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.write_characteristic(LEAD_OFF_FREQUENCY_UUID, &[value.into()])
            .await
    }

    pub async fn set_srb1(
        &self,
        value: bool,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.write_characteristic(SRB1_UUID, &[value as u8]).await
    }

    pub async fn set_single_shot(
        &self,
        value: bool,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.write_characteristic(SINGLE_SHOT_UUID, &[value as u8]).await
    }

    pub async fn set_pd_loff_comp(
        &self,
        value: bool,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.write_characteristic(PD_LOFF_COMP_UUID, &[value as u8]).await
    }

    pub async fn set_power_down(
        &self,
        values: &[u8],
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.write_characteristic(POWER_DOWN_UUID, values).await
    }

    pub async fn set_gain(
        &self,
        values: &[u8],
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.write_characteristic(GAIN_UUID, values).await
    }

    pub async fn set_srb2(
        &self,
        values: &[u8],
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.write_characteristic(SRB2_UUID, values).await
    }

    pub async fn set_mux(
        &self,
        values: &[u8],
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.write_characteristic(MUX_UUID, values).await
    }

    pub async fn set_bias_sensp(
        &self,
        values: &[u8],
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.write_characteristic(BIAS_SENSP_UUID, values).await
    }

    pub async fn set_bias_sensn(
        &self,
        values: &[u8],
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.write_characteristic(BIAS_SENSN_UUID, values).await
    }

    pub async fn set_lead_off_sensp(
        &self,
        values: &[u8],
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.write_characteristic(LEAD_OFF_SENSP_UUID, values).await
    }

    pub async fn set_lead_off_sensn(
        &self,
        values: &[u8],
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.write_characteristic(LEAD_OFF_SENSN_UUID, values).await
    }

    pub async fn set_lead_off_flip(
        &self,
        values: &[u8],
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.write_characteristic(LEAD_OFF_FLIP_UUID, values).await
    }

    // Mic Service Methods
    pub async fn notify_mic_stream(
        &self,
    ) -> impl Stream<Item = bluest::Result<Vec<u8>>> + Send + Unpin + use<'_>
    {
        let characteristic = self
            .get_characteristic(uuids::mic::DATA_STREAM_UUID)
            .ok_or("Mic data stream characteristic not found")
            .unwrap();
        let stream = characteristic.notify().await.unwrap();
        stream
    }

    pub async fn get_mic_config(
        &self,
    ) -> Result<icd::MicConfig, Box<dyn std::error::Error + Send + Sync>>
    {
        let gain_db =
            self.read_characteristic(uuids::mic::GAIN_DB_UUID).await?[0]
                as i8;
        let sample_rate = icd::MicSampleRate::from(
            self.read_characteristic(uuids::mic::SAMPLE_RATE_UUID).await?[0],
        );
        Ok(icd::MicConfig { gain_db, sample_rate })
    }

    pub async fn set_mic_config(
        &self,
        config: &icd::MicConfig,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.write_characteristic(
            uuids::mic::GAIN_DB_UUID,
            &[config.gain_db as u8],
        )
        .await?;
        self.write_characteristic(
            uuids::mic::SAMPLE_RATE_UUID,
            &[config.sample_rate as u8],
        )
        .await?;
        Ok(())
    }

    pub async fn start_mic_streaming(
        &self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.write_characteristic(uuids::mic::COMMAND_UUID, &[0]).await
    }

    pub async fn stop_mic_streaming(
        &self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.write_characteristic(uuids::mic::COMMAND_UUID, &[1]).await
    }

    pub async fn is_connected(&self) -> bool {
        self.device.is_connected().await
    }

    pub async fn close(&self) -> bluest::Result<()> {
        self.adapter.disconnect_device(&self.device).await
    }
}
