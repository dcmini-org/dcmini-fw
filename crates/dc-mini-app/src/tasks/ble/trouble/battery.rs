use super::gatt::{ServerWithDfu, ServerWithoutDfu};
use crate::prelude::*;
use trouble_host::prelude::*;

/// Battery Service (UUID: 0x180F)
/// A standard BLE service that exposes battery level information of a device.
#[gatt_service(uuid = "180f")]
pub struct BatteryService {
    /// Battery Level (UUID: 0x2A19)
    /// The current charge level of a battery in percentage from 0% to 100%
    #[characteristic(uuid = "2a19", read, notify)]
    pub battery_level: u8,
}

macro_rules! impl_battery_support {
    ($server_ty:ident, $update_fn:ident) => {
        impl<'d> $server_ty<'d> {
            pub async fn handle_battery_read_event(
                &self,
                handle: u16,
                _app_context: &'static Mutex<
                    CriticalSectionRawMutex,
                    AppContext,
                >,
            ) {
                if handle == self.battery.battery_level.handle {
                }
            }
        }

        pub async fn $update_fn(server: &$server_ty<'_>, battery_level: u8) {
            let level = battery_level.min(100);
            unwrap!(server.set(&server.battery.battery_level, &level));
        }
    };
}

impl_battery_support!(
    ServerWithDfu,
    update_battery_characteristics_with_dfu
);
impl_battery_support!(
    ServerWithoutDfu,
    update_battery_characteristics_without_dfu
);
