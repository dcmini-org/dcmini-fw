use crate::prelude::*;
use embassy_sync::channel::Receiver;

/// Device Information Service (UUID: 0x180A)
/// A standard BLE service that exposes device information.
#[nrf_softdevice::gatt_service(uuid = "180a")]
pub struct DeviceInfoService {
    /// Hardware Revision String (UUID: 0x2A27)
    #[characteristic(uuid = "2a27", read)]
    hardware_revision: heapless::String<32>,

    /// Software Revision String (UUID: 0x2A28)
    #[characteristic(uuid = "2a28", read)]
    software_revision: heapless::String<32>,

    /// Manufacturer Name String (UUID: 0x2A29)
    #[characteristic(uuid = "2a29", read)]
    manufacturer_name: heapless::String<32>,
}

impl DeviceInfoService {
    pub async fn handle(
        &self,
        rx: Receiver<'_, NoopRawMutex, DeviceInfoServiceEvent, 10>,
        _app_context: &'static Mutex<CriticalSectionRawMutex, AppContext>,
    ) {
        loop {
            let _event = rx.receive().await;
            // No events to handle for device info service as it's read-only
        }
    }
}

/// Updates the device information characteristics
pub async fn update_device_info_characteristics(
    app_context: &'static Mutex<CriticalSectionRawMutex, AppContext>,
) {
    let app_ctx = app_context.lock().await;
    let server = app_ctx.ble_server;
    unwrap!(server
        .device_info
        .hardware_revision_set(&app_ctx.device_info.hardware_revision));
    unwrap!(server
        .device_info
        .software_revision_set(&app_ctx.device_info.software_revision));
    unwrap!(server
        .device_info
        .manufacturer_name_set(&app_ctx.device_info.manufacturer_name));
}
