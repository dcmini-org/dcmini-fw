use super::Server;
use crate::prelude::*;
use trouble_host::prelude::*;

/// Device Information Service (UUID: 0x180A)
/// A standard BLE service that exposes device information.
#[gatt_service(uuid = "180a")]
pub struct DeviceInfoService {
    /// Hardware Revision String (UUID: 0x2A27)
    #[characteristic(uuid = "2a27", read)]
    pub hardware_revision: heapless::String<32>,

    /// Software Revision String (UUID: 0x2A28)
    #[characteristic(uuid = "2a28", read)]
    pub software_revision: heapless::String<32>,

    /// Manufacturer Name String (UUID: 0x2A29)
    #[characteristic(uuid = "2a29", read)]
    pub manufacturer_name: heapless::String<32>,
}

impl<'d> Server<'d> {
    pub async fn handle_device_info_read_event(
        &self,
        handle: u16,
        _app_context: &'static Mutex<CriticalSectionRawMutex, AppContext>,
    ) {
        if handle == self.device_info.hardware_revision.handle
            || handle == self.device_info.software_revision.handle
            || handle == self.device_info.manufacturer_name.handle
        {
            // Device info reads are handled by the characteristics directly
        }
    }
}

/// Updates the device information characteristics
pub async fn update_device_info_characteristics(
    server: &Server<'_>,
    hardware_rev: &str,
    software_rev: &str,
    manufacturer: &str,
) {
    let (hw_rev, hw_truncated) = bounded_heapless_string::<32>(hardware_rev);
    let (sw_rev, sw_truncated) =
        bounded_heapless_string::<32>(software_rev);
    let (mfg, mfg_truncated) =
        bounded_heapless_string::<32>(manufacturer);
    if hw_truncated || sw_truncated || mfg_truncated {
        report_status(
            icd::SubsystemId::BleStream,
            icd::SubsystemState::Degraded,
            icd::FaultCode::MetadataTruncated,
        )
        .await;
    }

    unwrap!(server.set(&server.device_info.hardware_revision, &hw_rev));
    unwrap!(server.set(&server.device_info.software_revision, &sw_rev));
    unwrap!(server.set(&server.device_info.manufacturer_name, &mfg));
}
