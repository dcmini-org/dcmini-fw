use crate::internal::score::best_two;
use crate::types::Decision;

/// Accumulates class scores across repeated within-trial CCA decisions.
pub struct CcaDecisionAccumulator<const CLASSES: usize> {
    scores: [f32; CLASSES],
}

impl<const CLASSES: usize> CcaDecisionAccumulator<CLASSES> {
    pub const fn new() -> Self {
        Self { scores: [0.0; CLASSES] }
    }

    pub fn reset(&mut self) {
        self.scores = [0.0; CLASSES];
    }

    pub fn update(&mut self, scores: &[f32; CLASSES]) {
        let mut class_idx = 0;
        while class_idx < CLASSES {
            self.scores[class_idx] += scores[class_idx];
            class_idx += 1;
        }
    }

    pub fn scores(&self) -> &[f32; CLASSES] {
        &self.scores
    }

    pub fn decision(&self) -> Decision {
        let (best_class, best_score, runner_up) = best_two(&self.scores);
        Decision {
            class_index: best_class,
            raw_score: (best_score * 1_000_000.0) as i64,
            normalized_score: best_score,
            margin: best_score - runner_up,
        }
    }
}

impl<const CLASSES: usize> Default for CcaDecisionAccumulator<CLASSES> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::CcaDecisionAccumulator;

    #[test]
    fn accumulator_sums_scores_and_decides() {
        let mut accumulator = CcaDecisionAccumulator::<3>::new();
        accumulator.update(&[0.2, 0.4, 0.1]);
        accumulator.update(&[0.6, 0.1, 0.2]);

        let decision = accumulator.decision();
        assert_eq!(decision.class_index, 0);
        assert!(decision.margin > 0.0);
        assert_eq!(accumulator.scores(), &[0.8, 0.5, 0.3]);
    }

    #[test]
    fn accumulator_reset_clears_scores() {
        let mut accumulator = CcaDecisionAccumulator::<2>::new();
        accumulator.update(&[1.0, 0.5]);
        accumulator.reset();
        assert_eq!(accumulator.scores(), &[0.0, 0.0]);
    }
}
