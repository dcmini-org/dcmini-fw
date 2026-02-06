#![no_std]
#![no_main]

#[cfg(feature = "external-flash")]
use core::cell::RefCell;

use cortex_m_rt::{entry, exception};
#[cfg(feature = "external-flash")]
use dc_mini_bsp::*;
#[cfg(feature = "defmt")]
use defmt_rtt as _;
// Force-link embassy-nrf so the PAC's device.x interrupt vectors are available.
#[cfg(feature = "external-flash")]
use embassy_boot_nrf::*;
use embassy_nrf as _;
#[cfg(feature = "external-flash")]
use embassy_nrf::nvmc::Nvmc;
#[cfg(feature = "external-flash")]
use embassy_nrf::wdt::{self, HaltConfig, SleepConfig};
#[cfg(feature = "external-flash")]
use embassy_sync::blocking_mutex::Mutex;

#[entry]
fn main() -> ! {
    #[cfg(feature = "external-flash")]
    {
        let mut board = DCMini::default();

        let mut wdt_config = wdt::Config::default();
        wdt_config.timeout_ticks = 32768 * 5; // timeout seconds
        wdt_config.action_during_sleep = SleepConfig::RUN;
        wdt_config.action_during_debug_halt = HaltConfig::PAUSE;

        let flash =
            WatchdogFlash::start(Nvmc::new(board.nvmc), board.wdt, wdt_config);
        let flash = Mutex::new(RefCell::new(flash));

        let external_flash = board.external_flash.configure();
        let external_flash = Mutex::new(RefCell::new(external_flash));

        let config = BootLoaderConfig::from_linkerfile_blocking(
            &flash,
            &external_flash,
            &flash,
        );
        let active_offset = config.active.offset();
        let bl: BootLoader = BootLoader::prepare(config);

        #[cfg(feature = "defmt")]
        defmt::info!("Loading Application!");

        unsafe { bl.load(active_offset) }
    }

    #[cfg(not(feature = "external-flash"))]
    {
        #[cfg(feature = "defmt")]
        defmt::info!("Loading Application (no DFU)!");

        // No external flash available for DFU — boot directly to the
        // application at the ACTIVE partition start address.
        unsafe { cortex_m::asm::bootload(0x0000_7000 as *const u32) }
    }
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
