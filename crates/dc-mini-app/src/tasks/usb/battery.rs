use dc_mini_icd::BatteryLevel;
use postcard_rpc::header::VarHeader;

pub async fn battery_get_level(
    _context: &mut super::Context,
    _header: VarHeader,
    _req: (),
) -> BatteryLevel {
    // let app_ctx = context.app.lock().await;
    // TODO: Implement actual battery level reading
    BatteryLevel(100)
}
