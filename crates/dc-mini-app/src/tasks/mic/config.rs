use dc_mini_icd::{MicConfig, MicSampleRate};
use embassy_nrf::pdm::{Frequency, Ratio};
use fixed::types::I7F1;

// SELECT is tied to ground on this board revision.
pub const DEFAULT_MIC_CHANNEL: spk0838_pdm::Channel = spk0838_pdm::Channel::Left;

/// Convert an ICD `MicConfig` into the SPK0838 driver `Config`.
pub fn to_driver_config(config: &MicConfig) -> spk0838_pdm::Config {
    to_driver_config_with_channel(config, DEFAULT_MIC_CHANNEL)
}

/// Convert an ICD `MicConfig` into the SPK0838 driver `Config`
/// with an explicit channel/edge selection.
pub fn to_driver_config_with_channel(
    config: &MicConfig,
    channel: spk0838_pdm::Channel,
) -> spk0838_pdm::Config {
    let gain_db = I7F1::from_num(config.gain_db);

    let (frequency, ratio) = match config.sample_rate {
        MicSampleRate::Rate16000 => (Frequency::_1280K, Ratio::RATIO80),
        MicSampleRate::Rate12800 => (Frequency::DEFAULT, Ratio::RATIO80),
        MicSampleRate::Rate20000 => (Frequency::_1280K, Ratio::RATIO64),
    };

    spk0838_pdm::Config {
        gain_db,
        frequency,
        ratio,
        channel,
        ..Default::default()
    }
}
