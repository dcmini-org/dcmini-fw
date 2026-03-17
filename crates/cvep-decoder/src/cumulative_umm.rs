use crate::banks::UmmCodebook;
use crate::instantaneous_umm::{
    combine_covariance_summaries, combine_mean, covariance_with_structure,
    epoch_summary_f32, epochs_i32_to_f32, mahalanobis_delta_score,
    partition_means, UmmBlockStructure,
};
use crate::types::Decision;

/// Confidence models derived from the winner-vs-runner-up score comparison.
///
/// The accessible UMM sources clearly indicate that confidence should be based
/// on the winning class relative to the runner-up class, but they do not expose
/// one exact public formula. The default variant is therefore labeled as an
/// inference from the paper description rather than a verified reproduction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UmmConfidenceModel {
    InferredNormalizedMargin,
    MarginOverWinner,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CumulativeUmmStateSnapshot<const FEATURES: usize> {
    pub epochs_seen: usize,
    pub target_epochs_seen: usize,
    pub non_target_epochs_seen: usize,
    pub epoch_weight_sum: f32,
    pub target_weight_sum: f32,
    pub non_target_weight_sum: f32,
    pub last_confidence: f32,
    pub avg_epoch: [f32; FEATURES],
    pub avg_target: [f32; FEATURES],
    pub avg_non_target: [f32; FEATURES],
    pub covariance: [[f32; FEATURES]; FEATURES],
}

#[derive(Clone)]
struct RunningUmmState<const FEATURES: usize> {
    epochs_seen: usize,
    epoch_weight_sum: f32,
    avg_epoch: [f32; FEATURES],
    covariance: [[f32; FEATURES]; FEATURES],
    target_epochs_seen: usize,
    target_weight_sum: f32,
    avg_target: [f32; FEATURES],
    non_target_epochs_seen: usize,
    non_target_weight_sum: f32,
    avg_non_target: [f32; FEATURES],
    last_confidence: f32,
}

impl<const FEATURES: usize> Default for RunningUmmState<FEATURES> {
    fn default() -> Self {
        Self {
            epochs_seen: 0,
            epoch_weight_sum: 0.0,
            avg_epoch: [0.0; FEATURES],
            covariance: [[0.0; FEATURES]; FEATURES],
            target_epochs_seen: 0,
            target_weight_sum: 0.0,
            avg_target: [0.0; FEATURES],
            non_target_epochs_seen: 0,
            non_target_weight_sum: 0.0,
            avg_non_target: [0.0; FEATURES],
            last_confidence: 0.0,
        }
    }
}

/// Zero-training UMM with confidence-weighted cumulative learning from previous
/// predicted trials.
///
/// This moves closer to the published cumulative UMM variants by weighting the
/// running target / non-target summaries by classification confidence. The
/// covariance treatment remains a simplified dense empirical approximation.
pub struct CumulativeUmmDecoder<
    'a,
    const CLASSES: usize,
    const FEATURES: usize,
    const EPOCHS: usize,
> {
    codebook: UmmCodebook<'a, CLASSES, EPOCHS>,
    state: RunningUmmState<FEATURES>,
    regularization: f32,
    covariance_structure: Option<UmmBlockStructure>,
    confidence_model: UmmConfidenceModel,
}

impl<'a, const CLASSES: usize, const FEATURES: usize, const EPOCHS: usize>
    CumulativeUmmDecoder<'a, CLASSES, FEATURES, EPOCHS>
{
    pub const fn new(
        codebook: UmmCodebook<'a, CLASSES, EPOCHS>,
        regularization: f32,
    ) -> Self {
        Self {
            codebook,
            state: RunningUmmState {
                epochs_seen: 0,
                epoch_weight_sum: 0.0,
                avg_epoch: [0.0; FEATURES],
                covariance: [[0.0; FEATURES]; FEATURES],
                target_epochs_seen: 0,
                target_weight_sum: 0.0,
                avg_target: [0.0; FEATURES],
                non_target_epochs_seen: 0,
                non_target_weight_sum: 0.0,
                avg_non_target: [0.0; FEATURES],
                last_confidence: 0.0,
            },
            regularization,
            covariance_structure: None,
            confidence_model: UmmConfidenceModel::InferredNormalizedMargin,
        }
    }

    pub fn new_tapered_block_toeplitz(
        codebook: UmmCodebook<'a, CLASSES, EPOCHS>,
        regularization: f32,
        covariance_structure: UmmBlockStructure,
    ) -> Self {
        assert!(covariance_structure.n_channels() > 0);
        assert!(covariance_structure.n_timepoints() > 0);
        assert_eq!(covariance_structure.feature_count(), FEATURES);
        let mut decoder = Self::new(codebook, regularization);
        decoder.covariance_structure = Some(covariance_structure);
        decoder
    }

    pub const fn with_confidence_model(
        mut self,
        confidence_model: UmmConfidenceModel,
    ) -> Self {
        self.confidence_model = confidence_model;
        self
    }

    pub fn reset(&mut self) {
        self.state = RunningUmmState::default();
    }

    pub fn codebook(&self) -> &UmmCodebook<'a, CLASSES, EPOCHS> {
        &self.codebook
    }

    pub fn state_snapshot(&self) -> CumulativeUmmStateSnapshot<FEATURES> {
        CumulativeUmmStateSnapshot {
            epochs_seen: self.state.epochs_seen,
            target_epochs_seen: self.state.target_epochs_seen,
            non_target_epochs_seen: self.state.non_target_epochs_seen,
            epoch_weight_sum: self.state.epoch_weight_sum,
            target_weight_sum: self.state.target_weight_sum,
            non_target_weight_sum: self.state.non_target_weight_sum,
            last_confidence: self.state.last_confidence,
            avg_epoch: self.state.avg_epoch,
            avg_target: self.state.avg_target,
            avg_non_target: self.state.avg_non_target,
            covariance: self.state.covariance,
        }
    }

    pub fn class_scores_f32(
        &self,
        epochs: &[[f32; EPOCHS]; FEATURES],
    ) -> [f32; CLASSES] {
        let (_trial_mean, trial_cov) = epoch_summary_f32(epochs);
        let covariance = if self.state.epoch_weight_sum > 1.0 {
            self.state.covariance
        } else {
            trial_cov
        };
        let structured_covariance =
            covariance_with_structure(&covariance, self.covariance_structure);

        let mut scores = [0.0; CLASSES];
        let mut class_idx = 0;
        while class_idx < CLASSES {
            let labels = &self.codebook.labels()[class_idx];
            let Some((
                target_count,
                non_target_count,
                trial_target,
                trial_non_target,
            )) = partition_means(epochs, labels)
            else {
                scores[class_idx] = 0.0;
                class_idx += 1;
                continue;
            };

            let combined_target = combine_mean(
                &self.state.avg_target,
                self.state.target_weight_sum,
                &trial_target,
                target_count as f32,
            );
            let combined_non_target = combine_mean(
                &self.state.avg_non_target,
                self.state.non_target_weight_sum,
                &trial_non_target,
                non_target_count as f32,
            );

            let mut delta = [0.0; FEATURES];
            let mut feature_idx = 0;
            while feature_idx < FEATURES {
                delta[feature_idx] = combined_target[feature_idx]
                    - combined_non_target[feature_idx];
                feature_idx += 1;
            }
            scores[class_idx] = mahalanobis_delta_score(
                &delta,
                &structured_covariance,
                self.regularization,
            );
            class_idx += 1;
        }

        scores
    }

    pub fn observe_f32(
        &mut self,
        epochs: &[[f32; EPOCHS]; FEATURES],
    ) -> Decision {
        let scores = self.class_scores_f32(epochs);
        let (decision, confidence) = decision_and_confidence_from_scores(
            &scores,
            self.confidence_model,
        );
        self.update_state_f32(epochs, decision.class_index, confidence);
        decision
    }

    pub fn observe_i32(
        &mut self,
        epochs: &[[i32; EPOCHS]; FEATURES],
    ) -> Decision {
        let epochs = epochs_i32_to_f32(epochs);
        self.observe_f32(&epochs)
    }

    fn update_state_f32(
        &mut self,
        epochs: &[[f32; EPOCHS]; FEATURES],
        class_index: usize,
        confidence: f32,
    ) {
        self.state.last_confidence = confidence;
        self.state.epochs_seen += EPOCHS;

        let (trial_mean, trial_cov) = epoch_summary_f32(epochs);
        let labels = &self.codebook.labels()[class_index];
        let Some((
            target_count,
            non_target_count,
            trial_target,
            trial_non_target,
        )) = partition_means(epochs, labels)
        else {
            return;
        };
        self.state.target_epochs_seen += target_count;
        self.state.non_target_epochs_seen += non_target_count;

        if confidence <= 0.0 {
            return;
        }

        let epoch_weight = confidence * EPOCHS as f32;
        let (next_mean, next_cov, next_weight_sum) =
            combine_covariance_summaries(
                &self.state.avg_epoch,
                &self.state.covariance,
                self.state.epoch_weight_sum,
                &trial_mean,
                &trial_cov,
                epoch_weight,
            );
        self.state.avg_epoch = next_mean;
        self.state.covariance = next_cov;
        self.state.epoch_weight_sum = next_weight_sum;

        self.state.avg_target = combine_mean(
            &self.state.avg_target,
            self.state.target_weight_sum,
            &trial_target,
            confidence * target_count as f32,
        );
        self.state.target_weight_sum += confidence * target_count as f32;

        self.state.avg_non_target = combine_mean(
            &self.state.avg_non_target,
            self.state.non_target_weight_sum,
            &trial_non_target,
            confidence * non_target_count as f32,
        );
        self.state.non_target_weight_sum +=
            confidence * non_target_count as f32;
    }
}

fn decision_and_confidence_from_scores<const CLASSES: usize>(
    scores: &[f32; CLASSES],
    confidence_model: UmmConfidenceModel,
) -> (Decision, f32) {
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
    let decision = Decision {
        class_index: best_class,
        raw_score: (best_score * 1_000_000.0) as i64,
        normalized_score: best_score,
        margin: best_score - runner_up,
    };
    let confidence =
        confidence_from_scores(best_score, runner_up, confidence_model);
    (decision, confidence)
}

fn confidence_from_scores(
    best_score: f32,
    runner_up: f32,
    confidence_model: UmmConfidenceModel,
) -> f32 {
    if !best_score.is_finite() {
        return 0.0;
    }
    if !runner_up.is_finite() {
        return 1.0;
    }
    let margin = (best_score - runner_up).max(0.0);
    match confidence_model {
        UmmConfidenceModel::InferredNormalizedMargin => {
            let scale = best_score.abs() + runner_up.abs() + 1.0e-6;
            (margin / scale).clamp(0.0, 1.0)
        }
        UmmConfidenceModel::MarginOverWinner => {
            let scale = best_score.abs() + 1.0e-6;
            (margin / scale).clamp(0.0, 1.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CumulativeUmmDecoder, UmmConfidenceModel};
    use crate::UmmCodebook;

    #[test]
    fn cumulative_umm_accumulates_partition_statistics() {
        const CLASSES: usize = 2;
        const EPOCHS: usize = 4;

        let labels = [[1, 0, 1, 0], [1, 1, 0, 0]];
        let first = [[4.0, 1.0, 5.0, 1.0], [1.0, 2.0, 1.0, 2.0]];
        let second = [[3.0, 1.5, 4.0, 1.0], [1.0, 2.0, 1.0, 2.0]];

        let codebook = UmmCodebook::<CLASSES, EPOCHS>::new(&labels);
        let mut decoder = CumulativeUmmDecoder::new(codebook, 1.0e-3);

        let first_decision = decoder.observe_f32(&first);
        assert_eq!(first_decision.class_index, 0);
        let second_decision = decoder.observe_f32(&second);
        assert!(second_decision.normalized_score.is_finite());
        let snapshot = decoder.state_snapshot();
        assert_eq!(snapshot.epochs_seen, EPOCHS * 2);
        assert!(snapshot.target_epochs_seen > 0);
        assert!(snapshot.non_target_epochs_seen > 0);
        assert!(snapshot.epoch_weight_sum > 0.0);
        assert!(snapshot.target_weight_sum > 0.0);
        assert!(snapshot.non_target_weight_sum > 0.0);
        assert!(snapshot.last_confidence > 0.0);
    }

    #[test]
    fn cumulative_umm_confidence_is_bounded() {
        let scores = [10.0, 9.0, 2.0];
        let (_decision, confidence) =
            super::decision_and_confidence_from_scores(
                &scores,
                UmmConfidenceModel::InferredNormalizedMargin,
            );
        assert!(confidence > 0.0);
        assert!(confidence <= 1.0);
    }

    #[test]
    fn cumulative_umm_supports_alternative_confidence_models() {
        let scores = [10.0, 9.0, 2.0];
        let (_decision, normalized_margin) =
            super::decision_and_confidence_from_scores(
                &scores,
                UmmConfidenceModel::InferredNormalizedMargin,
            );
        let (_decision, margin_over_winner) =
            super::decision_and_confidence_from_scores(
                &scores,
                UmmConfidenceModel::MarginOverWinner,
            );
        assert!(margin_over_winner >= normalized_margin);
        assert!(margin_over_winner <= 1.0);
    }
}
