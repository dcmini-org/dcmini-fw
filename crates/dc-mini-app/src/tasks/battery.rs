use crate::prelude::*;
use embassy_nrf::Peri;
use embassy_nrf::{
    bind_interrupts,
    gpio::{AnyPin, Level, Output, OutputDrive},
    peripherals::SAADC,
    saadc::{self, AnyInput, ChannelConfig, Config, Saadc},
};
use portable_atomic::{AtomicI16, Ordering};

pub static BATT_LVL: AtomicI16 = AtomicI16::new(0);

/// Reads the current ADC value every second and notifies the connected client.
#[embassy_executor::task]
pub async fn battery_monitor(
    vdiv_pin: AnyInput<'static>,
    vsense_pin: Peri<'static, AnyPin>,
    adc: Peri<'static, SAADC>,
) {
    // Then we initialize the ADC. We are only using one channel in this example.
    let config = Config::default();
    let channel_cfg = ChannelConfig::single_ended(vdiv_pin);
    interrupt::SAADC.set_priority(interrupt::Priority::P3);
    bind_interrupts!(struct BatteryIrqs {SAADC => saadc::InterruptHandler;});
    let mut saadc = Saadc::new(adc, BatteryIrqs, config, [channel_cfg]);
    // Indicated: wait for ADC calibration.
    saadc.calibrate().await;

    let mut vsense =
        Output::new(vsense_pin, Level::Low, OutputDrive::Standard);

    let mut buf = [0i16; 1];
    loop {
        vsense.set_high();
        Timer::after(Duration::from_millis(100)).await;

        saadc.sample(&mut buf).await;

        // We only sampled one ADC channel.
        let adc_raw_value: i16 = buf[0];

        // Store latest value for use elsewhere.
        BATT_LVL.store(adc_raw_value, Ordering::Release);

        // Sleep for ten seconds.
        vsense.set_low();
        Timer::after(Duration::from_secs(10)).await
    }
}
