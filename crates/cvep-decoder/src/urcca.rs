use crate::banks::UrCcaBank;
use crate::internal::score::{best_two, top_canonical_correlation};
use crate::internal::stats::{
    observation_mean_f32, observation_mean_i32, update_running_cov_x_f32,
    update_running_cov_x_i32, update_running_cov_y_and_xy_f32,
    update_running_cov_y_and_xy_i32, RunningCcaState,
};
use crate::types::Decision;

#[derive(Clone, Debug, PartialEq)]
pub struct UrCcaStateSnapshot<const CHANNELS: usize, const FEATURES: usize> {
    pub samples_seen: usize,
    pub avg_x: [f32; CHANNELS],
    pub avg_y: [f32; FEATURES],
    pub cov_x: [[f32; CHANNELS]; CHANNELS],
    pub cov_y: [[f32; FEATURES]; FEATURES],
    pub cov_xy: [[f32; FEATURES]; CHANNELS],
}

/// Online adaptive urCCA-style decoder with a single shared running-CCA state.
///
/// This mirrors PyntBCI's update rule: score each class by updating a running
/// CCA with the current EEG trial and the class-specific encoding matrix, then
/// copy the winning class state forward to all classes.
pub struct UrCcaDecoder<
    'a,
    const CLASSES: usize,
    const CHANNELS: usize,
    const FEATURES: usize,
    const WINDOW: usize,
> {
    bank: UrCcaBank<'a, CLASSES, FEATURES, WINDOW>,
    state: RunningCcaState<CHANNELS, FEATURES>,
    regularization: f32,
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
    > UrCcaDecoder<'a, CLASSES, CHANNELS, FEATURES, WINDOW>
{
    pub fn new(
        bank: UrCcaBank<'a, CLASSES, FEATURES, WINDOW>,
        regularization: f32,
    ) -> Self {
        Self {
            bank,
            state: RunningCcaState::default(),
            regularization,
            scratch_cov_y: [[0.0; FEATURES]; FEATURES],
            scratch_cov_xy: [[0.0; FEATURES]; CHANNELS],
            scratch_avg_y: [0.0; FEATURES],
        }
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
                &self.bank.encodings[class_idx],
                &mut scratch_avg_y,
                &mut scratch_cov_y,
                &mut scratch_cov_xy,
                n_new,
            );

            scores[class_idx] = top_canonical_correlation(
                &next_avg_x,
                &scratch_avg_y,
                trial,
                &self.bank.encodings[class_idx],
                &next_cov_x,
                &scratch_cov_y,
                &scratch_cov_xy,
                self.regularization,
            );
            class_idx += 1;
        }

        scores
    }

    pub fn state_snapshot(&self) -> UrCcaStateSnapshot<CHANNELS, FEATURES> {
        UrCcaStateSnapshot {
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
                &self.bank.encodings[class_idx],
                &mut self.scratch_avg_y,
                &mut self.scratch_cov_y,
                &mut self.scratch_cov_xy,
                n_new,
            );

            let score = top_canonical_correlation(
                &next_avg_x,
                &self.scratch_avg_y,
                &trial_f32,
                &self.bank.encodings[class_idx],
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

        update_running_cov_y_and_xy_i32(
            &self.state,
            trial,
            &self.bank.encodings[best_class],
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

        Decision {
            class_index: best_class,
            raw_score: (best_score * 1_000_000.0) as i64,
            normalized_score: best_score,
            margin: best_score - runner_up,
        }
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

        update_running_cov_y_and_xy_f32(
            &self.state,
            trial,
            &self.bank.encodings[best_class],
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

        Decision {
            class_index: best_class,
            raw_score: (best_score * 1_000_000.0) as i64,
            normalized_score: best_score,
            margin: best_score - runner_up,
        }
    }
}
