#![no_std]

use embassy_nrf::{
    gpio::Pin,
    pwm::{
        self, Config, Error, Instance, Prescaler, SequenceConfig,
        SequenceLoad, SequencePwm, SingleSequenceMode, SingleSequencer,
    },
    Peri,
};
use embassy_time::Timer;
use smart_leds_trait::{SmartLedsWriteAsync, RGB8};

/// WS2812 0-bit high time in ns.
const T0H_NS: u32 = 400;
/// WS2812 1-bit high time in ns.
const T1H_NS: u32 = 800;
/// WS2812 total frame time in ns.
const FRAME_NS: u32 = 1250;
/// WS2812 frame reset time in µs. (50µs)
const RESET_NS: u32 = 50_000;

const DELAY_NS: u64 = FRAME_NS as u64 + (RESET_NS as u64);

/// Convert nanoseconds to PWM ticks, rounding.
const fn to_ticks(ns: u32) -> u32 {
    // Convert Hz to MHz to avoid overflow
    const PWM_CLOCK_MHZ: u32 = pwm::PWM_CLK_HZ / 1_000_000; // 16 MHz

    (ns * PWM_CLOCK_MHZ + 500) / 1_000
}

const RES: u16 = 0x8000;

/// WS2812 frame reset time in PWM ticks.
const RESET_TICKS: u32 = to_ticks(RESET_NS);

/// Samples for PWM array, with flip bits.
const BITS: [u16; 2] = [
    // 0-bit high time in ticks.
    to_ticks(T0H_NS) as u16 | RES,
    // 1-bit high time in ticks.
    to_ticks(T1H_NS) as u16 | RES,
];
/// Total PWM period in ticks.
const PWM_PERIOD: u16 = to_ticks(FRAME_NS) as u16;

pub struct Ws2812<'d, T: Instance, const N: usize> {
    seq_pwm: SequencePwm<'d, T>,
    seq_words: [u16; N],
    seq_config: SequenceConfig,
}

impl<'d, T: Instance, const N: usize> Ws2812<'d, T, N> {
    pub fn new(pwm: Peri<'d, T>, pin: Peri<'d, impl Pin>) -> Self {
        let mut config = Config::default();
        config.sequence_load = SequenceLoad::Common;
        config.prescaler = Prescaler::Div1;
        config.max_duty = PWM_PERIOD; // 1.25us (1s / 16Mhz * 20)

        let seq_pwm = SequencePwm::new_1ch(pwm, pin, config).unwrap();

        let mut seq_words = [0; N];
        if let Some(last) = seq_words.last_mut() {
            *last = RES;
        }

        let mut seq_config = SequenceConfig::default();
        seq_config.end_delay = RESET_TICKS - 1; // - 1 tick because we've already got one RES;

        Ws2812 { seq_pwm, seq_words, seq_config }
    }
}

impl<'d, T: Instance, const N: usize> SmartLedsWriteAsync
    for Ws2812<'d, T, N>
{
    type Error = Error;
    type Color = RGB8;

    /// Write all the items of an iterator to a ws2812 strip
    async fn write<C, I>(&mut self, iterator: C) -> Result<(), Self::Error>
    where
        C: IntoIterator<Item = I>,
        I: Into<Self::Color>,
    {
        for (color, words) in
            iterator.into_iter().zip(self.seq_words.chunks_mut(24))
        {
            let color = color.into();
            let color = (u32::from(color.g) << 16)
                | (u32::from(color.r) << 8)
                | (u32::from(color.b));
            for (i, word) in words.iter_mut().enumerate() {
                let val = (color >> (23 - i)) & 1;
                *word = BITS[val as usize]
            }
        }

        let sequencer = SingleSequencer::new(
            &mut self.seq_pwm,
            &self.seq_words,
            self.seq_config.clone(),
        );
        sequencer.start(SingleSequenceMode::Times(1)).unwrap();
        Timer::after_nanos(DELAY_NS).await;
        sequencer.stop();

        Ok(())
    }
}
