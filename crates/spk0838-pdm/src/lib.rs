#![no_std]

//! Driver for the SPK0838HT4H PDM microphone.
//!
//! This is a thin wrapper around [`embassy_nrf::pdm::Pdm`] that encapsulates
//! SPK0838HT4H-specific configuration defaults and provides a microphone-oriented API.
//!
//! The SPK0838HT4H is a pure PDM output device with no registers — all configuration
//! happens on the nRF52840's PDM peripheral.

use embassy_nrf::gpio::Pin;
use embassy_nrf::interrupt;
use embassy_nrf::pdm::{
    self, Edge, Frequency, OperationMode, Pdm, Ratio, SamplerState,
};
use embassy_nrf::Peri;
use fixed::types::I7F1;

pub use embassy_nrf::pdm::Error;

/// Which PDM clock edge to sample the microphone data on.
///
/// This depends on the board's SELECT pin wiring:
/// - SELECT tied to GND → data on falling edge → [`Channel::Left`]
/// - SELECT tied to VDD → data on rising edge → [`Channel::Right`]
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Channel {
    /// Microphone data sampled on the falling clock edge (SELECT = GND).
    Left,
    /// Microphone data sampled on the rising clock edge (SELECT = VDD).
    Right,
}

impl Default for Channel {
    fn default() -> Self {
        // SPK0838HT4H with SELECT tied to GND (typical wiring)
        Self::Left
    }
}

/// Configuration for the SPK0838HT4H PDM microphone.
pub struct Config {
    /// Mono or stereo operation mode.
    pub mode: OperationMode,
    /// Which clock edge to sample data on, determined by SELECT pin wiring.
    pub channel: Channel,
    /// Gain in dB (0.5 dB steps via fixed-point I7F1). Range: -20.0 to +20.0 dB.
    pub gain_db: I7F1,
    /// PDM clock frequency. Must be within 1.0–3.25 MHz for the SPK0838HT4H.
    pub frequency: Frequency,
    /// Ratio between PDM_CLK and output sample rate.
    pub ratio: Ratio,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mode: OperationMode::Mono,
            channel: Channel::default(),
            gain_db: I7F1::ZERO,
            frequency: Frequency::DEFAULT,
            ratio: Ratio::RATIO80,
        }
    }
}

impl Config {
    fn into_pdm_config(self) -> pdm::Config {
        let edge = match self.channel {
            Channel::Left => Edge::LeftFalling,
            Channel::Right => Edge::LeftRising,
        };

        pdm::Config {
            operation_mode: self.mode,
            edge,
            frequency: self.frequency,
            ratio: self.ratio,
            gain_left: self.gain_db,
            gain_right: self.gain_db,
        }
    }
}

/// Driver for the SPK0838HT4H PDM microphone.
///
/// Wraps the embassy-nrf [`Pdm`] peripheral with SPK0838HT4H-specific defaults.
pub struct Spk0838<'d> {
    pdm: Pdm<'d>,
}

impl<'d> Spk0838<'d> {
    /// Create a new SPK0838HT4H microphone driver.
    ///
    /// # Arguments
    /// * `pdm` - The PDM peripheral instance
    /// * `irq` - Interrupt binding for the PDM peripheral
    /// * `clk` - GPIO pin connected to the microphone CLK line
    /// * `din` - GPIO pin connected to the microphone DATA line
    /// * `config` - Microphone configuration
    pub fn new<T: pdm::Instance>(
        pdm: Peri<'d, T>,
        irq: impl interrupt::typelevel::Binding<
                T::Interrupt,
                pdm::InterruptHandler<T>,
            > + 'd,
        clk: Peri<'d, impl Pin>,
        din: Peri<'d, impl Pin>,
        config: Config,
    ) -> Self {
        Self { pdm: Pdm::new(pdm, irq, clk, din, config.into_pdm_config()) }
    }

    /// Start the PDM clock, waking the microphone from sleep.
    ///
    /// Call this before [`sample`](Self::sample) to allow the microphone's
    /// startup time (~10 ms) to elapse.
    pub async fn start(&mut self) {
        self.pdm.start().await;
    }

    /// Stop the PDM clock. The microphone enters sleep mode.
    pub async fn stop(&mut self) {
        self.pdm.stop().await;
    }

    /// Capture a single buffer of PCM samples.
    ///
    /// The PDM must be started with [`start`](Self::start) before calling this.
    pub async fn sample(&mut self, buf: &mut [i16]) -> Result<(), Error> {
        self.pdm.sample(buf).await
    }

    /// Run a continuous double-buffered sampler.
    ///
    /// The `sampler` callback is called each time a buffer is filled. Return
    /// [`SamplerState::Sampled`] to continue or [`SamplerState::Stopped`] to finish.
    pub async fn run_sampler<S, const N: usize>(
        &mut self,
        bufs: &mut [[i16; N]; 2],
        sampler: S,
    ) -> Result<(), Error>
    where
        S: FnMut(&[i16; N]) -> SamplerState,
    {
        self.pdm.run_task_sampler(bufs, sampler).await
    }

    /// Adjust the microphone gain at runtime.
    ///
    /// Gain is in dB with 0.5 dB resolution (I7F1 fixed-point).
    /// Range: -20.0 to +20.0 dB. Values outside this range are clamped.
    pub fn set_gain(&mut self, gain_db: I7F1) {
        self.pdm.set_gain(gain_db, gain_db);
    }
}
