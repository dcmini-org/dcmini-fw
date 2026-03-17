#[derive(Clone)]
pub(crate) struct RunningCcaState<const CHANNELS: usize, const FEATURES: usize>
{
    pub(crate) samples_seen: usize,
    pub(crate) avg_x: [f32; CHANNELS],
    pub(crate) avg_y: [f32; FEATURES],
    pub(crate) cov_x: [[f32; CHANNELS]; CHANNELS],
    pub(crate) cov_y: [[f32; FEATURES]; FEATURES],
    pub(crate) cov_xy: [[f32; FEATURES]; CHANNELS],
}

impl<const CHANNELS: usize, const FEATURES: usize> Default
    for RunningCcaState<CHANNELS, FEATURES>
{
    fn default() -> Self {
        Self {
            samples_seen: 0,
            avg_x: [0.0; CHANNELS],
            avg_y: [0.0; FEATURES],
            cov_x: [[0.0; CHANNELS]; CHANNELS],
            cov_y: [[0.0; FEATURES]; FEATURES],
            cov_xy: [[0.0; FEATURES]; CHANNELS],
        }
    }
}

pub(crate) fn observation_mean_i32<
    const CHANNELS: usize,
    const WINDOW: usize,
>(
    trial: &[[i32; WINDOW]; CHANNELS],
) -> [f32; CHANNELS] {
    let mut avg = [0.0; CHANNELS];
    let mut channel_idx = 0;
    while channel_idx < CHANNELS {
        let mut sum = 0.0f32;
        let mut sample_idx = 0;
        while sample_idx < WINDOW {
            sum += trial[channel_idx][sample_idx] as f32;
            sample_idx += 1;
        }
        avg[channel_idx] = sum / WINDOW as f32;
        channel_idx += 1;
    }
    avg
}

pub(crate) fn observation_mean_f32<
    const FEATURES: usize,
    const WINDOW: usize,
>(
    trial: &[[f32; WINDOW]; FEATURES],
) -> [f32; FEATURES] {
    let mut avg = [0.0; FEATURES];
    let mut feature_idx = 0;
    while feature_idx < FEATURES {
        let mut sum = 0.0f32;
        let mut sample_idx = 0;
        while sample_idx < WINDOW {
            sum += trial[feature_idx][sample_idx];
            sample_idx += 1;
        }
        avg[feature_idx] = sum / WINDOW as f32;
        feature_idx += 1;
    }
    avg
}

pub(crate) fn update_running_cov_x_f32<
    const CHANNELS: usize,
    const FEATURES: usize,
    const WINDOW: usize,
>(
    state: &RunningCcaState<CHANNELS, FEATURES>,
    trial: &[[f32; WINDOW]; CHANNELS],
    obs_avg: &[f32; CHANNELS],
    out_avg: &mut [f32; CHANNELS],
    out_cov: &mut [[f32; CHANNELS]; CHANNELS],
) -> usize {
    let n_obs = WINDOW;
    if state.samples_seen == 0 {
        *out_avg = *obs_avg;
        let scale = 1.0 / (n_obs.saturating_sub(1).max(1) as f32);
        let mut i = 0;
        while i < CHANNELS {
            let mut j = 0;
            while j < CHANNELS {
                let mut sum = 0.0f32;
                let mut t = 0;
                while t < WINDOW {
                    sum += (trial[i][t] - out_avg[i])
                        * (trial[j][t] - out_avg[j]);
                    t += 1;
                }
                out_cov[i][j] = sum * scale;
                j += 1;
            }
            i += 1;
        }
        return n_obs;
    }

    let n_new = state.samples_seen + n_obs;
    let old_factor = (state.samples_seen.saturating_sub(1) as f32)
        / (n_new.saturating_sub(1).max(1) as f32);

    let mut i = 0;
    while i < CHANNELS {
        out_avg[i] = state.avg_x[i]
            + (obs_avg[i] - state.avg_x[i]) * (n_obs as f32 / n_new as f32);
        i += 1;
    }

    let scale = 1.0 / (n_new.saturating_sub(1).max(1) as f32);
    let mut i = 0;
    while i < CHANNELS {
        let mut j = 0;
        while j < CHANNELS {
            let mut sum = 0.0f32;
            let mut t = 0;
            while t < WINDOW {
                let left = trial[i][t] - state.avg_x[i];
                let right = trial[j][t] - out_avg[j];
                sum += left * right;
                t += 1;
            }
            out_cov[i][j] = sum * scale + state.cov_x[i][j] * old_factor;
            j += 1;
        }
        i += 1;
    }

    n_new
}

pub(crate) fn update_running_cov_x_i32<
    const CHANNELS: usize,
    const FEATURES: usize,
    const WINDOW: usize,
>(
    state: &RunningCcaState<CHANNELS, FEATURES>,
    trial: &[[i32; WINDOW]; CHANNELS],
    obs_avg: &[f32; CHANNELS],
    out_avg: &mut [f32; CHANNELS],
    out_cov: &mut [[f32; CHANNELS]; CHANNELS],
) -> usize {
    let n_obs = WINDOW;
    if state.samples_seen == 0 {
        *out_avg = *obs_avg;
        let scale = 1.0 / (n_obs.saturating_sub(1).max(1) as f32);
        let mut i = 0;
        while i < CHANNELS {
            let mut j = 0;
            while j < CHANNELS {
                let mut sum = 0.0f32;
                let mut t = 0;
                while t < WINDOW {
                    sum += (trial[i][t] as f32 - out_avg[i])
                        * (trial[j][t] as f32 - out_avg[j]);
                    t += 1;
                }
                out_cov[i][j] = sum * scale;
                j += 1;
            }
            i += 1;
        }
        return n_obs;
    }

    let n_new = state.samples_seen + n_obs;
    let old_factor = (state.samples_seen.saturating_sub(1) as f32)
        / (n_new.saturating_sub(1).max(1) as f32);

    let mut i = 0;
    while i < CHANNELS {
        out_avg[i] = state.avg_x[i]
            + (obs_avg[i] - state.avg_x[i]) * (n_obs as f32 / n_new as f32);
        i += 1;
    }

    let scale = 1.0 / (n_new.saturating_sub(1).max(1) as f32);
    let mut i = 0;
    while i < CHANNELS {
        let mut j = 0;
        while j < CHANNELS {
            let mut sum = 0.0f32;
            let mut t = 0;
            while t < WINDOW {
                let left = trial[i][t] as f32 - state.avg_x[i];
                let right = trial[j][t] as f32 - out_avg[j];
                sum += left * right;
                t += 1;
            }
            out_cov[i][j] = sum * scale + state.cov_x[i][j] * old_factor;
            j += 1;
        }
        i += 1;
    }

    n_new
}

pub(crate) fn update_running_cov_y_and_xy_i32<
    const CHANNELS: usize,
    const FEATURES: usize,
    const WINDOW: usize,
>(
    state: &RunningCcaState<CHANNELS, FEATURES>,
    trial_x: &[[i32; WINDOW]; CHANNELS],
    trial_y: &[[f32; WINDOW]; FEATURES],
    out_avg_y: &mut [f32; FEATURES],
    out_cov_y: &mut [[f32; FEATURES]; FEATURES],
    out_cov_xy: &mut [[f32; FEATURES]; CHANNELS],
    n_new: usize,
) {
    let y_obs = observation_mean_f32(trial_y);
    let n_obs = WINDOW;

    if state.samples_seen == 0 {
        *out_avg_y = y_obs;
        let scale = 1.0 / (n_obs.saturating_sub(1).max(1) as f32);

        let mut i = 0;
        while i < FEATURES {
            let mut j = 0;
            while j < FEATURES {
                let mut sum = 0.0f32;
                let mut t = 0;
                while t < WINDOW {
                    sum += (trial_y[i][t] - out_avg_y[i])
                        * (trial_y[j][t] - out_avg_y[j]);
                    t += 1;
                }
                out_cov_y[i][j] = sum * scale;
                j += 1;
            }
            i += 1;
        }

        let x_obs = observation_mean_i32(trial_x);
        let mut j = 0;
        while j < CHANNELS {
            let mut i = 0;
            while i < FEATURES {
                let mut sum = 0.0f32;
                let mut t = 0;
                while t < WINDOW {
                    sum += (trial_x[j][t] as f32 - x_obs[j])
                        * (trial_y[i][t] - out_avg_y[i]);
                    t += 1;
                }
                out_cov_xy[j][i] = sum * scale;
                i += 1;
            }
            j += 1;
        }
        return;
    }

    let old_factor = (state.samples_seen.saturating_sub(1) as f32)
        / (n_new.saturating_sub(1).max(1) as f32);
    let scale = 1.0 / (n_new.saturating_sub(1).max(1) as f32);
    let mut i = 0;
    while i < FEATURES {
        out_avg_y[i] = state.avg_y[i]
            + (y_obs[i] - state.avg_y[i]) * (n_obs as f32 / n_new as f32);
        i += 1;
    }

    let mut i = 0;
    while i < FEATURES {
        let mut j = 0;
        while j < FEATURES {
            let mut sum = 0.0f32;
            let mut t = 0;
            while t < WINDOW {
                let left = trial_y[i][t] - state.avg_y[i];
                let right = trial_y[j][t] - out_avg_y[j];
                sum += left * right;
                t += 1;
            }
            out_cov_y[i][j] = sum * scale + state.cov_y[i][j] * old_factor;
            j += 1;
        }
        i += 1;
    }

    let mut j = 0;
    while j < CHANNELS {
        let mut i = 0;
        while i < FEATURES {
            let mut sum = 0.0f32;
            let mut t = 0;
            while t < WINDOW {
                let left = trial_x[j][t] as f32 - state.avg_x[j];
                let right = trial_y[i][t] - out_avg_y[i];
                sum += left * right;
                t += 1;
            }
            out_cov_xy[j][i] = sum * scale + state.cov_xy[j][i] * old_factor;
            i += 1;
        }
        j += 1;
    }
}

pub(crate) fn update_running_cov_y_and_xy_f32<
    const CHANNELS: usize,
    const FEATURES: usize,
    const WINDOW: usize,
>(
    state: &RunningCcaState<CHANNELS, FEATURES>,
    trial_x: &[[f32; WINDOW]; CHANNELS],
    trial_y: &[[f32; WINDOW]; FEATURES],
    out_avg_y: &mut [f32; FEATURES],
    out_cov_y: &mut [[f32; FEATURES]; FEATURES],
    out_cov_xy: &mut [[f32; FEATURES]; CHANNELS],
    n_new: usize,
) {
    let y_obs = observation_mean_f32(trial_y);
    let n_obs = WINDOW;

    if state.samples_seen == 0 {
        *out_avg_y = y_obs;
        let scale = 1.0 / (n_obs.saturating_sub(1).max(1) as f32);

        let mut i = 0;
        while i < FEATURES {
            let mut j = 0;
            while j < FEATURES {
                let mut sum = 0.0f32;
                let mut t = 0;
                while t < WINDOW {
                    sum += (trial_y[i][t] - out_avg_y[i])
                        * (trial_y[j][t] - out_avg_y[j]);
                    t += 1;
                }
                out_cov_y[i][j] = sum * scale;
                j += 1;
            }
            i += 1;
        }

        let x_obs = observation_mean_f32(trial_x);
        let mut j = 0;
        while j < CHANNELS {
            let mut i = 0;
            while i < FEATURES {
                let mut sum = 0.0f32;
                let mut t = 0;
                while t < WINDOW {
                    sum += (trial_x[j][t] - x_obs[j])
                        * (trial_y[i][t] - out_avg_y[i]);
                    t += 1;
                }
                out_cov_xy[j][i] = sum * scale;
                i += 1;
            }
            j += 1;
        }
        return;
    }

    let old_factor = (state.samples_seen.saturating_sub(1) as f32)
        / (n_new.saturating_sub(1).max(1) as f32);
    let scale = 1.0 / (n_new.saturating_sub(1).max(1) as f32);
    let mut i = 0;
    while i < FEATURES {
        out_avg_y[i] = state.avg_y[i]
            + (y_obs[i] - state.avg_y[i]) * (n_obs as f32 / n_new as f32);
        i += 1;
    }

    let mut i = 0;
    while i < FEATURES {
        let mut j = 0;
        while j < FEATURES {
            let mut sum = 0.0f32;
            let mut t = 0;
            while t < WINDOW {
                let left = trial_y[i][t] - state.avg_y[i];
                let right = trial_y[j][t] - out_avg_y[j];
                sum += left * right;
                t += 1;
            }
            out_cov_y[i][j] = sum * scale + state.cov_y[i][j] * old_factor;
            j += 1;
        }
        i += 1;
    }

    let mut j = 0;
    while j < CHANNELS {
        let mut i = 0;
        while i < FEATURES {
            let mut sum = 0.0f32;
            let mut t = 0;
            while t < WINDOW {
                let left = trial_x[j][t] - state.avg_x[j];
                let right = trial_y[i][t] - out_avg_y[i];
                sum += left * right;
                t += 1;
            }
            out_cov_xy[j][i] = sum * scale + state.cov_xy[j][i] * old_factor;
            i += 1;
        }
        j += 1;
    }
}
