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
pub mod status;

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
pub use status::*;

use super::Error;

use crate::prelude::{
    error, info, report_status, AppContext, CriticalSectionRawMutex, Mutex,
};
use dc_mini_icd as icd;
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

    let external_flash_available = {
        let app_ctx = app_context.lock().await;
        app_ctx.state.external_flash_available
    };

    info!("Starting BLE advertising and GATT service");

    if external_flash_available {
        let server = match ServerWithDfu::new_with_config(GapConfig::Peripheral(
            PeripheralConfig {
                name: "dc-mini",
                appearance: &appearance::sensor::MULTI_SENSOR,
            },
        )) {
            Ok(server) => server,
            Err(e) => {
                error!("Error creating GATT server: {:?}", e);
                report_status(
                    icd::SubsystemId::BleStream,
                    icd::SubsystemState::Unavailable,
                    icd::FaultCode::BleInitFailed,
                )
                .await;
                return;
            }
        };

        let app_loop = app_task_with_dfu(
            &server,
            &mut peripheral,
            app_context,
            dfu_resources,
        );
        let _ =
            embassy_futures::join::join(ble_runner(runner), app_loop).await;
    } else {
        let server = match ServerWithoutDfu::new_with_config(
            GapConfig::Peripheral(PeripheralConfig {
                name: "dc-mini",
                appearance: &appearance::sensor::MULTI_SENSOR,
            }),
        ) {
            Ok(server) => server,
            Err(e) => {
                error!("Error creating GATT server: {:?}", e);
                report_status(
                    icd::SubsystemId::BleStream,
                    icd::SubsystemState::Unavailable,
                    icd::FaultCode::BleInitFailed,
                )
                .await;
                return;
            }
        };

        let app_loop =
            app_task_without_dfu(&server, &mut peripheral, app_context);
        let _ =
            embassy_futures::join::join(ble_runner(runner), app_loop).await;
    }
}

async fn app_task_with_dfu<'values>(
    server: &ServerWithDfu<'values>,
    peripheral: &mut Peripheral<'values, BleController, DefaultPacketPool>,
    app_context: &'static Mutex<CriticalSectionRawMutex, AppContext>,
    dfu_resources: &'static DfuResources,
) {
    loop {
        match advertise_with_dfu("dc-mini", peripheral, server).await {
            Ok(conn) => {
                let gatt = gatt_server_task_with_dfu(
                    server,
                    &conn,
                    app_context,
                    dfu_resources,
                );
                let ads = ads_stream_notify_with_dfu(server, &conn);
                let mic = mic_stream_notify_with_dfu(server, &conn);
                let status = async {
                    status_notify_with_dfu(server, &conn).await;
                    core::future::pending::<()>().await;
                };
                let main = async {
                    futures::pin_mut!(gatt, ads, mic);
                    embassy_futures::select::select3(gatt, ads, mic).await;
                };
                futures::pin_mut!(main, status);
                embassy_futures::select::select(main, status).await;
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

async fn app_task_without_dfu<'values>(
    server: &ServerWithoutDfu<'values>,
    peripheral: &mut Peripheral<'values, BleController, DefaultPacketPool>,
    app_context: &'static Mutex<CriticalSectionRawMutex, AppContext>,
) {
    loop {
        match advertise_without_dfu("dc-mini", peripheral, server).await {
            Ok(conn) => {
                let gatt =
                    gatt_server_task_without_dfu(server, &conn, app_context);
                let ads = ads_stream_notify_without_dfu(server, &conn);
                let mic = mic_stream_notify_without_dfu(server, &conn);
                let status = async {
                    status_notify_without_dfu(server, &conn).await;
                    core::future::pending::<()>().await;
                };
                let main = async {
                    futures::pin_mut!(gatt, ads, mic);
                    embassy_futures::select::select3(gatt, ads, mic).await;
                };
                futures::pin_mut!(main, status);
                embassy_futures::select::select(main, status).await;
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
