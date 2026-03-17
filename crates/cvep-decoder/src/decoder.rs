use libm::sqrtf;

use crate::banks::{EtRcaBank, ProjectedCorrelationBank, RccaBank};
use crate::types::{Decision, DecodeError};

/// Sliding-window CVEP decoder using integer samples from the ADS frontend.
pub struct CvepDecoder<const CHANNELS: usize, const WINDOW: usize> {
    ring: [[i32; WINDOW]; CHANNELS],
    sums: [i64; CHANNELS],
    write_index: usize,
    len: usize,
}

impl<const CHANNELS: usize, const WINDOW: usize>
    CvepDecoder<CHANNELS, WINDOW>
{
    pub const fn new() -> Self {
        Self {
            ring: [[0; WINDOW]; CHANNELS],
            sums: [0; CHANNELS],
            write_index: 0,
            len: 0,
        }
    }

    pub fn clear(&mut self) {
        self.ring = [[0; WINDOW]; CHANNELS];
        self.sums = [0; CHANNELS];
        self.write_index = 0;
        self.len = 0;
    }

    pub const fn len(&self) -> usize {
        self.len
    }

    pub const fn is_ready(&self) -> bool {
        self.len == WINDOW
    }

    pub fn push(&mut self, sample: [i32; CHANNELS]) {
        debug_assert!(CHANNELS > 0);
        debug_assert!(WINDOW > 0);

        let slot = self.write_index;
        let evict = self.len == WINDOW;

        let mut channel_idx = 0;
        while channel_idx < CHANNELS {
            if evict {
                self.sums[channel_idx] -= self.ring[channel_idx][slot] as i64;
            }

            self.ring[channel_idx][slot] = sample[channel_idx];
            self.sums[channel_idx] += sample[channel_idx] as i64;
            channel_idx += 1;
        }

        self.write_index += 1;
        if self.write_index == WINDOW {
            self.write_index = 0;
        }

        if !evict {
            self.len += 1;
        }
    }

    pub fn predict_etrca<const CLASSES: usize>(
        &self,
        bank: &EtRcaBank<CLASSES, CHANNELS, WINDOW>,
    ) -> Result<Decision, DecodeError> {
        self.predict_projected_correlation(bank)
    }

    pub fn predict_rcca<const CLASSES: usize>(
        &self,
        bank: &RccaBank<CLASSES, CHANNELS, WINDOW>,
    ) -> Result<Decision, DecodeError> {
        self.predict_projected_correlation(bank)
    }

    fn predict_projected_correlation<const CLASSES: usize>(
        &self,
        bank: &ProjectedCorrelationBank<CLASSES, CHANNELS, WINDOW>,
    ) -> Result<Decision, DecodeError> {
        if !self.is_ready() {
            return Err(DecodeError::NotReady);
        }

        let mut best_class = 0usize;
        let mut best_raw = i64::MIN;
        let mut best_score = f32::NEG_INFINITY;
        let mut runner_up = f32::NEG_INFINITY;

        let mut class_idx = 0;
        while class_idx < CLASSES {
            let filter = &bank.spatial_filters[class_idx];
            let template = &bank.templates[class_idx];

            let mut ring_idx = self.write_index;
            let mut projected_sum = 0.0f32;
            let mut sample_idx = 0;
            while sample_idx < WINDOW {
                projected_sum += self.projected_sample(ring_idx, filter);
                ring_idx += 1;
                if ring_idx == WINDOW {
                    ring_idx = 0;
                }
                sample_idx += 1;
            }

            let projected_mean = projected_sum / WINDOW as f32;
            let mut energy = 0.0f32;
            let mut numerator = 0.0f32;
            let mut ring_idx = self.write_index;
            let mut sample_idx = 0;

            while sample_idx < WINDOW {
                let centered =
                    self.projected_sample(ring_idx, filter) - projected_mean;
                numerator += centered * template[sample_idx];
                energy += centered * centered;

                ring_idx += 1;
                if ring_idx == WINDOW {
                    ring_idx = 0;
                }
                sample_idx += 1;
            }

            let raw = numerator as i64;

            let score = if energy > 0.0 {
                numerator / (sqrtf(energy) * bank.template_norms[class_idx])
            } else {
                f32::NEG_INFINITY
            };

            if score > best_score {
                runner_up = best_score;
                best_score = score;
                best_raw = raw;
                best_class = class_idx;
            } else if score > runner_up {
                runner_up = score;
            }

            class_idx += 1;
        }

        Ok(Decision {
            class_index: best_class,
            raw_score: best_raw,
            normalized_score: best_score,
            margin: best_score - runner_up,
        })
    }

    fn projected_sample(
        &self,
        ring_idx: usize,
        filter: &[f32; CHANNELS],
    ) -> f32 {
        let mut value = 0.0f32;
        let mut channel_idx = 0;
        while channel_idx < CHANNELS {
            value +=
                self.ring[channel_idx][ring_idx] as f32 * filter[channel_idx];
            channel_idx += 1;
        }
        value
    }
}

impl<const CHANNELS: usize, const WINDOW: usize> Default
    for CvepDecoder<CHANNELS, WINDOW>
{
    fn default() -> Self {
        Self::new()
    }
}
