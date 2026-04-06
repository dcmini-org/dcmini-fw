use crate::prelude::*;
use dc_mini_bsp::usb::UsbDriverBuilder;
use embassy_nrf::usb::Driver;
use embassy_usb::Config;
use static_cell::ConstStaticCell;

// Re-exports
use postcard_rpc::{
    define_dispatch,
    server::{
        impls::embassy_usb_v0_5::{
            dispatch_impl::{
                spawn_fn, WireRxBuf, WireRxImpl, WireSpawnImpl, WireStorage,
                WireTxImpl,
            },
            PacketBuffers,
        },
        Dispatch, Server, SpawnContext,
    },
};

mod ads;
mod battery;
mod device_info;
mod dfu;
mod mic;
mod profile;
mod session;
mod system;

use ads::*;
use battery::*;
use device_info::*;
use dfu::*;
use mic::*;
use profile::*;
use session::*;
use system::*;

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;

// Postcard types
type MutexType = CriticalSectionRawMutex;
pub type AppTx = WireTxImpl<MutexType, AppDriver>;
type AppRx = WireRxImpl<AppDriver>;
type AppServer<D> = Server<AppTx, AppRx, WireRxBuf, D>;

type AppDriver =
    Driver<'static, embassy_nrf::usb::vbus_detect::HardwareVbusDetect>;
type AppStorage = WireStorage<MutexType, AppDriver, 256, 256, 64, 256>;
type BufStorage = PacketBuffers<1024, 1024>;

// Statics
static PBUFS: ConstStaticCell<BufStorage> =
    ConstStaticCell::new(BufStorage::new());
static STORAGE: AppStorage = AppStorage::new();

pub struct Context {
    pub app: &'static Mutex<MutexType, AppContext>,
    pub dfu: &'static crate::tasks::dfu::DfuResources,
}

mod dispatch_full {
    use super::*;

    define_dispatch! {
        app: DcMiniUsbApp;
        spawn_fn: spawn_fn;
        tx_impl: AppTx;
        spawn_impl: WireSpawnImpl;
        context: Context;

        endpoints: {
            list: ENDPOINT_LIST;

            | EndpointTy                | kind      | handler                       |
            | ----------                | ----      | -------                       |
            | AdsStartEndpoint          | spawn     | ads_start_handler             |
            | AdsStopEndpoint           | async     | ads_stop_handler              |
            | AdsResetConfigEndpoint    | async     | ads_reset_config              |
            | AdsGetConfigEndpoint      | async     | ads_get_config                |
            | AdsSetConfigEndpoint      | async     | ads_set_config                |
            | MicStartEndpoint          | spawn     | mic_start_handler             |
            | MicStopEndpoint           | async     | mic_stop_handler              |
            | MicGetConfigEndpoint      | async     | mic_get_config                |
            | MicSetConfigEndpoint      | async     | mic_set_config                |
            | BatteryGetLevelEndpoint   | async     | battery_get_level             |
            | DeviceInfoGetEndpoint     | async     | device_info_get               |
            | ProfileGetEndpoint        | async     | profile_get                   |
            | ProfileSetEndpoint        | async     | profile_set                   |
            | ProfileCommandEndpoint    | async     | profile_command               |
            | SessionGetStatusEndpoint  | async     | session_get_status            |
            | SessionGetIdEndpoint      | async     | session_get_id                |
            | SessionSetIdEndpoint      | async     | session_set_id                |
            | SessionStartEndpoint      | async     | session_start                 |
            | SessionStopEndpoint       | async     | session_stop                  |
            | SystemStatusGetEndpoint   | async     | system_status_get             |
            | DfuBeginEndpoint          | async     | dfu_begin                     |
            | DfuWriteEndpoint          | async     | dfu_write                     |
            | DfuFinishEndpoint         | async     | dfu_finish                    |
            | DfuAbortEndpoint          | async     | dfu_abort                     |
            | DfuStatusEndpoint         | async     | dfu_status                    |
        };
        topics_in: {
            list: TOPICS_IN_LIST;

            | TopicTy                   | kind      | handler                       |
            | ----------                | ----      | -------                       |
        };
        topics_out: {
            list: TOPICS_OUT_LIST;
        };
    }
}

mod dispatch_no_dfu {
    use super::*;

    define_dispatch! {
        app: DcMiniUsbAppNoDfu;
        spawn_fn: spawn_fn;
        tx_impl: AppTx;
        spawn_impl: WireSpawnImpl;
        context: Context;

        endpoints: {
            list: ENDPOINT_LIST;

            | EndpointTy                | kind      | handler                       |
            | ----------                | ----      | -------                       |
            | AdsStartEndpoint          | spawn     | ads_start_handler             |
            | AdsStopEndpoint           | async     | ads_stop_handler              |
            | AdsResetConfigEndpoint    | async     | ads_reset_config              |
            | AdsGetConfigEndpoint      | async     | ads_get_config                |
            | AdsSetConfigEndpoint      | async     | ads_set_config                |
            | MicStartEndpoint          | spawn     | mic_start_handler             |
            | MicStopEndpoint           | async     | mic_stop_handler              |
            | MicGetConfigEndpoint      | async     | mic_get_config                |
            | MicSetConfigEndpoint      | async     | mic_set_config                |
            | BatteryGetLevelEndpoint   | async     | battery_get_level             |
            | DeviceInfoGetEndpoint     | async     | device_info_get               |
            | ProfileGetEndpoint        | async     | profile_get                   |
            | ProfileSetEndpoint        | async     | profile_set                   |
            | ProfileCommandEndpoint    | async     | profile_command               |
            | SessionGetStatusEndpoint  | async     | session_get_status            |
            | SessionGetIdEndpoint      | async     | session_get_id                |
            | SessionSetIdEndpoint      | async     | session_set_id                |
            | SessionStartEndpoint      | async     | session_start                 |
            | SessionStopEndpoint       | async     | session_stop                  |
            | SystemStatusGetEndpoint   | async     | system_status_get             |
        };
        topics_in: {
            list: TOPICS_IN_LIST;

            | TopicTy                   | kind      | handler                       |
            | ----------                | ----      | -------                       |
        };
        topics_out: {
            list: TOPICS_OUT_LIST;
        };
    }
}

pub use dispatch_full::DcMiniUsbApp;
pub use dispatch_no_dfu::DcMiniUsbAppNoDfu;

// Structs
pub struct SpawnCtx {
    pub app: &'static Mutex<CriticalSectionRawMutex, AppContext>,
    pub dfu: &'static crate::tasks::dfu::DfuResources,
}

impl SpawnContext for Context {
    type SpawnCtxt = SpawnCtx;
    fn spawn_ctxt(&mut self) -> Self::SpawnCtxt {
        SpawnCtx { app: self.app, dfu: self.dfu }
    }
}

async fn run_usb<D>(
    dispatcher: D,
    usbd: UsbDriverBuilder,
) where
    D: Dispatch<Tx = AppTx>,
{
    let vkk = dispatcher.min_key_len();

    let driver = usbd.init();
    let pbufs = PBUFS.take();
    let config = usb_config();

    let (mut device, tx_impl, rx_impl) =
        STORAGE.init(driver, config, pbufs.tx_buf.as_mut_slice(), 64);

    let mut server: AppServer<D> = Server::new(
        tx_impl,
        rx_impl,
        pbufs.rx_buf.as_mut_slice(),
        dispatcher,
        vkk,
    );

    let status_fut = system_status_topic_task(server.sender());

    let server_fut = async {
        // Need to allow time for the USB driver to intialize prior to running the postcard server.
        Timer::after(Duration::from_secs(2)).await;
        info!("Starting Postcard Server...");
        server.run().await;
    };

    let status_fut = async {
        Timer::after(Duration::from_secs(2)).await;
        status_fut.await;
    };

    let _ = embassy_futures::join::join3(server_fut, device.run(), status_fut)
        .await;
    warn!("Exiting usb_task!!");
}

// USB configuration
fn usb_config() -> Config<'static> {
    let mut config = Config::new(0x16c0, 0x27DD);
    config.manufacturer = Some("JHUAPL");
    config.product = Some("dc-mini");
    config.serial_number = Some("12345678");

    // Required for windows compatibility.
    // https://developer.nordicsemi.com/nRF_Connect_SDK/doc/1.9.1/kconfig/CONFIG_CDC_ACM_IAD.html#help
    config.device_class = 0xEF;
    config.device_sub_class = 0x02;
    config.device_protocol = 0x01;
    config.composite_with_iads = true;

    config
}

#[embassy_executor::task]
pub async fn usb_task(
    spawner: Spawner,
    usbd: UsbDriverBuilder,
    app_context: &'static Mutex<CriticalSectionRawMutex, AppContext>,
    dfu_resources: &'static crate::tasks::dfu::DfuResources,
) {
    let external_flash_available = {
        let ctx = app_context.lock().await;
        ctx.state.external_flash_available
    };
    let context = Context { app: app_context, dfu: dfu_resources };

    if external_flash_available {
        let dispatcher = DcMiniUsbApp::new(context, spawner.into());
        run_usb(dispatcher, usbd).await;
    } else {
        let dispatcher = DcMiniUsbAppNoDfu::new(context, spawner.into());
        run_usb(dispatcher, usbd).await;
    }
}
