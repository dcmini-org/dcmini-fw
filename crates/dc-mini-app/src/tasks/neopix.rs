use crate::prelude::*;
use embassy_nrf::gpio::AnyPin;
use embassy_nrf::peripherals;
use embassy_nrf::pwm::Error as PwmError;
use embassy_nrf::Peri;
use embassy_sync::channel::Channel;
use embassy_time::{Duration, Instant, Timer};
use smart_leds::{brightness, colors, SmartLedsWriteAsync, RGB8};
use ws2812_nrf_pwm::Ws2812;

pub static NEOPIX_CHAN: Channel<CriticalSectionRawMutex, NeopixEvent, 4> =
    Channel::new();

#[derive(Debug)]
pub enum NeopixEvent {
    PowerOn,
    PowerOff,
    Recording,
    Color(RGB8),
    Flash(RGB8, Duration, Option<u8>), // Color, blink interval, duty cycle (0-100)
    FlashFor(RGB8, Duration, u32, Option<u8>), // Color, blink interval, number of cycles, duty cycle
    OnFor(RGB8, Duration),                     // Color and duration to stay on
}

#[cfg(feature = "defmt")]
impl defmt::Format for NeopixEvent {
    fn format(&self, f: defmt::Formatter) {
        match self {
            NeopixEvent::PowerOn => defmt::write!(f, "PowerOn"),
            NeopixEvent::PowerOff => defmt::write!(f, "PowerOff"),
            NeopixEvent::Recording => defmt::write!(f, "Recording"),
            NeopixEvent::Color(c) => defmt::write!(f, "Color({},{},{})", c.r, c.g, c.b),
            NeopixEvent::Flash(c, d, dc) => defmt::write!(f, "Flash({},{},{}, {:?}, {:?})", c.r, c.g, c.b, d, dc),
            NeopixEvent::FlashFor(c, d, n, dc) => defmt::write!(f, "FlashFor({},{},{}, {:?}, {}, {:?})", c.r, c.g, c.b, d, n, dc),
            NeopixEvent::OnFor(c, d) => defmt::write!(f, "OnFor({},{},{}, {:?})", c.r, c.g, c.b, d),
        }
    }
}

const BRIGHTNESS: u8 = 10;
const DEFAULT_DUTY_CYCLE: u8 = 50;

struct NeopixState {
    current_color: RGB8,
    mode: NeopixMode,
    end_time: Option<Instant>,
    remaining_cycles: Option<u32>,
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
enum NeopixMode {
    Off,
    Solid,
    Flashing { on_time: Duration, off_time: Duration },
}

impl NeopixState {
    fn new() -> Self {
        Self {
            current_color: colors::BLACK,
            mode: NeopixMode::Off,
            end_time: None,
            remaining_cycles: None,
        }
    }

    fn calculate_flash_times(
        interval: Duration,
        duty_cycle: u8,
    ) -> (Duration, Duration) {
        let duty_cycle = duty_cycle.clamp(0, 100);
        let on_time = interval
            .checked_mul(duty_cycle.into())
            .expect("Failed to multiply duty cycle.")
            .checked_div(100)
            .expect("Failed to divide duty cycle.");
        let off_time = interval - on_time;
        (on_time, off_time)
    }

    async fn update<'a>(
        &mut self,
        ws: &mut Ws2812<'a, 25>,
    ) -> Result<(), PwmError> {
        // Check if we've reached the end time for timed operations
        if let Some(end_time) = self.end_time {
            if Instant::now() >= end_time {
                self.mode = NeopixMode::Off;
                self.current_color = colors::BLACK;
                self.end_time = None;
            }
        }

        match self.mode {
            NeopixMode::Off => {
                ws.write([colors::BLACK; 1].into_iter()).await?;
            }
            NeopixMode::Solid => {
                let color = [self.current_color; 1];
                let dimmed = brightness(color.into_iter(), BRIGHTNESS);
                ws.write(dimmed).await?;
            }
            NeopixMode::Flashing { on_time, off_time } => {
                // Write current color
                let color = [self.current_color; 1];
                let dimmed = brightness(color.into_iter(), BRIGHTNESS);
                ws.write(dimmed).await?;

                Timer::after(on_time).await;

                ws.write(brightness([colors::BLACK; 1].into_iter(), 0))
                    .await?;

                Timer::after(off_time).await;

                // Decrement cycle count if we're counting cycles
                if let Some(cycles) = self.remaining_cycles.as_mut() {
                    *cycles = cycles.saturating_sub(1);
                    if *cycles == 0 {
                        self.mode = NeopixMode::Off;
                        self.current_color = colors::BLACK;
                        self.remaining_cycles = None;
                    }
                }
            }
        }
        Ok(())
    }

    fn handle_event(&mut self, evt: NeopixEvent) {
        match evt {
            NeopixEvent::PowerOn => {
                let (on_time, off_time) =
                    Self::calculate_flash_times(Duration::from_secs(3), 5);
                self.mode = NeopixMode::Flashing { on_time, off_time };
                self.current_color = colors::ALICE_BLUE;
                self.end_time = None;
                self.remaining_cycles = None;
            }
            NeopixEvent::PowerOff => {
                self.mode = NeopixMode::Off;
                self.current_color = colors::BLACK;
                self.end_time = None;
                self.remaining_cycles = None;
            }
            NeopixEvent::Recording => {
                let (on_time, off_time) =
                    Self::calculate_flash_times(Duration::from_secs(2), 25);
                self.mode = NeopixMode::Flashing { on_time, off_time };
                self.current_color = colors::MEDIUM_VIOLET_RED;
                self.end_time = None;
                self.remaining_cycles = None;
            }
            NeopixEvent::Color(color) => {
                self.mode = NeopixMode::Solid;
                self.current_color = color;
                self.end_time = None;
                self.remaining_cycles = None;
            }
            NeopixEvent::Flash(color, interval, duty_cycle) => {
                let (on_time, off_time) = Self::calculate_flash_times(
                    interval,
                    duty_cycle.unwrap_or(DEFAULT_DUTY_CYCLE),
                );
                self.mode = NeopixMode::Flashing { on_time, off_time };
                self.current_color = color;
                self.end_time = None;
                self.remaining_cycles = None;
            }
            NeopixEvent::FlashFor(color, interval, cycles, duty_cycle) => {
                if cycles > 0 {
                    let (on_time, off_time) = Self::calculate_flash_times(
                        interval,
                        duty_cycle.unwrap_or(DEFAULT_DUTY_CYCLE),
                    );
                    self.mode = NeopixMode::Flashing { on_time, off_time };
                    self.current_color = color;
                    self.end_time = None;
                    self.remaining_cycles = Some(cycles);
                } else {
                    self.mode = NeopixMode::Off;
                    self.current_color = colors::BLACK;
                    self.end_time = None;
                    self.remaining_cycles = None;
                }
            }
            NeopixEvent::OnFor(color, duration) => {
                self.mode = NeopixMode::Solid;
                self.current_color = color;
                self.end_time = Some(Instant::now() + duration);
                self.remaining_cycles = None;
            }
        }
    }

    fn is_flashing(&self) -> bool {
        matches!(self.mode, NeopixMode::Flashing { .. })
    }
}

#[embassy_executor::task]
pub async fn neopix_task(
    pwm: Peri<'static, peripherals::PWM0>,
    pin: Peri<'static, AnyPin>,
) {
    let receiver = NEOPIX_CHAN.receiver();
    let mut ws: Ws2812<'_, 25> = Ws2812::new(pwm, pin);
    let mut state = NeopixState::new();
    state.handle_event(NeopixEvent::PowerOn);
    unwrap!(state.update(&mut ws).await);

    loop {
        // Check for new events with timeout if we're flashing
        let evt = if state.is_flashing() {
            match receiver.try_receive() {
                Ok(evt) => Some(evt),
                Err(_) => None,
            }
        } else {
            Some(receiver.receive().await)
        };

        if let Some(evt) = evt {
            state.handle_event(evt);
        }

        unwrap!(state.update(&mut ws).await);
    }
}
