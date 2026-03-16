use crate::banks::UrCcaBank;
use crate::cca_update_policy::CumulativeCcaUpdatePolicy;
use crate::internal::score::{best_two, top_canonical_correlation};
use crate::internal::stats::{
    observation_mean_f32, observation_mean_i32, update_running_cov_x_f32,
    update_running_cov_x_i32, update_running_cov_y_and_xy_f32,
    update_running_cov_y_and_xy_i32, RunningCcaState,
};
use crate::types::Decision;

#[derive(Clone, Debug, PartialEq)]
pub struct CumulativeCcaStateSnapshot<
    const CHANNELS: usize,
    const FEATURES: usize,
> {
    pub samples_seen: usize,
    pub avg_x: [f32; CHANNELS],
    pub avg_y: [f32; FEATURES],
    pub cov_x: [[f32; CHANNELS]; CHANNELS],
    pub cov_y: [[f32; FEATURES]; FEATURES],
    pub cov_xy: [[f32; FEATURES]; CHANNELS],
}

/// Zero-training CCA with cumulative learning from previous predicted trials.
///
/// This mirrors the current urCCA-style update rule already used in the crate.
pub struct CumulativeCcaDecoder<
    'a,
    const CLASSES: usize,
    const CHANNELS: usize,
    const FEATURES: usize,
    const WINDOW: usize,
> {
    bank: UrCcaBank<'a, CLASSES, FEATURES, WINDOW>,
    state: RunningCcaState<CHANNELS, FEATURES>,
    regularization: f32,
    update_policy: CumulativeCcaUpdatePolicy,
    scratch_cov_y: [[f32; FEATURES]; FEATURES],
    scratch_cov_xy: [[f32; FEATURES]; CHANNELS],
    scratch_avg_y: [f32; FEATURES],
}

impl<
        'a,
        const CLASSES: usize,
        const CHANNELS: usize,
        const FEATURES: usize,
        const WINDOW: usize,
    > CumulativeCcaDecoder<'a, CLASSES, CHANNELS, FEATURES, WINDOW>
{
    pub fn new(
        bank: UrCcaBank<'a, CLASSES, FEATURES, WINDOW>,
        regularization: f32,
    ) -> Self {
        Self {
            bank,
            state: RunningCcaState::default(),
            regularization,
            update_policy: CumulativeCcaUpdatePolicy::AlwaysUpdate,
            scratch_cov_y: [[0.0; FEATURES]; FEATURES],
            scratch_cov_xy: [[0.0; FEATURES]; CHANNELS],
            scratch_avg_y: [0.0; FEATURES],
        }
    }

    pub fn with_update_policy(
        mut self,
        update_policy: CumulativeCcaUpdatePolicy,
    ) -> Self {
        self.update_policy = update_policy;
        self
    }

    pub fn update_policy(&self) -> CumulativeCcaUpdatePolicy {
        self.update_policy
    }

    pub fn reset(&mut self) {
        self.state = RunningCcaState::default();
    }

    pub fn bank(&self) -> &UrCcaBank<'a, CLASSES, FEATURES, WINDOW> {
        &self.bank
    }

    pub fn class_scores_f32(
        &self,
        trial: &[[f32; WINDOW]; CHANNELS],
    ) -> [f32; CLASSES] {
        let x_obs = observation_mean_f32(trial);
        let mut next_avg_x = [0.0; CHANNELS];
        let mut next_cov_x = [[0.0; CHANNELS]; CHANNELS];
        let n_new = update_running_cov_x_f32(
            &self.state,
            trial,
            &x_obs,
            &mut next_avg_x,
            &mut next_cov_x,
        );

        let mut scratch_avg_y = [0.0; FEATURES];
        let mut scratch_cov_y = [[0.0; FEATURES]; FEATURES];
        let mut scratch_cov_xy = [[0.0; FEATURES]; CHANNELS];
        let mut scores = [0.0; CLASSES];

        let mut class_idx = 0;
        while class_idx < CLASSES {
            update_running_cov_y_and_xy_f32(
                &self.state,
                trial,
                &self.bank.encodings()[class_idx],
                &mut scratch_avg_y,
                &mut scratch_cov_y,
                &mut scratch_cov_xy,
                n_new,
            );

            scores[class_idx] = top_canonical_correlation(
                &next_avg_x,
                &scratch_avg_y,
                trial,
                &self.bank.encodings()[class_idx],
                &next_cov_x,
                &scratch_cov_y,
                &scratch_cov_xy,
                self.regularization,
            );
            class_idx += 1;
        }

        scores
    }

    pub fn state_snapshot(
        &self,
    ) -> CumulativeCcaStateSnapshot<CHANNELS, FEATURES> {
        CumulativeCcaStateSnapshot {
            samples_seen: self.state.samples_seen,
            avg_x: self.state.avg_x,
            avg_y: self.state.avg_y,
            cov_x: self.state.cov_x,
            cov_y: self.state.cov_y,
            cov_xy: self.state.cov_xy,
        }
    }

    pub fn observe_i32(
        &mut self,
        trial: &[[i32; WINDOW]; CHANNELS],
    ) -> Decision {
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

        let x_obs = observation_mean_i32(trial);
        let mut next_avg_x = [0.0; CHANNELS];
        let mut next_cov_x = [[0.0; CHANNELS]; CHANNELS];
        let n_new = update_running_cov_x_i32(
            &self.state,
            trial,
            &x_obs,
            &mut next_avg_x,
            &mut next_cov_x,
        );

        let mut best_class = 0usize;
        let mut best_score = f32::NEG_INFINITY;
        let mut runner_up = f32::NEG_INFINITY;

        let mut class_idx = 0;
        while class_idx < CLASSES {
            update_running_cov_y_and_xy_i32(
                &self.state,
                trial,
                &self.bank.encodings()[class_idx],
                &mut self.scratch_avg_y,
                &mut self.scratch_cov_y,
                &mut self.scratch_cov_xy,
                n_new,
            );

            let score = top_canonical_correlation(
                &next_avg_x,
                &self.scratch_avg_y,
                &trial_f32,
                &self.bank.encodings()[class_idx],
                &next_cov_x,
                &self.scratch_cov_y,
                &self.scratch_cov_xy,
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

        let decision = Decision {
            class_index: best_class,
            raw_score: (best_score * 1_000_000.0) as i64,
            normalized_score: best_score,
            margin: best_score - runner_up,
        };

        if self.update_policy.should_update(&decision) {
            update_running_cov_y_and_xy_i32(
                &self.state,
                trial,
                &self.bank.encodings()[best_class],
                &mut self.scratch_avg_y,
                &mut self.scratch_cov_y,
                &mut self.scratch_cov_xy,
                n_new,
            );

            self.state.samples_seen = n_new;
            self.state.avg_x = next_avg_x;
            self.state.cov_x = next_cov_x;
            self.state.avg_y = self.scratch_avg_y;
            self.state.cov_y = self.scratch_cov_y;
            self.state.cov_xy = self.scratch_cov_xy;
        }

        decision
    }

    pub fn observe_f32(
        &mut self,
        trial: &[[f32; WINDOW]; CHANNELS],
    ) -> Decision {
        let x_obs = observation_mean_f32(trial);
        let mut next_avg_x = [0.0; CHANNELS];
        let mut next_cov_x = [[0.0; CHANNELS]; CHANNELS];
        let n_new = update_running_cov_x_f32(
            &self.state,
            trial,
            &x_obs,
            &mut next_avg_x,
            &mut next_cov_x,
        );
        let scores = self.class_scores_f32(trial);
        let (best_class, best_score, runner_up) = best_two(&scores);

        let decision = Decision {
            class_index: best_class,
            raw_score: (best_score * 1_000_000.0) as i64,
            normalized_score: best_score,
            margin: best_score - runner_up,
        };

        if self.update_policy.should_update(&decision) {
            update_running_cov_y_and_xy_f32(
                &self.state,
                trial,
                &self.bank.encodings()[best_class],
                &mut self.scratch_avg_y,
                &mut self.scratch_cov_y,
                &mut self.scratch_cov_xy,
                n_new,
            );

            self.state.samples_seen = n_new;
            self.state.avg_x = next_avg_x;
            self.state.cov_x = next_cov_x;
            self.state.avg_y = self.scratch_avg_y;
            self.state.cov_y = self.scratch_cov_y;
            self.state.cov_xy = self.scratch_cov_xy;
        }

        decision
    }
}

#[cfg(test)]
mod tests {
    use super::CumulativeCcaDecoder;
    use crate::{CumulativeCcaUpdatePolicy, UrCcaBank};

    #[test]
    fn cumulative_cca_updates_across_trials() {
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
        let bank = UrCcaBank::<CLASSES, FEATURES, WINDOW>::new(&encodings);
        let mut decoder = CumulativeCcaDecoder::new(bank, 1.0e-3);

        let trial_a = [
            [1_000, 0, 1_000, 0, 1_000, 0, 1_000, 0],
            [0, 800, 0, 800, 0, 800, 0, 800],
        ];
        let trial_b = [
            [0, 1_000, 0, 1_000, 0, 1_000, 0, 1_000],
            [800, 0, 800, 0, 800, 0, 800, 0],
        ];

        let first = decoder.observe_i32(&trial_a);
        assert_eq!(first.class_index, 0);
        let second = decoder.observe_i32(&trial_b);
        assert!(second.normalized_score.is_finite());
        assert_eq!(decoder.state_snapshot().samples_seen, WINDOW * 2);
    }

    #[test]
    fn cumulative_cca_can_skip_low_margin_updates() {
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
        let bank = UrCcaBank::<CLASSES, FEATURES, WINDOW>::new(&encodings);
        let mut decoder = CumulativeCcaDecoder::new(bank, 1.0e-3)
            .with_update_policy(CumulativeCcaUpdatePolicy::MarginThreshold(
                10.0,
            ));

        let ambiguous = [
            [500, 500, 500, 500, 500, 500, 500, 500],
            [500, 500, 500, 500, 500, 500, 500, 500],
        ];

        let _decision = decoder.observe_i32(&ambiguous);
        assert_eq!(decoder.state_snapshot().samples_seen, 0);
    }
}
