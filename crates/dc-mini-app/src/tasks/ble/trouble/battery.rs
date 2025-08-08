use super::Server;
use crate::prelude::*;
use trouble_host::prelude::*;

/// Battery Service (UUID: 0x180F)
/// A standard BLE service that exposes battery level information of a device.
#[gatt_service(uuid = "180f")]
pub struct BatteryService {
    /// Battery Level (UUID: 0x2A19)
    /// The current charge level of a battery in percentage from 0% to 100%
    #[characteristic(uuid = "2a19", read, notify)]
    battery_level: u8,
}

impl<'d> Server<'d> {
    pub async fn handle_battery_read_event(
        &self,
        handle: u16,
        _app_context: &'static Mutex<CriticalSectionRawMutex, AppContext>,
    ) {
        if handle == self.battery.battery_level.handle {
            // Battery level reads are handled by the characteristic directly
        }
    }
}

/// Updates the battery level characteristic with the current value
pub async fn update_battery_characteristics(
    server: &Server,
    battery_level: u8,
) {
    // Ensure battery level is within valid range (0-100)
    let level = battery_level.min(100);
    unwrap!(server.battery.battery_level.set(server, &level));
}
