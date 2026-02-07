use dc_mini_icd::{
    AdsConfig, AdsGetConfigEndpoint, AdsResetConfigEndpoint,
    AdsSetConfigEndpoint, AdsStartEndpoint, AdsStopEndpoint,
    BatteryGetLevelEndpoint, BatteryLevel, DeviceInfo, DeviceInfoGetEndpoint,
    DfuAbortEndpoint, DfuBegin, DfuBeginEndpoint, DfuFinishEndpoint,
    DfuProgress, DfuResult, DfuStatusEndpoint, DfuWriteChunk,
    DfuWriteEndpoint, MicConfig, MicGetConfigEndpoint, MicSetConfigEndpoint,
    MicStartEndpoint, MicStopEndpoint, ProfileCommand, ProfileCommandEndpoint,
    ProfileGetEndpoint, ProfileSetEndpoint, SessionGetIdEndpoint,
    SessionGetStatusEndpoint, SessionId, SessionSetIdEndpoint,
    SessionStartEndpoint, SessionStopEndpoint,
};
use postcard_rpc::{
    header::VarSeqKind,
    host_client::{HostClient, HostErr},
    standard_icd::{WireError, ERROR_PATH},
};
use std::convert::Infallible;

pub struct UsbClient {
    pub client: HostClient<WireError>,
}

#[derive(Debug)]
pub enum UsbError<E> {
    Comms(HostErr<WireError>),
    Endpoint(E),
}

impl<E> From<HostErr<WireError>> for UsbError<E> {
    fn from(value: HostErr<WireError>) -> Self {
        Self::Comms(value)
    }
}

impl UsbClient {
    pub fn try_new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>>
    {
        let client = HostClient::try_new_raw_nusb(
            |d| d.product_string() == Some("dc-mini"),
            ERROR_PATH,
            8,
            VarSeqKind::Seq2,
        )?;
        Ok(Self { client })
    }

    pub fn new() -> Self {
        Self::try_new().expect("Failed to create USB client")
    }

    pub async fn wait_closed(&self) {
        self.client.wait_closed().await;
    }

    // ADS Service Methods
    pub async fn start_streaming(
        &self,
    ) -> Result<AdsConfig, UsbError<Infallible>> {
        let config = self.client.send_resp::<AdsStartEndpoint>(&()).await?;
        Ok(config)
    }

    pub async fn stop_streaming(&self) -> Result<(), UsbError<Infallible>> {
        let res = self.client.send_resp::<AdsStopEndpoint>(&()).await?;
        Ok(res)
    }

    pub async fn reset_ads_config(
        &self,
    ) -> Result<bool, UsbError<Infallible>> {
        let result =
            self.client.send_resp::<AdsResetConfigEndpoint>(&()).await?;
        Ok(result)
    }

    pub async fn get_ads_config(
        &self,
    ) -> Result<AdsConfig, UsbError<Infallible>> {
        let config =
            self.client.send_resp::<AdsGetConfigEndpoint>(&()).await?;
        Ok(config)
    }

    pub async fn set_ads_config(
        &self,
        config: AdsConfig,
    ) -> Result<bool, UsbError<Infallible>> {
        let result =
            self.client.send_resp::<AdsSetConfigEndpoint>(&config).await?;
        Ok(result)
    }

    // Battery Service Methods
    pub async fn get_battery_level(
        &self,
    ) -> Result<BatteryLevel, UsbError<Infallible>> {
        let level =
            self.client.send_resp::<BatteryGetLevelEndpoint>(&()).await?;
        Ok(level)
    }

    // Device Info Service Methods
    pub async fn get_device_info(
        &self,
    ) -> Result<DeviceInfo, UsbError<Infallible>> {
        let info = self.client.send_resp::<DeviceInfoGetEndpoint>(&()).await?;
        Ok(info)
    }

    // Profile Service Methods
    pub async fn get_profile(&self) -> Result<u8, UsbError<Infallible>> {
        let profile = self.client.send_resp::<ProfileGetEndpoint>(&()).await?;
        Ok(profile)
    }

    pub async fn set_profile(
        &self,
        profile: u8,
    ) -> Result<bool, UsbError<Infallible>> {
        let result =
            self.client.send_resp::<ProfileSetEndpoint>(&profile).await?;
        Ok(result)
    }

    pub async fn send_profile_command(
        &self,
        cmd: ProfileCommand,
    ) -> Result<bool, UsbError<Infallible>> {
        let result =
            self.client.send_resp::<ProfileCommandEndpoint>(&cmd).await?;
        Ok(result)
    }

    // Session Service Methods
    pub async fn get_session_status(
        &self,
    ) -> Result<bool, UsbError<Infallible>> {
        let status =
            self.client.send_resp::<SessionGetStatusEndpoint>(&()).await?;
        Ok(status)
    }

    pub async fn get_session_id(
        &self,
    ) -> Result<String, UsbError<Infallible>> {
        let id = String::from(
            self.client
                .send_resp::<SessionGetIdEndpoint>(&())
                .await?
                .0
                .as_str(),
        );
        Ok(id)
    }

    pub async fn set_session_id(
        &self,
        id: String,
    ) -> Result<bool, UsbError<Infallible>> {
        let id = SessionId(
            heapless::String::from_utf8(
                heapless::Vec::from_slice(id.as_bytes()).unwrap(),
            )
            .unwrap(),
        );
        let result =
            self.client.send_resp::<SessionSetIdEndpoint>(&id).await?;
        Ok(result)
    }

    pub async fn start_session(&self) -> Result<bool, UsbError<Infallible>> {
        let result =
            self.client.send_resp::<SessionStartEndpoint>(&()).await?;
        Ok(result)
    }

    pub async fn stop_session(&self) -> Result<bool, UsbError<Infallible>> {
        let result = self.client.send_resp::<SessionStopEndpoint>(&()).await?;
        Ok(result)
    }

    // Mic Service Methods
    pub async fn start_mic_streaming(
        &self,
    ) -> Result<MicConfig, UsbError<Infallible>> {
        let config = self.client.send_resp::<MicStartEndpoint>(&()).await?;
        Ok(config)
    }

    pub async fn stop_mic_streaming(
        &self,
    ) -> Result<(), UsbError<Infallible>> {
        let res = self.client.send_resp::<MicStopEndpoint>(&()).await?;
        Ok(res)
    }

    pub async fn get_mic_config(
        &self,
    ) -> Result<MicConfig, UsbError<Infallible>> {
        let config =
            self.client.send_resp::<MicGetConfigEndpoint>(&()).await?;
        Ok(config)
    }

    pub async fn set_mic_config(
        &self,
        config: MicConfig,
    ) -> Result<bool, UsbError<Infallible>> {
        let result =
            self.client.send_resp::<MicSetConfigEndpoint>(&config).await?;
        Ok(result)
    }

    pub fn is_connected(&self) -> bool {
        !self.client.is_closed()
    }

    // DFU Service Methods
    pub async fn dfu_begin(
        &self,
        firmware_size: u32,
    ) -> Result<DfuResult, UsbError<Infallible>> {
        let result = self
            .client
            .send_resp::<DfuBeginEndpoint>(&DfuBegin { firmware_size })
            .await?;
        Ok(result)
    }

    pub async fn dfu_write(
        &self,
        offset: u32,
        data: &[u8],
    ) -> Result<DfuResult, UsbError<Infallible>> {
        let chunk = DfuWriteChunk {
            offset,
            data: heapless::Vec::from_slice(data).unwrap(),
        };
        let result = self.client.send_resp::<DfuWriteEndpoint>(&chunk).await?;
        Ok(result)
    }

    pub async fn dfu_finish(&self) -> Result<DfuResult, UsbError<Infallible>> {
        let result = self.client.send_resp::<DfuFinishEndpoint>(&()).await?;
        Ok(result)
    }

    pub async fn dfu_abort(&self) -> Result<DfuResult, UsbError<Infallible>> {
        let result = self.client.send_resp::<DfuAbortEndpoint>(&()).await?;
        Ok(result)
    }

    pub async fn dfu_status(
        &self,
    ) -> Result<DfuProgress, UsbError<Infallible>> {
        let status = self.client.send_resp::<DfuStatusEndpoint>(&()).await?;
        Ok(status)
    }

    /// Perform a full DFU transfer of the given firmware binary.
    /// Sends the firmware in chunks and prints progress.
    pub async fn dfu_upload(
        &self,
        firmware: &[u8],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        const CHUNK_SIZE: usize = 256;

        println!("Starting DFU: {} bytes", firmware.len());
        let begin_result = self.dfu_begin(firmware.len() as u32).await?;
        if !begin_result.success {
            return Err(
                format!("DFU begin failed: {}", begin_result.message).into()
            );
        }
        println!("DFU partition erased");

        let mut offset = 0u32;
        for chunk in firmware.chunks(CHUNK_SIZE) {
            let result = self.dfu_write(offset, chunk).await?;
            if !result.success {
                let _ = self.dfu_abort().await;
                return Err(format!(
                    "DFU write failed at offset {}: {}",
                    offset, result.message
                )
                .into());
            }
            offset += chunk.len() as u32;
            if offset % (64 * 1024) == 0 || offset as usize == firmware.len() {
                println!(
                    "  Progress: {}/{} bytes ({:.1}%)",
                    offset,
                    firmware.len(),
                    offset as f64 / firmware.len() as f64 * 100.0
                );
            }
        }

        println!("Firmware transfer complete, finishing DFU...");
        let finish_result = self.dfu_finish().await;
        // The device will reset, so connection may drop before we get a response
        match finish_result {
            Ok(result) => {
                if result.success {
                    println!("DFU finish acknowledged. Device will reset.");
                } else {
                    return Err(format!(
                        "DFU finish failed: {}",
                        result.message
                    )
                    .into());
                }
            }
            Err(_) => {
                println!("Device is resetting (connection lost as expected).");
            }
        }

        Ok(())
    }
}

impl Default for UsbClient {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for UsbClient {
    fn drop(&mut self) {
        self.client.close();
    }
}
