use crate::prelude::*;
use embassy_sync::channel::Receiver;

/// Battery Service (UUID: 0x180F)
/// A standard BLE service that exposes battery level information of a device.
#[nrf_softdevice::gatt_service(uuid = "180f")]
pub struct BatteryService {
    /// Battery Level (UUID: 0x2A19)
    /// The current charge level of a battery in percentage from 0% to 100%
    #[characteristic(uuid = "2a19", read, notify)]
    pub battery_level: u8,
}

impl BatteryService {
    pub async fn handle(
        &self,
        rx: Receiver<'_, NoopRawMutex, BatteryServiceEvent, 10>,
        _app_context: &'static Mutex<CriticalSectionRawMutex, AppContext>,
    ) {
        loop {
            let event = rx.receive().await;
            match event {
                BatteryServiceEvent::BatteryLevelCccdWrite {
                    notifications,
                } => {
                    info!("Battery level notifications = {:?}", notifications);
                }
            }
        }
    }
}

/// Updates the battery level characteristic with the current value
pub async fn update_battery_characteristics(
    app_context: &'static Mutex<CriticalSectionRawMutex, AppContext>,
    battery_level: u8,
) {
    let app_ctx = app_context.lock().await;
    // Ensure battery level is within valid range (0-100)
    // let level = battery_level.min(100);
    let level = battery_level;
    unwrap!(app_ctx.ble_server.battery.battery_level_set(&level));
}
