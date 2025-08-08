use embassy_sync::channel::Channel;

use super::{
    ads::*, advertiser, enable_softdevice, softdevice_task, Advertiser,
};
use crate::prelude::*;
use embassy_executor::Spawner;
use embassy_futures::select::select6;
use nrf_softdevice::ble::{gatt_server, Connection};
use nrf_softdevice::Softdevice;
use static_cell::StaticCell;

#[cfg(feature = "usb")]
use embassy_nrf::usb::vbus_detect::SoftwareVbusDetect;

#[nrf_softdevice::gatt_server]
pub struct Server {
    pub battery: BatteryService,
    pub device_info: DeviceInfoService,
    pub profile: ProfileService,
    pub ads: AdsService,
    pub session: SessionService,
}

impl Server {
    pub fn start_gatt(
        name: &'static str,
        spawner: Spawner,
        #[cfg(feature = "usb")] vbus: &'static SoftwareVbusDetect,
    ) -> (&'static Server, Advertiser, &'static Softdevice) {
        // Spawn the underlying softdevice task
        let sd = enable_softdevice(name);
        info!("softdevice initialized");
        // Create a BLE GATT server and make it static
        static SERVER: StaticCell<Server> = StaticCell::new();
        let server = SERVER.init(Server::new(sd).unwrap());
        info!("Starting Gatt Server");

        #[cfg(feature = "usb")]
        unwrap!(spawner.spawn(softdevice_task(sd, vbus)));
        #[cfg(not(feature = "usb"))]
        unwrap!(spawner.spawn(softdevice_task(sd)));

        let advertiser = advertiser::AdvertiserBuilder::new(name, sd).build();

        (server, advertiser, sd)
    }
}

/// A BLE GATT server
pub async fn gatt_server_task(
    server: &Server,
    conn: &Connection,
    app_context: &'static Mutex<CriticalSectionRawMutex, AppContext>,
) {
    let ads_channel: Channel<NoopRawMutex, AdsServiceEvent, 10> =
        Channel::new();
    let ads_sender = ads_channel.sender();
    let ads_receiver = ads_channel.receiver();

    let session_channel: Channel<NoopRawMutex, SessionServiceEvent, 10> =
        Channel::new();
    let session_sender = session_channel.sender();
    let session_receiver = session_channel.receiver();

    let battery_channel: Channel<NoopRawMutex, BatteryServiceEvent, 10> =
        Channel::new();
    let battery_sender = battery_channel.sender();
    let battery_receiver = battery_channel.receiver();

    let device_info_channel: Channel<
        NoopRawMutex,
        DeviceInfoServiceEvent,
        10,
    > = Channel::new();
    let device_info_sender = device_info_channel.sender();
    let device_info_receiver = device_info_channel.receiver();

    let profile_channel: Channel<NoopRawMutex, ProfileServiceEvent, 10> =
        Channel::new();
    let profile_sender = profile_channel.sender();
    let profile_receiver = profile_channel.receiver();

    let gatt_server_fut = gatt_server::run(&conn, server, |e| match e {
        ServerEvent::Ads(e) => {
            let res = ads_sender.try_send(e);
            if res.is_err() {
                warn!("Error when trying to send AdsServiceEvent!");
            }
        }
        ServerEvent::Session(e) => {
            let res = session_sender.try_send(e);
            if res.is_err() {
                warn!("Error when trying to send SessionServiceEvent!");
            }
        }
        ServerEvent::Battery(e) => {
            let res = battery_sender.try_send(e);
            if res.is_err() {
                warn!("Error when trying to send BatteryServiceEvent!");
            }
        }
        ServerEvent::DeviceInfo(e) => {
            let res = device_info_sender.try_send(e);
            if res.is_err() {
                warn!("Error when trying to send DeviceInfoServiceEvent!");
            }
        }
        ServerEvent::Profile(e) => {
            let res = profile_sender.try_send(e);
            if res.is_err() {
                warn!("Error when trying to send ProfileServiceEvent!");
            }
        }
    });

    let ads_handle_fut = server.ads.handle(ads_receiver, app_context);
    let session_handle_fut =
        server.session.handle(session_receiver, app_context);
    let battery_handle_fut =
        server.battery.handle(battery_receiver, app_context);
    let device_info_handle_fut =
        server.device_info.handle(device_info_receiver, app_context);
    let profile_handle_fut =
        server.profile.handle(profile_receiver, app_context);

    futures::pin_mut!(
        gatt_server_fut,
        ads_handle_fut,
        session_handle_fut,
        battery_handle_fut,
        device_info_handle_fut,
        profile_handle_fut
    );

    let _ = select6(
        gatt_server_fut,
        ads_handle_fut,
        session_handle_fut,
        battery_handle_fut,
        device_info_handle_fut,
        profile_handle_fut,
    )
    .await;

    info!("Gatt server task finished");
}
