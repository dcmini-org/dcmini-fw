use dc_mini_icd::{
    AdsConfig, AdsGetConfigEndpoint, AdsResetConfigEndpoint,
    AdsSetConfigEndpoint, AdsStartEndpoint, AdsStopEndpoint,
    BatteryGetLevelEndpoint, BatteryLevel, DeviceInfo, DeviceInfoGetEndpoint,
    ProfileCommand, ProfileCommandEndpoint, ProfileGetEndpoint,
    ProfileSetEndpoint, SessionGetIdEndpoint, SessionGetStatusEndpoint,
    SessionId, SessionSetIdEndpoint, SessionStartEndpoint,
    SessionStopEndpoint,
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

    pub fn is_connected(&self) -> bool {
        !self.client.is_closed()
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
