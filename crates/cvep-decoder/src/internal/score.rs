use libm::sqrtf;

use crate::internal::linalg::{
    cholesky_lower, dominant_singular_vectors, solve_lower_left,
    solve_lower_right_transpose, solve_lower_transpose_vec,
};

pub(crate) fn best_two<const N: usize>(
    scores: &[f32; N],
) -> (usize, f32, f32) {
    let mut best_index = 0usize;
    let mut best = f32::NEG_INFINITY;
    let mut runner_up = f32::NEG_INFINITY;
    let mut idx = 0;
    while idx < N {
        let score = scores[idx];
        if score > best {
            runner_up = best;
            best = score;
            best_index = idx;
        } else if score > runner_up {
            runner_up = score;
        }
        idx += 1;
    }
    (best_index, best, runner_up)
}

pub(crate) fn top_canonical_correlation<
    const CHANNELS: usize,
    const FEATURES: usize,
    const WINDOW: usize,
>(
    avg_x: &[f32; CHANNELS],
    avg_y: &[f32; FEATURES],
    trial_x: &[[f32; WINDOW]; CHANNELS],
    trial_y: &[[f32; WINDOW]; FEATURES],
    cov_x: &[[f32; CHANNELS]; CHANNELS],
    cov_y: &[[f32; FEATURES]; FEATURES],
    cov_xy: &[[f32; FEATURES]; CHANNELS],
    regularization: f32,
) -> f32 {
    let mut reg_x = *cov_x;
    let mut reg_y = *cov_y;
    let mut idx = 0;
    while idx < CHANNELS {
        reg_x[idx][idx] += regularization;
        idx += 1;
    }
    let mut idx = 0;
    while idx < FEATURES {
        reg_y[idx][idx] += regularization;
        idx += 1;
    }

    let Some(chol_x) = cholesky_lower(&reg_x) else {
        return 0.0;
    };
    let Some(chol_y) = cholesky_lower(&reg_y) else {
        return 0.0;
    };

    let left_whitened =
        solve_lower_left::<CHANNELS, FEATURES>(&chol_x, cov_xy);
    let whitened = solve_lower_right_transpose::<CHANNELS, FEATURES>(
        &left_whitened,
        &chol_y,
    );
    let (u, v, sigma) = dominant_singular_vectors(&whitened);
    if sigma <= 0.0 {
        return 0.0;
    }

    let wx = solve_lower_transpose_vec(&chol_x, &u);
    let wy = solve_lower_transpose_vec(&chol_y, &v);
    projected_trial_correlation(avg_x, avg_y, trial_x, trial_y, &wx, &wy)
        .min(1.0)
}

fn projected_trial_correlation<
    const CHANNELS: usize,
    const FEATURES: usize,
    const WINDOW: usize,
>(
    avg_x: &[f32; CHANNELS],
    avg_y: &[f32; FEATURES],
    trial_x: &[[f32; WINDOW]; CHANNELS],
    trial_y: &[[f32; WINDOW]; FEATURES],
    wx: &[f32; CHANNELS],
    wy: &[f32; FEATURES],
) -> f32 {
    let mut mean_x = 0.0f32;
    let mut mean_y = 0.0f32;
    let mut sample_idx = 0;
    while sample_idx < WINDOW {
        let mut x_value = 0.0f32;
        let mut channel_idx = 0;
        while channel_idx < CHANNELS {
            x_value += (trial_x[channel_idx][sample_idx] - avg_x[channel_idx])
                * wx[channel_idx];
            channel_idx += 1;
        }

        let mut y_value = 0.0f32;
        let mut feature_idx = 0;
        while feature_idx < FEATURES {
            y_value += (trial_y[feature_idx][sample_idx] - avg_y[feature_idx])
                * wy[feature_idx];
            feature_idx += 1;
        }

        mean_x += x_value;
        mean_y += y_value;
        sample_idx += 1;
    }

    mean_x /= WINDOW as f32;
    mean_y /= WINDOW as f32;

    let mut numerator = 0.0f32;
    let mut energy_x = 0.0f32;
    let mut energy_y = 0.0f32;
    let mut sample_idx = 0;
    while sample_idx < WINDOW {
        let mut x_value = 0.0f32;
        let mut channel_idx = 0;
        while channel_idx < CHANNELS {
            x_value += (trial_x[channel_idx][sample_idx] - avg_x[channel_idx])
                * wx[channel_idx];
            channel_idx += 1;
        }

        let mut y_value = 0.0f32;
        let mut feature_idx = 0;
        while feature_idx < FEATURES {
            y_value += (trial_y[feature_idx][sample_idx] - avg_y[feature_idx])
                * wy[feature_idx];
            feature_idx += 1;
        }

        let centered_x = x_value - mean_x;
        let centered_y = y_value - mean_y;
        numerator += centered_x * centered_y;
        energy_x += centered_x * centered_x;
        energy_y += centered_y * centered_y;
        sample_idx += 1;
    }

    if energy_x <= 0.0 || energy_y <= 0.0 {
        0.0
    } else {
        numerator / sqrtf(energy_x * energy_y)
    }
}
