use super::CvepDecisionEvent;
use cvep_decoder::{ChannelPreprocessor, EtRcaBank, SosCascade};
use dc_mini_icd::CvepConfig;
use static_cell::StaticCell;

pub const MODEL_ENABLED: bool = false;

pub const CHANNELS: usize = 8;
pub const CLASSES: usize = 20;
pub const WINDOW: usize = 125;
pub const INFERENCE_STRIDE: usize = WINDOW / 2;
pub const PREPROCESSING_SECTIONS: usize = 0;
pub const ADC_SCALE: f32 = 1.0;
pub const SCORE_THRESHOLD: Option<f32> = None;
pub const MARGIN_THRESHOLD: Option<f32> = None;

pub const PREPROCESSING_SOS: [[f32; 6]; PREPROCESSING_SECTIONS] = [];

pub const SPATIAL_FILTERS: [[f32; CHANNELS]; CLASSES] =
    [[0.0; CHANNELS]; CLASSES];
pub const TEMPLATES: [[f32; WINDOW]; CLASSES] = [[0.0; WINDOW]; CLASSES];

static ET_RCA_BANK: StaticCell<EtRcaBank<CLASSES, CHANNELS, WINDOW>> =
    StaticCell::new();

pub fn bank() -> Option<&'static EtRcaBank<CLASSES, CHANNELS, WINDOW>> {
    if !MODEL_ENABLED {
        return None;
    }

    Some(ET_RCA_BANK.init(EtRcaBank::new(SPATIAL_FILTERS, TEMPLATES)))
}

pub fn preprocessor() -> ChannelPreprocessor<CHANNELS, PREPROCESSING_SECTIONS>
{
    ChannelPreprocessor::shared(SosCascade::from_scipy_rows(PREPROCESSING_SOS))
}

pub fn config() -> CvepConfig {
    CvepConfig {
        model_enabled: MODEL_ENABLED,
        channels: CHANNELS as u8,
        classes: CLASSES as u8,
        window_samples: WINDOW as u16,
        inference_stride_samples: INFERENCE_STRIDE as u16,
        score_threshold: SCORE_THRESHOLD,
        margin_threshold: MARGIN_THRESHOLD,
    }
}

pub fn to_decision(
    decision: cvep_decoder::Decision,
    ts: u64,
) -> CvepDecisionEvent {
    CvepDecisionEvent {
        ts,
        class_index: decision.class_index,
        raw_score: decision.raw_score,
        normalized_score: decision.normalized_score,
        margin: decision.margin,
    }
}
