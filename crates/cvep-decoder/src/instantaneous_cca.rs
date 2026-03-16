use crate::banks::UrCcaBank;
use crate::internal::score::{best_two, top_canonical_correlation};
use crate::internal::stats::{
    observation_mean_f32, observation_mean_i32, update_running_cov_x_f32,
    update_running_cov_x_i32, update_running_cov_y_and_xy_f32,
    update_running_cov_y_and_xy_i32, RunningCcaState,
};
use crate::types::Decision;

/// Calibration-free reconvolution / encoding-model CCA without cumulative
/// state. Each trial is decoded independently from the known class encodings.
pub struct InstantaneousCcaDecoder<
    'a,
    const CLASSES: usize,
    const CHANNELS: usize,
    const FEATURES: usize,
    const WINDOW: usize,
> {
    bank: UrCcaBank<'a, CLASSES, FEATURES, WINDOW>,
    regularization: f32,
}

impl<
    'a,
    const CLASSES: usize,
    const CHANNELS: usize,
    const FEATURES: usize,
    const WINDOW: usize,
> InstantaneousCcaDecoder<'a, CLASSES, CHANNELS, FEATURES, WINDOW>
{
    pub const fn new(
        bank: UrCcaBank<'a, CLASSES, FEATURES, WINDOW>,
        regularization: f32,
    ) -> Self {
        Self { bank, regularization }
    }

    pub fn bank(&self) -> &UrCcaBank<'a, CLASSES, FEATURES, WINDOW> {
        &self.bank
    }

    pub fn class_scores_f32(
        &self,
        trial: &[[f32; WINDOW]; CHANNELS],
    ) -> [f32; CLASSES] {
        let state = RunningCcaState::<CHANNELS, FEATURES>::default();
        let x_obs = observation_mean_f32(trial);
        let mut avg_x = [0.0; CHANNELS];
        let mut cov_x = [[0.0; CHANNELS]; CHANNELS];
        let n_new = update_running_cov_x_f32(
            &state,
            trial,
            &x_obs,
            &mut avg_x,
            &mut cov_x,
        );

        let mut scratch_avg_y = [0.0; FEATURES];
        let mut scratch_cov_y = [[0.0; FEATURES]; FEATURES];
        let mut scratch_cov_xy = [[0.0; FEATURES]; CHANNELS];
        let mut scores = [0.0; CLASSES];

        let mut class_idx = 0;
        while class_idx < CLASSES {
            update_running_cov_y_and_xy_f32(
                &state,
                trial,
                &self.bank.encodings()[class_idx],
                &mut scratch_avg_y,
                &mut scratch_cov_y,
                &mut scratch_cov_xy,
                n_new,
            );
            scores[class_idx] = top_canonical_correlation(
                &avg_x,
                &scratch_avg_y,
                trial,
                &self.bank.encodings()[class_idx],
                &cov_x,
                &scratch_cov_y,
                &scratch_cov_xy,
                self.regularization,
            );
            class_idx += 1;
        }

        scores
    }

    pub fn observe_f32(
        &self,
        trial: &[[f32; WINDOW]; CHANNELS],
    ) -> Decision {
        let scores = self.class_scores_f32(trial);
        let (best_class, best_score, runner_up) = best_two(&scores);
        Decision {
            class_index: best_class,
            raw_score: (best_score * 1_000_000.0) as i64,
            normalized_score: best_score,
            margin: best_score - runner_up,
        }
    }

    pub fn observe_i32(
        &self,
        trial: &[[i32; WINDOW]; CHANNELS],
    ) -> Decision {
        let state = RunningCcaState::<CHANNELS, FEATURES>::default();
        let x_obs = observation_mean_i32(trial);
        let mut avg_x = [0.0; CHANNELS];
        let mut cov_x = [[0.0; CHANNELS]; CHANNELS];
        let n_new = update_running_cov_x_i32(
            &state,
            trial,
            &x_obs,
            &mut avg_x,
            &mut cov_x,
        );

        let mut trial_f32 = [[0.0; WINDOW]; CHANNELS];
        let mut channel_idx = 0;
        while channel_idx < CHANNELS {
            let mut sample_idx = 0;
            while sample_idx < WINDOW {
                trial_f32[channel_idx][sample_idx] =
                    trial[channel_idx][sample_idx] as f32;
                sample_idx += 1;
            }
            channel_idx += 1;
        }

        let mut scratch_avg_y = [0.0; FEATURES];
        let mut scratch_cov_y = [[0.0; FEATURES]; FEATURES];
        let mut scratch_cov_xy = [[0.0; FEATURES]; CHANNELS];
        let mut best_class = 0usize;
        let mut best_score = f32::NEG_INFINITY;
        let mut runner_up = f32::NEG_INFINITY;

        let mut class_idx = 0;
        while class_idx < CLASSES {
            update_running_cov_y_and_xy_i32(
                &state,
                trial,
                &self.bank.encodings()[class_idx],
                &mut scratch_avg_y,
                &mut scratch_cov_y,
                &mut scratch_cov_xy,
                n_new,
            );

            let score = top_canonical_correlation(
                &avg_x,
                &scratch_avg_y,
                &trial_f32,
                &self.bank.encodings()[class_idx],
                &cov_x,
                &scratch_cov_y,
                &scratch_cov_xy,
                self.regularization,
            );
            if score > best_score {
                runner_up = best_score;
                best_score = score;
                best_class = class_idx;
            } else if score > runner_up {
                runner_up = score;
            }
            class_idx += 1;
        }

        Decision {
            class_index: best_class,
            raw_score: (best_score * 1_000_000.0) as i64,
            normalized_score: best_score,
            margin: best_score - runner_up,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::InstantaneousCcaDecoder;
    use crate::UrCcaBank;

    #[test]
    fn instantaneous_cca_decodes_matching_trial() {
        const CLASSES: usize = 2;
        const FEATURES: usize = 2;
        const WINDOW: usize = 8;

        let encodings = [
            [
                [1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0],
                [0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0],
            ],
            [
                [0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0],
                [1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0],
            ],
        ];
        let trial = [
            [1_000, 0, 1_000, 0, 1_000, 0, 1_000, 0],
            [0, 800, 0, 800, 0, 800, 0, 800],
        ];

        let bank = UrCcaBank::<CLASSES, FEATURES, WINDOW>::new(&encodings);
        let decoder = InstantaneousCcaDecoder::new(bank, 1.0e-3);
        let decision = decoder.observe_i32(&trial);
        assert_eq!(decision.class_index, 0);
        assert!(decision.normalized_score > 0.8);
    }
}
