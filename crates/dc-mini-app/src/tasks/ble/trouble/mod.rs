pub mod ads;
pub mod advertiser;
pub mod battery;
pub mod clock;
pub mod device_info;
pub mod dfu;
pub mod gatt;
pub mod mic;
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
pub use mic::*;
pub use profile::*;
pub use session::*;

use super::Error;

use crate::prelude::{
    error, info, AppContext, CriticalSectionRawMutex, Mutex,
};
use crate::tasks::dfu::DfuResources;

/// Maximum ATT MTU supported by this device.
/// Derived from TROUBLE_HOST_DEFAULT_PACKET_POOL_MTU (251) - 4 byte L2CAP header.
/// This ensures every notification fits in a single DLE ACL packet (251 bytes).
pub const ATT_MTU: usize = 247;

/// Max number of connections
const CONNECTIONS_MAX: usize = 1;

/// Max number of L2CAP channels.
const L2CAP_CHANNELS_MAX: usize = 2; // Signal + att

pub type BleController = SoftdeviceController<'static>;

pub type BleResources =
    HostResources<DefaultPacketPool, CONNECTIONS_MAX, L2CAP_CHANNELS_MAX>;

#[embassy_executor::task]
pub async fn mpsl_task(
    mpsl: &'static MultiprotocolServiceLayer<'static>,
) -> ! {
    mpsl.run().await;
}

/// Run the BLE controller runner in a loop, restarting on error.
async fn ble_runner(mut runner: Runner<'_, BleController, DefaultPacketPool>) {
    loop {
        if let Err(e) = runner.run().await {
            error!("BLE runner error: {:?}", e);
        }
    }
}

/// Main BLE run function. Creates all BLE resources on the stack and runs
/// the GATT server loop. When this function returns (or its future is dropped),
/// all BLE resources are cleaned up.
async fn run(
    controller: BleController,
    app_context: &'static Mutex<CriticalSectionRawMutex, AppContext>,
    dfu_resources: &'static DfuResources,
) {
    let address = Address::random([0x42, 0x5A, 0xE3, 0x1E, 0x83, 0xE7]);
    info!("Our address = {:?}", address);

    let mut resources: BleResources = HostResources::new();
    let stack = trouble_host::new(controller, &mut resources)
        .set_random_address(address);
    let Host { mut peripheral, runner, .. } = stack.build();

    let server =
        Server::new_with_config(GapConfig::Peripheral(PeripheralConfig {
            name: "dc-mini",
            appearance: &appearance::sensor::MULTI_SENSOR,
        }))
        .expect("Error creating Gatt Server");

    info!("Starting BLE advertising and GATT service");

    // Use a scope to ensure `server` is dropped before `resources`.
    // The join runs forever (app_loop is infinite), so in practice
    // this drop ordering only matters for compiler verification.
    let app_loop =
        app_task(&server, &mut peripheral, app_context, dfu_resources);
    let _ = embassy_futures::join::join(ble_runner(runner), app_loop).await;
}

async fn app_task<'values>(
    server: &Server<'values>,
    peripheral: &mut Peripheral<'values, BleController, DefaultPacketPool>,
    app_context: &'static Mutex<CriticalSectionRawMutex, AppContext>,
    dfu_resources: &'static DfuResources,
) {
    loop {
        match advertise("dc-mini", peripheral, server).await {
            Ok(conn) => {
                let gatt = gatt_server_task(
                    server,
                    &conn,
                    app_context,
                    dfu_resources,
                );
                let ads = ads_stream_notify(server, &conn);
                let mic = mic_stream_notify(server, &conn);
                futures::pin_mut!(gatt, ads, mic);
                embassy_futures::select::select3(gatt, ads, mic).await;
                // Release DFU lock if connection drops mid-transfer
                dfu_resources.finish();
            }
            Err(e) => {
                error!("Advertisement error: {:?}", e);
                embassy_time::Timer::after_secs(1).await;
            }
        }
    }
}

#[embassy_executor::task]
pub async fn ble_run_task(
    controller: BleController,
    app_context: &'static Mutex<CriticalSectionRawMutex, AppContext>,
    dfu_resources: &'static DfuResources,
) {
    run(controller, app_context, dfu_resources).await;
}
