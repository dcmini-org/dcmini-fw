use crate::events::{ButtonPress, Event};
use crate::prelude::*;
use embassy_nrf::gpio::{AnyPin, Input, Pull};
use embassy_nrf::peripherals::WDT;
use embassy_nrf::wdt;
use embassy_nrf::wdt::Watchdog;
use embassy_nrf::Peri;
use embassy_time::Instant;

pub mod ads;
pub mod apds;
pub mod blinky;
pub mod imu;
pub mod mic;
pub mod neopix;
pub mod power_control;
pub mod session;

#[cfg(feature = "trouble")]
pub mod ble;
#[cfg(feature = "demo")]
pub mod demo;
#[cfg(feature = "usb")]
pub mod usb;

// Re-exports
pub use ads::*;
pub use apds::*;
#[cfg(feature = "trouble")]
pub use ble::*;
pub use blinky::*;
#[cfg(feature = "demo")]
pub use demo::*;
pub use imu::*;
pub use mic::*;
pub use neopix::*;
pub use power_control::*;
pub use session::*;
#[cfg(feature = "usb")]
pub use usb::*;

// Keeps our system alive
#[embassy_executor::task]
pub async fn watchdog_task(wdt: Peri<'static, WDT>) {
    let wdt_config = wdt::Config::try_new(&wdt).unwrap();
    let (_wdt, [mut handle]) = match Watchdog::try_new(wdt, wdt_config) {
        Ok(x) => x,
        Err(_) => {
            // Watchdog already active with the wrong number of handles, waiting for it to timeout...
            loop {
                cortex_m::asm::wfe();
            }
        }
    };
    loop {
        handle.pet();
        Timer::after(Duration::from_secs(2)).await;
    }
}

use embassy_time::{with_timeout, Duration, Timer};

#[embassy_executor::task]
pub async fn button_task(btn_pin: Peri<'static, AnyPin>, sender: EventSender) {
    const DOUBLE_CLICK_DELAY: u64 = 250;
    const HOLD_DELAY: u64 = 1000;

    let mut button = Input::new(btn_pin, Pull::Up);

    button.wait_for_falling_edge().await; // Wait for the first falling edge to initialize
    loop {
        if with_timeout(
            Duration::from_millis(HOLD_DELAY),
            button.wait_for_rising_edge(),
        )
        .await
        .is_err()
        {
            info!("Hold detected");
            sender.send(ButtonPress::Hold.into()).await;
        } else if with_timeout(
            Duration::from_millis(DOUBLE_CLICK_DELAY),
            button.wait_for_falling_edge(),
        )
        .await
        .is_err()
        {
            info!("Single click detected");
            sender.send(ButtonPress::Single.into()).await;
        } else {
            info!("Double click detected");
            sender.send(ButtonPress::Double.into()).await;
            button.wait_for_rising_edge().await; // Wait for the button to rise after the double click
        }
        button.wait_for_falling_edge().await; // Wait for the next button press
    }
}

#[embassy_executor::task]
pub async fn timer_task(duration: u64, sender: EventSender) {
    loop {
        Timer::after_millis(duration).await;
        sender.send(Event::TimerElapsed.into()).await;
    }
}

#[embassy_executor::task]
pub async fn heap_usage() {
    loop {
        Timer::after_secs(1).await;
        info!("Heap Usage = {:?}", crate::ALLOCATOR.usage());
    }
}

#[embassy_executor::task]
pub async fn log_stats() {
    const MSECS_PER_LOG: u64 = 1000;
    let mut receiver =
        ADS_MEAS_CH.subscriber().expect("Failed to create subscriber.");
    let mut num_samps = 0;
    let mut last_ts = Instant::now();
    loop {
        let data = receiver.next_message_pure().await;
        num_samps += 1;
        if Instant::now().duration_since(last_ts).as_millis() > MSECS_PER_LOG {
            info!(
                "Received {:?} blocks with {:?} samples each in {:?}ms",
                num_samps,
                data.len(),
                MSECS_PER_LOG
            );
            num_samps = 0;
            last_ts = Instant::now();
        }
    }
}
