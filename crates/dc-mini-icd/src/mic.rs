extern crate alloc;

use postcard_schema::Schema;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Serialize, Deserialize, Schema, Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum MicSampleRate {
    Rate16000,  // 16 kHz (1.280 MHz CLK / RATIO80)
    Rate12800,  // 12.8 kHz (1.032 MHz CLK / RATIO80) — DEFAULT frequency
    Rate20000,  // 20 kHz (1.280 MHz CLK / RATIO64)
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Schema, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct MicConfig {
    pub gain_db: i8,
    pub sample_rate: MicSampleRate,
}

impl Default for MicConfig {
    fn default() -> Self {
        Self {
            gain_db: 0,
            sample_rate: MicSampleRate::Rate16000,
        }
    }
}

impl From<u8> for MicSampleRate {
    fn from(value: u8) -> Self {
        match value {
            0 => MicSampleRate::Rate16000,
            1 => MicSampleRate::Rate12800,
            2 => MicSampleRate::Rate20000,
            _ => MicSampleRate::Rate16000,
        }
    }
}

impl MicSampleRate {
    pub fn as_hz(&self) -> u32 {
        match self {
            MicSampleRate::Rate16000 => 16000,
            MicSampleRate::Rate12800 => 12800,
            MicSampleRate::Rate20000 => 20000,
        }
    }
}

pub fn default_mic_settings() -> MicConfig {
    MicConfig::default()
}

#[derive(Debug, Serialize, Deserialize, Schema, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct MicDataFrame {
    pub ts: u64,
    pub packet_counter: u64,
    pub sample_rate: u32,
    pub predictor: i32,
    pub step_index: u32,
    pub adpcm_data: alloc::vec::Vec<u8>,
}
