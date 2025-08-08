use dc_mini_icd::DeviceInfo;
use postcard_rpc::header::VarHeader;

pub async fn device_info_get(
    context: &mut super::Context,
    _header: VarHeader,
    _req: (),
) -> DeviceInfo {
    let app_ctx = context.app.lock().await;
    app_ctx.device_info.clone()
}
