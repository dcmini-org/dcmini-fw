#![no_std]
#![feature(type_alias_impl_trait)]
#![feature(impl_trait_in_assoc_type)]
#[allow(async_fn_in_trait)]
extern crate alloc;

mod bus_manager;
mod clock;
pub mod events;
pub mod storage;
pub mod tasks;
mod util;

#[cfg(all(feature = "critical-section", feature = "trouble"))]
compile_error!("You must enable exactly one of the following features: `trouble`, `critical-section`");

use core::ptr::addr_of_mut;
use dc_mini_icd::DeviceInfo;
use embassy_executor::{InterruptExecutor, SendSpawner};
use embassy_nrf::interrupt;
use embassy_nrf::interrupt::{InterruptExt, Priority};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Receiver, Sender};
use embassy_sync::mutex::Mutex;
use embedded_alloc::LlffHeap;
use static_cell::StaticCell;
use storage::profile_manager::ProfileManager;

pub const HW_VERSION: &str = env!("HW_VERSION");
pub const FW_VERSION: &str = env!("FW_VERSION");
pub const MANUFACTURER: &str = "Johns Hopkins APL";

// Heap helpers
#[global_allocator]
pub static ALLOCATOR: trallocator::Trallocator<LlffHeap> =
    trallocator::Trallocator::new(LlffHeap::empty());
// static HEAP: LlffHeap = LlffHeap::empty();
pub fn init_heap() {
    use core::mem::MaybeUninit;
    const HEAP_SIZE: usize = 32 * 1024;
    static mut HEAP_MEM: [MaybeUninit<u8>; HEAP_SIZE] =
        [MaybeUninit::uninit(); HEAP_SIZE];
    unsafe {
        ALLOCATOR.borrow().init(addr_of_mut!(HEAP_MEM) as usize, HEAP_SIZE)
    }
}

const PROFILE_BUF_SZ: usize = 256;
pub type AppProfileManager = ProfileManager<
    embassy_embedded_hal::adapter::BlockingAsync<
        embassy_nrf::nvmc::Nvmc<'static>,
    >,
    PROFILE_BUF_SZ,
>;

pub static CLOCK: clock::Clock = clock::Clock::new();

pub struct State {
    pub usb_powered: bool,
    pub vsys_voltage: f32,
    pub recording_status: bool,
}

pub struct AppContext {
    pub device_info: DeviceInfo,
    pub high_prio_spawner: SendSpawner,
    pub medium_prio_spawner: SendSpawner,
    pub low_prio_spawner: SendSpawner,
    pub event_sender: EventSender,
    pub profile_manager: AppProfileManager,
    pub state: State,
}

impl AppContext {
    pub async fn save_ads_config(&mut self, config: prelude::AdsConfig) {
        match self.profile_manager.set_ads_config(config).await {
            Ok(_) => {
                self.event_sender
                    .send(prelude::AdsEvent::ConfigChanged.into())
                    .await;
            }
            Err(e) => {
                prelude::warn!("Failed to save ADS config: {:?}", e);
            }
        }
    }
    pub async fn save_imu_config(&mut self, config: prelude::ImuConfig) {
        match self.profile_manager.set_imu_config(config).await {
            Ok(_) => {
                self.event_sender
                    .send(prelude::ImuEvent::ConfigChanged.into())
                    .await;
            }
            Err(e) => {
                prelude::warn!("Failed to save IMU config: {:?}", e);
            }
        }
    }
    pub async fn save_apds_config(&mut self, config: prelude::ApdsConfig) {
        match self.profile_manager.set_apds_config(config).await {
            Ok(_) => {
                self.event_sender
                    .send(prelude::ApdsEvent::ConfigChanged.into())
                    .await;
            }
            Err(e) => {
                prelude::warn!("Failed to save APDS config: {:?}", e);
            }
        }
    }
    pub async fn save_mic_config(&mut self, config: prelude::MicConfig) {
        match self.profile_manager.set_mic_config(config).await {
            Ok(_) => {
                self.event_sender
                    .send(prelude::MicEvent::ConfigChanged.into())
                    .await;
            }
            Err(e) => {
                prelude::warn!("Failed to save MIC config: {:?}", e);
            }
        }
    }
}

// Statics
static EXECUTOR_HIGH: InterruptExecutor = InterruptExecutor::new();
static EXECUTOR_MED: InterruptExecutor = InterruptExecutor::new();
pub static APP_CONTEXT: StaticCell<
    Mutex<CriticalSectionRawMutex, AppContext>,
> = StaticCell::new();

const EVENT_CAPACITY: usize = 10;
pub type EventMutexType = CriticalSectionRawMutex;
pub type EventChannel = Channel<EventMutexType, events::Event, EVENT_CAPACITY>;
pub type EventSender =
    Sender<'static, EventMutexType, events::Event, EVENT_CAPACITY>;
pub type EventReceiver =
    Receiver<'static, EventMutexType, events::Event, EVENT_CAPACITY>;
static EVENT_CHANNEL: StaticCell<
    Channel<CriticalSectionRawMutex, events::Event, 10>,
> = StaticCell::new();
pub fn init_event_channel() -> (EventSender, EventReceiver) {
    let channel = EVENT_CHANNEL.init(Channel::new());
    (channel.sender(), channel.receiver())
}

// Interrupt executors
#[interrupt]
unsafe fn EGU0_SWI0() {
    EXECUTOR_MED.on_interrupt()
}

#[interrupt]
unsafe fn EGU1_SWI1() {
    EXECUTOR_HIGH.on_interrupt()
}

pub fn init_executors() -> (SendSpawner, SendSpawner) {
    // Medium-priority executor: EGU0_SWI0, priority level 7
    interrupt::EGU0_SWI0.set_priority(Priority::P7);
    let medium_prio_spawner = EXECUTOR_MED.start(interrupt::EGU0_SWI0);

    // High-priority executor: EGU1_SWI1, priority level 6
    interrupt::EGU1_SWI1.set_priority(Priority::P6);
    let high_prio_spawner = EXECUTOR_HIGH.start(interrupt::EGU1_SWI1);
    (medium_prio_spawner, high_prio_spawner)
}

pub mod prelude {
    pub use super::{
        bus_manager::*, error, events::*, info, init_executors, init_heap,
        storage::*, tasks::*, unwrap, warn, AppContext, AppProfileManager,
        EventReceiver, EventSender, State, CLOCK, FW_VERSION, HW_VERSION,
        MANUFACTURER,
    };
    pub use embassy_executor::Spawner;
    pub use embassy_nrf::bind_interrupts;
    pub use embassy_nrf::gpio::Pin;
    pub use embassy_nrf::interrupt;
    pub use embassy_nrf::interrupt::{InterruptExt, Priority};
    pub use embassy_sync::blocking_mutex::raw::{
        CriticalSectionRawMutex, NoopRawMutex, ThreadModeRawMutex,
    };
    pub use embassy_sync::mutex::Mutex;
    pub use embassy_time::{Duration, Timer};

    pub use dc_mini_bsp::{
        AdsResources, DCMini, ImuResources, MicResources, SdCardResources,
        Spi3BusResources, Twim1BusResources,
    };
    pub use dc_mini_icd::{
        self as icd,
        // MAX_ID_LEN, // AdsGetConfigEndpoint, AdsSetConfigEndpoint, AdsStartEndpoint,
        // AdsStopEndpoint, ENDPOINT_LIST, TOPICS_IN_LIST, TOPICS_OUT_LIST,
        *,
    };
}
