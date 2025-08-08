use crate::prelude::*;
use embassy_nrf::gpio::AnyPin;
use embassy_nrf::peripherals::PWM0;
use embassy_nrf::pwm::{
    Config, Prescaler, SequenceConfig, SequenceLoad, SequencePwm,
    SingleSequenceMode, SingleSequencer,
};
use embassy_nrf::Peri;
use embassy_time::Timer;

const T1H: u16 = 0x8000 | 13; // Duty = 13/20 ticks (0.8us/1.25us) for a 1
const T0H: u16 = 0x8000 | 7; // Duty 7/20 ticks (0.4us/1.25us) for a 0
const RES: u16 = 0x8000;

#[embassy_executor::task]
pub async fn blinky_actor(
    neopix_pin: Peri<'static, AnyPin>,
    pwm: Peri<'static, PWM0>,
) {
    let mut config = Config::default();
    config.sequence_load = SequenceLoad::Common;
    config.prescaler = Prescaler::Div1;
    config.max_duty = 20; // 1.25us (1s / 16Mhz * 20)
    let mut pwm = unwrap!(SequencePwm::new_1ch(pwm, neopix_pin, config));

    // Declare the bits of 24 bits in a buffer we'll be
    // mutating later.
    let mut sequence = [
        T0H, T0H, T0H, T0H, T0H, T0H, T0H, T0H, // G
        T0H, T0H, T0H, T0H, T0H, T0H, T0H, T0H, // R
        T1H, T1H, T1H, T1H, T1H, T1H, T1H, T1H, // B
        RES,
    ];
    let mut seq_config = SequenceConfig::default();
    seq_config.end_delay = 799; // 50us (20 ticks * 40) - 1 tick because we've already got one RES;

    let mut color_bit = 16;
    let mut bit_value = T0H;

    loop {
        sequence[color_bit] = bit_value;
        // Pass the mutable reference to the sequencer
        let sequences =
            SingleSequencer::new(&mut pwm, &sequence, seq_config.clone());
        unwrap!(sequences.start(SingleSequenceMode::Times(1)));
        Timer::after_millis(50).await;

        if bit_value == T0H {
            if color_bit == 20 {
                bit_value = T1H;
            } else {
                color_bit += 1;
            }
        } else {
            if color_bit == 16 {
                bit_value = T0H;
            } else {
                color_bit -= 1;
            }
        }

        drop(sequences);
    }
}
