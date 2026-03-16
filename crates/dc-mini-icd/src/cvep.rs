use postcard_schema::Schema;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Serialize, Deserialize, Schema, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct CvepConfig {
    pub model_enabled: bool,
    pub channels: u8,
    pub classes: u8,
    pub window_samples: u16,
    pub inference_stride_samples: u16,
    pub score_threshold: Option<f32>,
    pub margin_threshold: Option<f32>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Schema, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct CvepDecision {
    pub ts: u64,
    pub class_index: u16,
    pub raw_score: i64,
    pub normalized_score: f32,
    pub margin: f32,
}
