pub mod ads;
pub mod advertiser;
pub mod battery;
// pub mod clock;
pub mod device_info;
pub mod gatt;
pub mod profile;
pub mod session;

// Re-exports
use super::Error;
pub use ads::*;
pub use advertiser::*;
pub use battery::*;
// pub use clock::*;
pub use device_info::*;
pub use gatt::*;
pub use profile::*;
pub use session::*;

use crate::{error, info, AppContext, CriticalSectionRawMutex, Mutex};
use nrf_softdevice::{raw, Softdevice};

pub const ATT_MTU: usize = 384;

pub fn enable_softdevice(name: &'static str) -> &'static mut Softdevice {
    let config = nrf_softdevice::Config {
        clock: Some(raw::nrf_clock_lf_cfg_t {
            source: raw::NRF_CLOCK_LF_SRC_RC as u8,
            rc_ctiv: 4,
            rc_temp_ctiv: 2,
            accuracy: raw::NRF_CLOCK_LF_ACCURACY_500_PPM as u8,
        }),
        conn_gap: Some(raw::ble_gap_conn_cfg_t {
            conn_count: 2,
            event_length: 24,
        }),
        conn_gatt: Some(raw::ble_gatt_conn_cfg_t {
            att_mtu: (ATT_MTU as u16),
        }),
        gatts_attr_tab_size: Some(raw::ble_gatts_cfg_attr_tab_size_t {
            attr_tab_size: 4096,
        }),
        gap_role_count: Some(raw::ble_gap_cfg_role_count_t {
            adv_set_count: 1,
            periph_role_count: 3,
            central_role_count: 1,
            central_sec_count: 1,
            _bitfield_1: Default::default(),
        }),
        gap_device_name: Some(raw::ble_gap_cfg_device_name_t {
            p_value: name.as_ptr() as *const u8 as _,
            current_len: name.len() as u16,
            max_len: name.len() as u16,
            write_perm: unsafe { core::mem::zeroed() },
            _bitfield_1: raw::ble_gap_cfg_device_name_t::new_bitfield_1(
                raw::BLE_GATTS_VLOC_STACK as u8,
            ),
        }),
        ..Default::default()
    };

    info!("Enabling softdevice");
    Softdevice::enable(&config)
}

#[cfg(not(feature = "usb"))]
#[embassy_executor::task]
pub async fn softdevice_task(sd: &'static Softdevice) {
    sd.run().await;
}

#[cfg(feature = "usb")]
#[embassy_executor::task]
async fn softdevice_task(
    sd: &'static Softdevice,
    software_vbus: &'static embassy_nrf::usb::vbus_detect::SoftwareVbusDetect,
) -> ! {
    unsafe {
        nrf_softdevice::raw::sd_power_usbdetected_enable(1);
        nrf_softdevice::raw::sd_power_usbpwrrdy_enable(1);
        nrf_softdevice::raw::sd_power_usbremoved_enable(1);
        nrf_softdevice::raw::sd_clock_hfclk_request();
    };
    sd.run_with_callback(|event| {
        match event {
            nrf_softdevice::SocEvent::PowerUsbRemoved => {
                software_vbus.detected(false)
            }
            nrf_softdevice::SocEvent::PowerUsbDetected => {
                software_vbus.detected(true)
            }
            nrf_softdevice::SocEvent::PowerUsbPowerReady => {
                software_vbus.ready()
            }
            _ => {}
        };
    })
    .await
}

#[embassy_executor::task]
pub async fn ble_task(
    server: &'static Server,
    advertiser: Advertiser,
    app_context: &'static Mutex<CriticalSectionRawMutex, AppContext>,
) {
    update_profile_characteristics(app_context).await;
    update_session_characteristics(app_context).await;
    update_battery_characteristics(app_context, 99).await;
    update_device_info_characteristics(app_context).await;
    update_ads_characteristics(app_context).await;

    loop {
        match advertiser.advertise().await {
            Ok(conn) => {
                // TODO: Fix insufficient Authentication
                // info!("Syncing time");
                // ble::sync_time(&conn, &CLOCK).await;
                info!(
                    "Battery level = {:?}",
                    server.battery.battery_level_get()
                );

                let gatt = gatt_server_task(server, &conn, app_context);
                let ads = ads_stream_notify(server, &conn);
                futures::pin_mut!(gatt, ads);
                embassy_futures::select::select(gatt, ads).await;
            }
            Err(e) => {
                error!("Advertisement error: {:?}", e);
                embassy_time::Timer::after_secs(1).await;
            }
        }
    }
}
