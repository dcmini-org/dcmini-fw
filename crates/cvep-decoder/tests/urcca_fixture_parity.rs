#![cfg(feature = "host-tools")]

use cvep_decoder::{UrCcaBank, UrCcaDecoder, UrCcaStateSnapshot};
use serde::Deserialize;
use std::env;
use std::fs;

const CLASSES: usize = 20;
const CHANNELS: usize = 8;
const FEATURES: usize = 144;
const WINDOW: usize = 1008;

#[derive(Deserialize)]
struct Fixture {
    classes: usize,
    channels: usize,
    features: usize,
    window: usize,
    encodings: Vec<Vec<Vec<f32>>>,
    trials: Vec<Vec<Vec<f32>>>,
    benchmark_predictions: Vec<usize>,
    benchmark_labels: Vec<usize>,
    regularization: f32,
    reference_states: Option<Vec<ReferenceState>>,
}

#[derive(Deserialize)]
struct ReferenceState {
    trial_index: usize,
    predicted_class: usize,
    scores: Vec<f32>,
    samples_seen: usize,
    avg_x: Vec<f32>,
    avg_y: Vec<f32>,
    cov_x: Vec<Vec<f32>>,
    cov_y: Vec<Vec<f32>>,
    cov_xy: Vec<Vec<f32>>,
}

#[test]
fn urcca_fixture_matches_pyntbci_predictions() {
    let Ok(path) = env::var("URCCA_FIXTURE_JSON") else {
        eprintln!("URCCA_FIXTURE_JSON not set; skipping parity fixture test");
        return;
    };

    let fixture_text = fs::read_to_string(path).expect("failed to read urcca fixture");
    let fixture: Fixture =
        serde_json::from_str(&fixture_text).expect("failed to parse urcca fixture");

    assert_eq!(fixture.classes, CLASSES);
    assert_eq!(fixture.channels, CHANNELS);
    assert_eq!(fixture.features, FEATURES);
    assert_eq!(fixture.window, WINDOW);
    assert_eq!(fixture.trials.len(), fixture.benchmark_predictions.len());
    assert_eq!(fixture.trials.len(), fixture.benchmark_labels.len());

    let encodings = boxed_encodings(fixture.encodings);
    let bank = UrCcaBank::<CLASSES, FEATURES, WINDOW>::new(&encodings);
    let mut decoder =
        UrCcaDecoder::<CLASSES, CHANNELS, FEATURES, WINDOW>::new(
            bank,
            fixture.regularization,
        );

    let mut predictions = Vec::with_capacity(fixture.trials.len());
    let mut snapshots = Vec::new();
    for (trial_index, trial) in fixture.trials.into_iter().enumerate() {
        let trial = trial_to_array(trial);
        let scores = if fixture.reference_states.is_some() {
            Some(decoder.class_scores_f32(&trial))
        } else {
            None
        };
        let prediction = decoder.observe_f32(&trial).class_index;
        predictions.push(prediction);
        if let Some(reference_states) = fixture.reference_states.as_ref() {
            if reference_states
                .iter()
                .any(|state| state.trial_index == trial_index)
            {
                snapshots.push((
                    trial_index,
                    prediction,
                    scores.expect("missing Rust score snapshot"),
                    decoder.state_snapshot(),
                ));
            }
        }
    }

    if let Some(reference_states) = fixture.reference_states {
        for reference in reference_states {
            let (_, prediction, scores, snapshot) = snapshots
                .iter()
                .find(|(trial_index, _, _, _)| *trial_index == reference.trial_index)
                .expect("missing Rust snapshot for reference state");
            assert_score_close(reference.trial_index, &scores, &reference.scores);
            assert_eq!(
                *prediction, reference.predicted_class,
                "reference predicted class mismatch at trial {}",
                reference.trial_index
            );
            assert_state_close(reference.trial_index, &snapshot, &reference);
        }
    }

    assert_eq!(predictions, fixture.benchmark_predictions);
}

fn assert_score_close(
    trial_index: usize,
    left: &[f32; CLASSES],
    right: &[f32],
) {
    assert_max_abs_diff(
        trial_index,
        "scores",
        max_abs_diff_1d(left, right),
        5.0e-3,
    );
}

fn assert_state_close(
    trial_index: usize,
    snapshot: &UrCcaStateSnapshot<CHANNELS, FEATURES>,
    reference: &ReferenceState,
) {
    assert_eq!(
        snapshot.samples_seen, reference.samples_seen,
        "samples_seen mismatch at trial {}",
        trial_index
    );
    assert_max_abs_diff(
        trial_index,
        "avg_x",
        max_abs_diff_1d(&snapshot.avg_x, &reference.avg_x),
        1.0e-4,
    );
    assert_max_abs_diff(
        trial_index,
        "avg_y",
        max_abs_diff_1d(&snapshot.avg_y, &reference.avg_y),
        1.0e-4,
    );
    assert_max_abs_diff(
        trial_index,
        "cov_x",
        max_abs_diff_2d(&snapshot.cov_x, &reference.cov_x),
        1.0e-3,
    );
    assert_max_abs_diff(
        trial_index,
        "cov_y",
        max_abs_diff_2d(&snapshot.cov_y, &reference.cov_y),
        1.0e-3,
    );
    assert_max_abs_diff(
        trial_index,
        "cov_xy",
        max_abs_diff_2d(&snapshot.cov_xy, &reference.cov_xy),
        1.0e-3,
    );
}

fn assert_max_abs_diff(
    trial_index: usize,
    label: &str,
    diff: f32,
    tolerance: f32,
) {
    assert!(
        diff <= tolerance,
        "{label} max abs diff at trial {trial_index} exceeded tolerance: {diff} > {tolerance}"
    );
}

fn max_abs_diff_1d<const N: usize>(
    left: &[f32; N],
    right: &[f32],
) -> f32 {
    assert_eq!(right.len(), N);
    let mut max_diff = 0.0f32;
    let mut idx = 0;
    while idx < N {
        let diff = (left[idx] - right[idx]).abs();
        if diff > max_diff {
            max_diff = diff;
        }
        idx += 1;
    }
    max_diff
}

fn max_abs_diff_2d<const R: usize, const C: usize>(
    left: &[[f32; C]; R],
    right: &[Vec<f32>],
) -> f32 {
    assert_eq!(right.len(), R);
    let mut max_diff = 0.0f32;
    let mut row = 0;
    while row < R {
        assert_eq!(right[row].len(), C);
        let mut col = 0;
        while col < C {
            let diff = (left[row][col] - right[row][col]).abs();
            if diff > max_diff {
                max_diff = diff;
            }
            col += 1;
        }
        row += 1;
    }
    max_diff
}

fn boxed_encodings(
    encodings: Vec<Vec<Vec<f32>>>,
) -> Box<[[[f32; WINDOW]; FEATURES]; CLASSES]> {
    let mut flat =
        Vec::with_capacity(CLASSES * FEATURES * WINDOW);
    for class in encodings {
        assert_eq!(class.len(), FEATURES);
        for feature in class {
            assert_eq!(feature.len(), WINDOW);
            flat.extend(feature);
        }
    }

    let boxed: Box<[f32]> = flat.into_boxed_slice();
    let boxed: Box<[f32; CLASSES * FEATURES * WINDOW]> =
        boxed.try_into().expect("invalid encoding length");
    let raw = Box::into_raw(boxed) as *mut [[[f32; WINDOW]; FEATURES]; CLASSES];
    unsafe { Box::from_raw(raw) }
}

fn trial_to_array(trial: Vec<Vec<f32>>) -> [[f32; WINDOW]; CHANNELS] {
    let mut out = [[0.0; WINDOW]; CHANNELS];
    for (channel_idx, samples) in trial.into_iter().enumerate() {
        assert!(channel_idx < CHANNELS);
        assert_eq!(samples.len(), WINDOW);
        for (sample_idx, value) in samples.into_iter().enumerate() {
            out[channel_idx][sample_idx] = value;
        }
    }
    out
}
