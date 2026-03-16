#![no_std]

//! Fixed-capacity CVEP decoder primitives for `no_std` targets.
//!
//! The decoder stores a sliding EEG window in a ring buffer and scores it
//! against a precomputed projected-correlation bank. Decoder parameters are
//! expected to be prepared offline after spatial filtering, down-selection, and
//! mean-centering.

mod banks;
mod cca_accumulator;
mod cca_update_policy;
mod cumulative_cca;
mod cumulative_umm;
mod decoder;
mod instantaneous_cca;
mod instantaneous_umm;
mod internal;
mod preprocessing;
mod types;
mod urcca;

pub use banks::{
    EtRcaBank, ProjectedCorrelationBank, RccaBank, UmmCodebook, UrCcaBank,
};
pub use cca_accumulator::CcaDecisionAccumulator;
pub use cca_update_policy::CumulativeCcaUpdatePolicy;
pub use cumulative_cca::{CumulativeCcaDecoder, CumulativeCcaStateSnapshot};
pub use cumulative_umm::{
    CumulativeUmmDecoder, CumulativeUmmStateSnapshot, UmmConfidenceModel,
};
pub use decoder::CvepDecoder;
pub use instantaneous_cca::InstantaneousCcaDecoder;
pub use instantaneous_umm::{
    InstantaneousUmmDecoder, UmmBlockStructure, UmmFeatureLayout,
};
pub use preprocessing::{ChannelPreprocessor, SosCascade, SosSection};
pub use types::{Decision, DecodeError};
pub use urcca::{UrCcaDecoder, UrCcaStateSnapshot};

#[cfg(test)]
mod tests {
    use super::{
        ChannelPreprocessor, CvepDecoder, EtRcaBank, ProjectedCorrelationBank,
        RccaBank, SosCascade, SosSection, UrCcaBank, UrCcaDecoder,
    };

    const CHANNELS: usize = 2;
    const WINDOW: usize = 8;
    const CLASSES: usize = 2;

    #[test]
    fn exact_etrca_path_matches_class_specific_projection() {
        let bank = EtRcaBank::new(
            [[1.0, 0.25], [-1.0, 0.5]],
            [
                [2.0, -2.0, 2.0, -2.0, 2.0, -2.0, 2.0, -2.0],
                [-2.0, -2.0, 2.0, 2.0, -2.0, -2.0, 2.0, 2.0],
            ],
        );
        let mut decoder = CvepDecoder::<CHANNELS, WINDOW>::new();
        let mut idx = 0;
        while idx < WINDOW {
            let t = bank.templates()[0][idx];
            decoder
                .push([((t * 64.0) as i32) + 900, ((t * 16.0) as i32) - 200]);
            idx += 1;
        }

        let decision = decoder.predict_etrca(&bank).unwrap();
        assert_eq!(decision.class_index, 0);
        assert!(decision.normalized_score > 0.9);
    }

    #[test]
    fn exact_rcca_path_reuses_projected_correlation_runtime() {
        let bank: RccaBank<CLASSES, CHANNELS, WINDOW> =
            ProjectedCorrelationBank::new(
                [[0.5, 1.0], [1.0, -0.5]],
                [
                    [1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0],
                    [-1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0],
                ],
            );
        let mut decoder = CvepDecoder::<CHANNELS, WINDOW>::new();
        let mut idx = 0;
        while idx < WINDOW {
            let t = bank.templates()[1][idx];
            decoder.push([
                ((t * 28.0) as i32) - 300,
                ((t * -56.0) as i32) + 1200,
            ]);
            idx += 1;
        }

        let decision = decoder.predict_rcca(&bank).unwrap();
        assert_eq!(decision.class_index, 1);
    }

    #[test]
    fn preprocessing_identity_section_passes_signal() {
        let identity =
            SosCascade::<1>::from_scipy_rows([[1.0, 0.0, 0.0, 1.0, 0.0, 0.0]]);
        let mut filter = ChannelPreprocessor::<2, 1>::shared(identity);
        let out0 = filter.process_frame([1.25, -0.5]);
        let out1 = filter.process_frame([2.0, 3.5]);
        assert_eq!(out0, [1.25, -0.5]);
        assert_eq!(out1, [2.0, 3.5]);
    }

    #[test]
    fn preprocessing_keeps_channel_state_separate() {
        let leaky =
            SosSection::from_scipy_row([0.5, 0.0, 0.0, 1.0, -0.5, 0.0]);
        let mut filter =
            ChannelPreprocessor::<2, 1>::shared(SosCascade::new([leaky]));
        let first = filter.process_frame([2.0, 0.0]);
        let second = filter.process_frame([0.0, 4.0]);
        assert_eq!(first[0], 1.0);
        assert_eq!(first[1], 0.0);
        assert_eq!(second[0], 0.5);
        assert_eq!(second[1], 2.0);
    }

    #[test]
    fn urcca_processes_multiple_trials() {
        const FEATURES: usize = 2;
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
        let mut decoder = UrCcaDecoder::new(bank, 1.0e-3);

        let trial_a = [
            [1000, 0, 1000, 0, 1000, 0, 1000, 0],
            [0, 800, 0, 800, 0, 800, 0, 800],
        ];
        let trial_b = [
            [0, 1000, 0, 1000, 0, 1000, 0, 1000],
            [800, 0, 800, 0, 800, 0, 800, 0],
        ];

        let decision_a = decoder.observe_i32(&trial_a);
        assert_eq!(decision_a.class_index, 0);

        let decision_b = decoder.observe_i32(&trial_b);
        assert!(decision_b.class_index < CLASSES);
        assert!(decision_b.normalized_score.is_finite());
    }
}
