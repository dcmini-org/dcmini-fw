#![no_std]
#![no_main]

use core::cell::RefCell;

use cortex_m_rt::{entry, exception};
use dc_mini_bsp::*;
#[cfg(feature = "defmt")]
use defmt_rtt as _;
use embassy_boot_nrf::*;
use embassy_nrf::nvmc::Nvmc;
use embassy_nrf::wdt::{self, HaltConfig, SleepConfig};
use embassy_sync::blocking_mutex::Mutex;

fn active_partition_start() -> u32 {
    unsafe extern "C" {
        static __bootloader_active_start: u32;
    }

    unsafe { &__bootloader_active_start as *const u32 as u32 }
}

#[entry]
fn main() -> ! {
    let mut board = DCMini::default();

    // Uncomment this if you are debugging the bootloader with debugger/RTT attached,
    // as it prevents a hard fault when accessing flash 'too early' after boot.
    // for i in 0..10000000 {
    //     cortex_m::asm::nop();
    // }

    let mut wdt_config = wdt::Config::default();
    wdt_config.timeout_ticks = 32768 * 5; // timeout seconds
    wdt_config.action_during_sleep = SleepConfig::RUN;
    wdt_config.action_during_debug_halt = HaltConfig::PAUSE;

    let flash =
        WatchdogFlash::start(Nvmc::new(board.nvmc), board.wdt, wdt_config);
    let flash = Mutex::new(RefCell::new(flash));

    let external_flash = match board.external_flash.configure() {
        Ok(external_flash) => Some(external_flash),
        Err(ExternalFlashConfigureError::Unavailable) => None,
        Err(ExternalFlashConfigureError::Flash(_)) => {
            #[cfg(feature = "defmt")]
            defmt::error!(
                "External flash init failed on DFU-capable hardware, resetting"
            );
            loop {
                cortex_m::peripheral::SCB::sys_reset();
            }
        }
    };

    let active_start = active_partition_start();
    let (bl, active_offset): (
        BootLoader<{ embassy_nrf::nvmc::PAGE_SIZE }>,
        u32,
    ) = if let Some(external_flash) = external_flash {
        let external_flash = Mutex::new(RefCell::new(external_flash));
        let config = BootLoaderConfig::from_linkerfile_blocking(
            &flash,
            &external_flash,
            &flash,
        );
        let active_offset = config.active.offset();
        (
            BootLoader::<{ embassy_nrf::nvmc::PAGE_SIZE }>::prepare(config),
            active_offset,
        )
    } else {
        #[cfg(feature = "defmt")]
        defmt::warn!("External flash unavailable, booting active image directly");
        (BootLoader::<{ embassy_nrf::nvmc::PAGE_SIZE }>, active_start)
    };

    #[cfg(feature = "defmt")]
    defmt::info!("Loading Application!");

    unsafe { bl.load(active_offset) }
}

#[unsafe(no_mangle)]
#[cfg_attr(target_os = "none", unsafe(link_section = ".HardFault.user"))]
unsafe extern "C" fn HardFault() {
    cortex_m::peripheral::SCB::sys_reset();
}

#[exception]
unsafe fn DefaultHandler(_: i16) -> ! {
    const SCB_ICSR: *const u32 = 0xE000_ED04 as *const u32;
    let irqn = unsafe { core::ptr::read_volatile(SCB_ICSR) } as u8 as i16 - 16;

    panic!("DefaultHandler #{:?}", irqn);
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    cortex_m::asm::udf();
}
