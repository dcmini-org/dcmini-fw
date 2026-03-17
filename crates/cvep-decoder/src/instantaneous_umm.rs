use crate::banks::UmmCodebook;
use crate::internal::linalg::cholesky_lower;
use crate::types::Decision;

/// Feature layout for flattened spatiotemporal epoch vectors.
///
/// `ChannelPrime` follows the ToeplitzLDA / blockmatrix convention: iterate
/// over all channels for the first time sample, then all channels for the next
/// time sample, and so on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UmmFeatureLayout {
    ChannelPrime,
    TimePrime,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UmmBlockStructure {
    n_channels: usize,
    n_timepoints: usize,
    layout: UmmFeatureLayout,
}

impl UmmBlockStructure {
    pub const fn new(n_channels: usize, n_timepoints: usize) -> Self {
        Self::channel_prime(n_channels, n_timepoints)
    }

    pub const fn channel_prime(
        n_channels: usize,
        n_timepoints: usize,
    ) -> Self {
        Self {
            n_channels,
            n_timepoints,
            layout: UmmFeatureLayout::ChannelPrime,
        }
    }

    pub const fn time_prime(n_channels: usize, n_timepoints: usize) -> Self {
        Self { n_channels, n_timepoints, layout: UmmFeatureLayout::TimePrime }
    }

    pub const fn n_channels(&self) -> usize {
        self.n_channels
    }

    pub const fn n_timepoints(&self) -> usize {
        self.n_timepoints
    }

    pub const fn feature_count(&self) -> usize {
        self.n_channels * self.n_timepoints
    }

    pub const fn layout(&self) -> UmmFeatureLayout {
        self.layout
    }
}

/// Simplified zero-training unsupervised mean-difference maximization over
/// epoch features.
///
/// The input trial is expected to be already represented as
/// `features x epochs`, where each epoch is a stimulus-locked response vector.
///
/// This captures the core target-vs-nontarget Mahalanobis mean-separation idea,
/// and can optionally apply the tapered block-Toeplitz covariance treatment
/// described in the published UMM variants when the feature layout is known.
pub struct InstantaneousUmmDecoder<
    'a,
    const CLASSES: usize,
    const FEATURES: usize,
    const EPOCHS: usize,
> {
    codebook: UmmCodebook<'a, CLASSES, EPOCHS>,
    regularization: f32,
    covariance_structure: Option<UmmBlockStructure>,
}

impl<'a, const CLASSES: usize, const FEATURES: usize, const EPOCHS: usize>
    InstantaneousUmmDecoder<'a, CLASSES, FEATURES, EPOCHS>
{
    pub const fn new(
        codebook: UmmCodebook<'a, CLASSES, EPOCHS>,
        regularization: f32,
    ) -> Self {
        Self { codebook, regularization, covariance_structure: None }
    }

    pub fn new_tapered_block_toeplitz(
        codebook: UmmCodebook<'a, CLASSES, EPOCHS>,
        regularization: f32,
        covariance_structure: UmmBlockStructure,
    ) -> Self {
        assert!(covariance_structure.n_channels() > 0);
        assert!(covariance_structure.n_timepoints() > 0);
        assert_eq!(covariance_structure.feature_count(), FEATURES);
        Self {
            codebook,
            regularization,
            covariance_structure: Some(covariance_structure),
        }
    }

    pub fn codebook(&self) -> &UmmCodebook<'a, CLASSES, EPOCHS> {
        &self.codebook
    }

    pub fn class_scores_f32(
        &self,
        epochs: &[[f32; EPOCHS]; FEATURES],
    ) -> [f32; CLASSES] {
        let (_overall_mean, cov) = epoch_summary_f32(epochs);
        let structured_cov =
            covariance_with_structure(&cov, self.covariance_structure);
        let mut scores = [0.0; CLASSES];
        let mut class_idx = 0;
        while class_idx < CLASSES {
            scores[class_idx] = class_score_f32(
                epochs,
                &self.codebook.labels()[class_idx],
                &structured_cov,
                self.regularization,
            );
            class_idx += 1;
        }
        scores
    }

    pub fn observe_f32(&self, epochs: &[[f32; EPOCHS]; FEATURES]) -> Decision {
        let scores = self.class_scores_f32(epochs);
        decision_from_scores(&scores)
    }

    pub fn observe_i32(&self, epochs: &[[i32; EPOCHS]; FEATURES]) -> Decision {
        let epochs = epochs_i32_to_f32(epochs);
        self.observe_f32(&epochs)
    }
}

pub(crate) fn epochs_i32_to_f32<const FEATURES: usize, const EPOCHS: usize>(
    epochs: &[[i32; EPOCHS]; FEATURES],
) -> [[f32; EPOCHS]; FEATURES] {
    let mut out = [[0.0; EPOCHS]; FEATURES];
    let mut feature_idx = 0;
    while feature_idx < FEATURES {
        let mut epoch_idx = 0;
        while epoch_idx < EPOCHS {
            out[feature_idx][epoch_idx] =
                epochs[feature_idx][epoch_idx] as f32;
            epoch_idx += 1;
        }
        feature_idx += 1;
    }
    out
}

pub(crate) fn epoch_summary_f32<const FEATURES: usize, const EPOCHS: usize>(
    epochs: &[[f32; EPOCHS]; FEATURES],
) -> ([f32; FEATURES], [[f32; FEATURES]; FEATURES]) {
    let mut mean = [0.0; FEATURES];
    let mut feature_idx = 0;
    while feature_idx < FEATURES {
        let mut sum = 0.0f32;
        let mut epoch_idx = 0;
        while epoch_idx < EPOCHS {
            sum += epochs[feature_idx][epoch_idx];
            epoch_idx += 1;
        }
        mean[feature_idx] = sum / EPOCHS as f32;
        feature_idx += 1;
    }

    let mut cov = [[0.0; FEATURES]; FEATURES];
    let scale = 1.0 / (EPOCHS.saturating_sub(1).max(1) as f32);
    let mut row = 0;
    while row < FEATURES {
        let mut col = 0;
        while col < FEATURES {
            let mut sum = 0.0f32;
            let mut epoch_idx = 0;
            while epoch_idx < EPOCHS {
                sum += (epochs[row][epoch_idx] - mean[row])
                    * (epochs[col][epoch_idx] - mean[col]);
                epoch_idx += 1;
            }
            cov[row][col] = sum * scale;
            col += 1;
        }
        row += 1;
    }

    (mean, cov)
}

pub(crate) fn partition_means<const FEATURES: usize, const EPOCHS: usize>(
    epochs: &[[f32; EPOCHS]; FEATURES],
    labels: &[u8; EPOCHS],
) -> Option<(usize, usize, [f32; FEATURES], [f32; FEATURES])> {
    let mut target_count = 0usize;
    let mut non_target_count = 0usize;
    let mut target_sum = [0.0; FEATURES];
    let mut non_target_sum = [0.0; FEATURES];

    let mut epoch_idx = 0;
    while epoch_idx < EPOCHS {
        if labels[epoch_idx] != 0 {
            target_count += 1;
            let mut feature_idx = 0;
            while feature_idx < FEATURES {
                target_sum[feature_idx] += epochs[feature_idx][epoch_idx];
                feature_idx += 1;
            }
        } else {
            non_target_count += 1;
            let mut feature_idx = 0;
            while feature_idx < FEATURES {
                non_target_sum[feature_idx] += epochs[feature_idx][epoch_idx];
                feature_idx += 1;
            }
        }
        epoch_idx += 1;
    }

    if target_count == 0 || non_target_count == 0 {
        return None;
    }

    let mut target_mean = [0.0; FEATURES];
    let mut non_target_mean = [0.0; FEATURES];
    let mut feature_idx = 0;
    while feature_idx < FEATURES {
        target_mean[feature_idx] =
            target_sum[feature_idx] / target_count as f32;
        non_target_mean[feature_idx] =
            non_target_sum[feature_idx] / non_target_count as f32;
        feature_idx += 1;
    }

    Some((target_count, non_target_count, target_mean, non_target_mean))
}

pub(crate) fn class_score_f32<const FEATURES: usize, const EPOCHS: usize>(
    epochs: &[[f32; EPOCHS]; FEATURES],
    labels: &[u8; EPOCHS],
    covariance: &[[f32; FEATURES]; FEATURES],
    regularization: f32,
) -> f32 {
    let Some((_target_count, _non_target_count, target_mean, non_target_mean)) =
        partition_means(epochs, labels)
    else {
        return 0.0;
    };

    let mut delta = [0.0; FEATURES];
    let mut idx = 0;
    while idx < FEATURES {
        delta[idx] = target_mean[idx] - non_target_mean[idx];
        idx += 1;
    }

    mahalanobis_delta_score(&delta, covariance, regularization)
}

pub(crate) fn combine_mean<const FEATURES: usize>(
    mean_a: &[f32; FEATURES],
    weight_a: f32,
    mean_b: &[f32; FEATURES],
    weight_b: f32,
) -> [f32; FEATURES] {
    if weight_a <= 0.0 {
        return *mean_b;
    }
    if weight_b <= 0.0 {
        return *mean_a;
    }

    let total = weight_a + weight_b;
    let mut out = [0.0; FEATURES];
    let mut idx = 0;
    while idx < FEATURES {
        out[idx] = (mean_a[idx] * weight_a + mean_b[idx] * weight_b) / total;
        idx += 1;
    }
    out
}

pub(crate) fn combine_covariance_summaries<const FEATURES: usize>(
    mean_a: &[f32; FEATURES],
    cov_a: &[[f32; FEATURES]; FEATURES],
    weight_a: f32,
    mean_b: &[f32; FEATURES],
    cov_b: &[[f32; FEATURES]; FEATURES],
    weight_b: f32,
) -> ([f32; FEATURES], [[f32; FEATURES]; FEATURES], f32) {
    if weight_a <= 0.0 {
        return (*mean_b, *cov_b, weight_b);
    }
    if weight_b <= 0.0 {
        return (*mean_a, *cov_a, weight_a);
    }

    let total = weight_a + weight_b;
    let out_mean = combine_mean(mean_a, weight_a, mean_b, weight_b);
    let mut out_cov = [[0.0; FEATURES]; FEATURES];
    let mut row = 0;
    while row < FEATURES {
        let delta_row = mean_b[row] - mean_a[row];
        let mut col = 0;
        while col < FEATURES {
            let delta_col = mean_b[col] - mean_a[col];
            let m2_a = cov_a[row][col] * (weight_a - 1.0).max(0.0);
            let m2_b = cov_b[row][col] * (weight_b - 1.0).max(0.0);
            let correction =
                delta_row * delta_col * ((weight_a * weight_b) / total);
            out_cov[row][col] =
                (m2_a + m2_b + correction) / (total - 1.0).max(1.0);
            col += 1;
        }
        row += 1;
    }
    (out_mean, out_cov, total)
}

pub(crate) fn covariance_with_structure<const FEATURES: usize>(
    covariance: &[[f32; FEATURES]; FEATURES],
    covariance_structure: Option<UmmBlockStructure>,
) -> [[f32; FEATURES]; FEATURES] {
    let Some(structure) = covariance_structure else {
        return *covariance;
    };
    tapered_block_toeplitz_covariance(covariance, structure)
}

pub(crate) fn tapered_block_toeplitz_covariance<const FEATURES: usize>(
    covariance: &[[f32; FEATURES]; FEATURES],
    structure: UmmBlockStructure,
) -> [[f32; FEATURES]; FEATURES] {
    let n_channels = structure.n_channels();
    let n_timepoints = structure.n_timepoints();
    if n_channels == 0
        || n_timepoints == 0
        || structure.feature_count() != FEATURES
    {
        return *covariance;
    }

    let mut out = [[0.0; FEATURES]; FEATURES];
    let mut lag = -(n_timepoints as isize) + 1;
    while lag < n_timepoints as isize {
        let abs_lag = lag.unsigned_abs();
        let count = n_timepoints - abs_lag;
        let taper = linear_taper(lag, n_timepoints);

        let mut row_channel = 0;
        while row_channel < n_channels {
            let mut col_channel = 0;
            while col_channel < n_channels {
                let mut sum = 0.0f32;
                let mut block_idx = 0;
                while block_idx < count {
                    let row_time =
                        if lag >= 0 { block_idx } else { block_idx + abs_lag };
                    let col_time =
                        if lag >= 0 { block_idx + abs_lag } else { block_idx };
                    let row = feature_index(structure, row_time, row_channel);
                    let col = feature_index(structure, col_time, col_channel);
                    sum += covariance[row][col];
                    block_idx += 1;
                }

                let averaged = (sum / count as f32) * taper;
                let mut block_idx = 0;
                while block_idx < count {
                    let row_time =
                        if lag >= 0 { block_idx } else { block_idx + abs_lag };
                    let col_time =
                        if lag >= 0 { block_idx + abs_lag } else { block_idx };
                    let row = feature_index(structure, row_time, row_channel);
                    let col = feature_index(structure, col_time, col_channel);
                    out[row][col] = averaged;
                    block_idx += 1;
                }
                col_channel += 1;
            }
            row_channel += 1;
        }
        lag += 1;
    }

    out
}

pub(crate) fn mahalanobis_delta_score<const FEATURES: usize>(
    delta: &[f32; FEATURES],
    covariance: &[[f32; FEATURES]; FEATURES],
    regularization: f32,
) -> f32 {
    let mut reg = *covariance;
    let mut idx = 0;
    while idx < FEATURES {
        reg[idx][idx] += regularization;
        idx += 1;
    }
    let Some(chol) = cholesky_lower(&reg) else {
        return 0.0;
    };
    let whitened = solve_lower_vec(&chol, delta);
    let mut score = 0.0f32;
    let mut feature_idx = 0;
    while feature_idx < FEATURES {
        score += whitened[feature_idx] * whitened[feature_idx];
        feature_idx += 1;
    }
    score
}

fn linear_taper(lag: isize, n_timepoints: usize) -> f32 {
    if n_timepoints == 0 {
        return 1.0;
    }
    (n_timepoints as f32 - lag.abs() as f32) / n_timepoints as f32
}

fn feature_index(
    structure: UmmBlockStructure,
    time_idx: usize,
    channel_idx: usize,
) -> usize {
    match structure.layout() {
        UmmFeatureLayout::ChannelPrime => {
            time_idx * structure.n_channels() + channel_idx
        }
        UmmFeatureLayout::TimePrime => {
            channel_idx * structure.n_timepoints() + time_idx
        }
    }
}

fn solve_lower_vec<const N: usize>(
    lower: &[[f32; N]; N],
    rhs: &[f32; N],
) -> [f32; N] {
    let mut out = [0.0; N];
    let mut row = 0;
    while row < N {
        let mut sum = rhs[row];
        let mut k = 0;
        while k < row {
            sum -= lower[row][k] * out[k];
            k += 1;
        }
        out[row] = sum / lower[row][row];
        row += 1;
    }
    out
}

fn decision_from_scores<const CLASSES: usize>(
    scores: &[f32; CLASSES],
) -> Decision {
    let mut best_class = 0usize;
    let mut best_score = f32::NEG_INFINITY;
    let mut runner_up = f32::NEG_INFINITY;
    let mut idx = 0;
    while idx < CLASSES {
        let score = scores[idx];
        if score > best_score {
            runner_up = best_score;
            best_score = score;
            best_class = idx;
        } else if score > runner_up {
            runner_up = score;
        }
        idx += 1;
    }
    Decision {
        class_index: best_class,
        raw_score: (best_score * 1_000_000.0) as i64,
        normalized_score: best_score,
        margin: best_score - runner_up,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        tapered_block_toeplitz_covariance, InstantaneousUmmDecoder,
        UmmBlockStructure,
    };
    use crate::UmmCodebook;

    #[test]
    fn instantaneous_umm_prefers_matching_partition() {
        const CLASSES: usize = 2;
        const EPOCHS: usize = 4;

        let labels = [[1, 0, 1, 0], [0, 1, 0, 1]];
        let epochs = [[4.0, 1.0, 5.0, 1.0], [1.0, 2.0, 1.0, 2.0]];

        let codebook = UmmCodebook::<CLASSES, EPOCHS>::new(&labels);
        let decoder = InstantaneousUmmDecoder::new(codebook, 1.0e-3);
        let decision = decoder.observe_f32(&epochs);
        assert_eq!(decision.class_index, 0);
        assert!(decision.normalized_score > 0.0);
    }

    #[test]
    fn tapered_block_toeplitz_averages_and_tapers_time_lags() {
        let structure = UmmBlockStructure::new(2, 2);
        let covariance = [
            [10.0, 1.0, 6.0, 2.0],
            [1.0, 20.0, 3.0, 8.0],
            [6.0, 3.0, 30.0, 4.0],
            [2.0, 8.0, 4.0, 40.0],
        ];

        let tapered =
            tapered_block_toeplitz_covariance(&covariance, structure);

        assert_eq!(tapered[0][0], 20.0);
        assert_eq!(tapered[1][1], 30.0);
        assert_eq!(tapered[2][2], 20.0);
        assert_eq!(tapered[3][3], 30.0);
        assert_eq!(tapered[0][2], 3.0);
        assert_eq!(tapered[2][0], 3.0);
        assert_eq!(tapered[1][3], 4.0);
        assert_eq!(tapered[3][1], 4.0);
    }

    #[test]
    fn tapered_block_toeplitz_supports_time_prime_layout() {
        let structure = UmmBlockStructure::time_prime(2, 2);
        let covariance = [
            [10.0, 6.0, 1.0, 2.0],
            [6.0, 30.0, 3.0, 4.0],
            [1.0, 3.0, 20.0, 8.0],
            [2.0, 4.0, 8.0, 40.0],
        ];

        let tapered =
            tapered_block_toeplitz_covariance(&covariance, structure);

        assert_eq!(tapered[0][0], 20.0);
        assert_eq!(tapered[1][1], 20.0);
        assert_eq!(tapered[2][2], 30.0);
        assert_eq!(tapered[3][3], 30.0);
        assert_eq!(tapered[0][1], 3.0);
        assert_eq!(tapered[1][0], 3.0);
        assert_eq!(tapered[2][3], 4.0);
        assert_eq!(tapered[3][2], 4.0);
    }
}
