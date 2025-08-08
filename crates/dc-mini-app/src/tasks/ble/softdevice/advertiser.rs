use crate::prelude::*;
use nrf_softdevice::ble::advertisement_builder::{
    AdvertisementBuilder, AdvertisementPayload, Flag, ServiceList,
};
use nrf_softdevice::ble::peripheral::AdvertiseError;
use nrf_softdevice::{
    ble::{
        peripheral::{self},
        Connection,
    },
    Softdevice,
};

/// BLE advertiser
pub struct AdvertiserBuilder {
    /// Name of the device
    name: &'static str,
    sd: &'static Softdevice,
}

pub struct Advertiser {
    adv_data: AdvertisementPayload<31>,
    scan_data: AdvertisementPayload<4>,
    sd: &'static Softdevice,
}

/// A BLE advertiser
impl AdvertiserBuilder {
    /// Create a new advertiser builder
    pub fn new(name: &'static str, sd: &'static Softdevice) -> Self {
        Self { name, sd }
    }
    /// Build the advertiser
    pub fn build(self) -> Advertiser {
        let name: &str;
        if self.name.len() > 22 {
            name = &self.name[..22];
            info!("Name truncated to {}", name);
        } else {
            name = self.name;
        }

        let adv_data = AdvertisementBuilder::new()
            .flags(&[Flag::GeneralDiscovery, Flag::LE_Only])
            // .services_16(ServiceList::Complete, &[ServiceUuid16::BATTERY])
            .services_128(
                ServiceList::Complete,
                &[0x32100000_af46_43af_a0ba_4dbeb457f51c_u128.to_le_bytes()],
            )
            .full_name(name)
            .build();

        let scan_data = AdvertisementBuilder::new().build();

        Advertiser { adv_data, scan_data, sd: self.sd }
    }
}

impl Advertiser {
    /// Advertise and connect to a device with the given name
    pub async fn advertise(&self) -> Result<Connection, AdvertiseError> {
        let config = peripheral::Config::default();
        let adv = peripheral::ConnectableAdvertisement::ScannableUndirected {
            adv_data: &self.adv_data,
            scan_data: &self.scan_data,
        };
        info!("advertising");
        let conn =
            peripheral::advertise_connectable(self.sd, adv, &config).await;
        info!("connection established");
        conn
    }
}
