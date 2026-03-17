#![cfg(feature = "host-tools")]

use cvep_decoder::{CvepDecoder, EtRcaBank};
use serde::Deserialize;
use std::env;
use std::fs;

const CLASSES: usize = 20;
const CHANNELS: usize = 8;
const WINDOW: usize = 1008;

#[derive(Deserialize)]
struct Fixture {
    classes: usize,
    channels: usize,
    window: usize,
    spatial_filters: Vec<Vec<f32>>,
    projected_templates: Vec<Vec<f32>>,
    benchmark_predictions: Vec<usize>,
    benchmark_labels: Vec<usize>,
    trials_i32: Vec<Vec<Vec<i32>>>,
}

#[test]
fn etrca_fixture_reports_exact_accuracy() {
    let Ok(path) = env::var("ETRCA_FIXTURE_JSON") else {
        eprintln!(
            "ETRCA_FIXTURE_JSON not set; skipping eTRCA fixture accuracy test"
        );
        return;
    };

    let fixture_text =
        fs::read_to_string(path).expect("failed to read etrca fixture");
    let fixture: Fixture = serde_json::from_str(&fixture_text)
        .expect("failed to parse etrca fixture");

    assert_eq!(fixture.classes, CLASSES);
    assert_eq!(fixture.channels, CHANNELS);
    assert_eq!(fixture.window, WINDOW);
    assert_eq!(fixture.trials_i32.len(), fixture.benchmark_predictions.len());
    assert_eq!(fixture.trials_i32.len(), fixture.benchmark_labels.len());

    let bank = EtRcaBank::new(
        spatial_filters_to_array(fixture.spatial_filters),
        templates_to_array(fixture.projected_templates),
    );

    let mut exact_predictions = Vec::with_capacity(fixture.trials_i32.len());

    for trial in fixture.trials_i32 {
        let mut decoder = CvepDecoder::<CHANNELS, WINDOW>::new();
        let trial = trial_to_array(trial);
        let mut sample_idx = 0;
        while sample_idx < WINDOW {
            let mut frame = [0i32; CHANNELS];
            let mut channel_idx = 0;
            while channel_idx < CHANNELS {
                frame[channel_idx] = trial[channel_idx][sample_idx];
                channel_idx += 1;
            }
            decoder.push(frame);
            sample_idx += 1;
        }

        exact_predictions
            .push(decoder.predict_etrca(&bank).unwrap().class_index);
    }

    let benchmark_accuracy = class_accuracy(
        &fixture.benchmark_labels,
        &fixture.benchmark_predictions,
    );
    let exact_accuracy =
        class_accuracy(&fixture.benchmark_labels, &exact_predictions);
    let exact_match_rate =
        class_accuracy(&fixture.benchmark_predictions, &exact_predictions);
    println!("benchmark_accuracy={benchmark_accuracy:.4}");
    println!("rust_exact_accuracy={exact_accuracy:.4}");
    println!("rust_exact_match_rate={exact_match_rate:.4}");
}

fn class_accuracy(labels: &[usize], predictions: &[usize]) -> f32 {
    assert_eq!(labels.len(), predictions.len());
    let correct = labels
        .iter()
        .zip(predictions.iter())
        .filter(|(left, right)| left == right)
        .count();
    correct as f32 / labels.len() as f32
}

fn spatial_filters_to_array(
    filters: Vec<Vec<f32>>,
) -> [[f32; CHANNELS]; CLASSES] {
    let mut out = [[0.0; CHANNELS]; CLASSES];
    for (class_idx, filter) in filters.into_iter().enumerate() {
        assert!(class_idx < CLASSES);
        assert_eq!(filter.len(), CHANNELS);
        for (channel_idx, value) in filter.into_iter().enumerate() {
            out[class_idx][channel_idx] = value;
        }
    }
    out
}

fn templates_to_array(templates: Vec<Vec<f32>>) -> [[f32; WINDOW]; CLASSES] {
    let mut out = [[0.0; WINDOW]; CLASSES];
    for (class_idx, template) in templates.into_iter().enumerate() {
        assert!(class_idx < CLASSES);
        assert_eq!(template.len(), WINDOW);
        for (sample_idx, value) in template.into_iter().enumerate() {
            out[class_idx][sample_idx] = value;
        }
    }
    out
}

fn trial_to_array(trial: Vec<Vec<i32>>) -> [[i32; WINDOW]; CHANNELS] {
    let mut out = [[0; WINDOW]; CHANNELS];
    for (channel_idx, samples) in trial.into_iter().enumerate() {
        assert!(channel_idx < CHANNELS);
        assert_eq!(samples.len(), WINDOW);
        for (sample_idx, value) in samples.into_iter().enumerate() {
            out[channel_idx][sample_idx] = value;
        }
    }
    out
}
