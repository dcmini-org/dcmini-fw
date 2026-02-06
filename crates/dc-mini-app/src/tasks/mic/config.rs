use dc_mini_icd::{MicConfig, MicSampleRate};
use embassy_nrf::pdm::{Frequency, Ratio};
use fixed::types::I7F1;

/// Convert an ICD `MicConfig` into the SPK0838 driver `Config`.
pub fn to_driver_config(config: &MicConfig) -> spk0838_pdm::Config {
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
        ..Default::default()
    }
}
