pub mod ads;
pub mod advertiser;
pub mod battery;
pub mod clock;
pub mod device_info;
pub mod gatt;
pub mod profile;
pub mod session;

use dc_mini_bsp::ble::{MultiprotocolServiceLayer, SoftdeviceController};
use trouble_host::prelude::*;

pub use ads::*;
pub use advertiser::*;
pub use battery::*;
pub use clock::*;
pub use device_info::*;
pub use gatt::*;
pub use profile::*;
pub use session::*;

use super::Error;

use crate::prelude::{
    error, info, AppContext, CriticalSectionRawMutex, Mutex,
};

/// Size of L2CAP packets (ATT MTU is this - 4)
// const L2CAP_MTU: usize = 251;

pub const MTU: usize = 381;
// Aligned to 4 bytes + 3 bytes for header
pub const ATT_MTU: usize = MTU + 3;
pub const L2CAP_MTU: usize = ATT_MTU + 4;

/// Max number of connections
const CONNECTIONS_MAX: usize = 1;

/// Max number of L2CAP channels.
const L2CAP_CHANNELS_MAX: usize = 2; // Signal + att

pub type BleController = SoftdeviceController<'static>;

pub type BleResources =
    HostResources<CONNECTIONS_MAX, L2CAP_CHANNELS_MAX, L2CAP_MTU>;

#[embassy_executor::task]
pub(self) async fn mpsl_task(
    mpsl: &'static MultiprotocolServiceLayer<'static>,
) -> ! {
    mpsl.run().await;
}

#[embassy_executor::task]
pub(self) async fn runner_task(mut runner: Runner<'static, BleController>) {
    let res = runner.run().await;
    info!("ble_task runner exited with: {:?}", res);
}

#[embassy_executor::task]
pub async fn ble_task(
    server: &'static Server<'static>,
    mut peripheral: Peripheral<'static, BleController>,
    _stack: &'static Stack<'static, BleController>,
    app_context: &'static Mutex<CriticalSectionRawMutex, AppContext>,
) {
    loop {
        match advertise("dc-mini", &mut peripheral).await {
            Ok(conn) => {
                // Update connection params
                // let params = ConnectParams {
                //     event_length: Duration::from_millis(36),
                //     ..Default::default()
                // };
                // conn.update_connection_params(stack, params).await;

                // Synchronize time
                // let s = Spawner::for_current_executor().await;
                // s.must_spawn(ble::sync_time(&stack, conn.clone()));

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
