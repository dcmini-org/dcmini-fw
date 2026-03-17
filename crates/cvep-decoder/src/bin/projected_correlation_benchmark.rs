use cvep_decoder::{CvepDecoder, EtRcaBank, RccaBank};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Deserialize)]
struct Fixture {
    algorithm: String,
    dataset: String,
    subject: usize,
    classes: usize,
    channels: usize,
    window: usize,
    spatial_filters: Vec<Vec<f32>>,
    projected_templates: Vec<Vec<f32>>,
    benchmark_predictions: Vec<usize>,
    benchmark_labels: Vec<usize>,
    trials_i32: Vec<Vec<Vec<i32>>>,
}

#[derive(Serialize)]
struct BenchmarkResult {
    algorithm: String,
    dataset: String,
    subject: usize,
    classes: usize,
    channels: usize,
    window: usize,
    benchmark_accuracy: f32,
    rust_exact_accuracy: f32,
    rust_exact_match_rate: f32,
    rust_exact_predictions: Vec<usize>,
}

fn main() {
    let path = parse_fixture_path();
    let text =
        fs::read_to_string(&path).expect("failed to read benchmark fixture");
    let fixture: Fixture = serde_json::from_str(&text)
        .expect("failed to parse benchmark fixture");

    let result = dispatch_fixture(&fixture);
    println!(
        "{}",
        serde_json::to_string_pretty(&result)
            .expect("failed to serialize benchmark result")
    );
}

fn parse_fixture_path() -> PathBuf {
    let mut args = env::args_os();
    let _exe = args.next();
    let Some(path) = args.next() else {
        panic!("usage: projected_correlation_benchmark <fixture.json>");
    };
    PathBuf::from(path)
}

macro_rules! try_dispatch_windows {
    ($fixture:expr, $algo:literal, $runner:ident, $classes:literal, $channels:literal, [$($window:literal),* $(,)?]) => {
        $(
            if $fixture.classes == $classes
                && $fixture.channels == $channels
                && $fixture.window == $window
                && $fixture.algorithm.as_str() == $algo
            {
                return $runner::<$classes, $channels, $window>($fixture);
            }
        )*
    };
}

fn dispatch_fixture(fixture: &Fixture) -> BenchmarkResult {
    try_dispatch_windows!(
        fixture,
        "etrca",
        run_etrca,
        20,
        8,
        [
            31, 63, 94, 125, 156, 188, 219, 250, 281, 313, 315, 336, 344, 375,
            406, 438, 469, 500, 531, 563, 594, 625, 656, 672, 688, 719, 750,
            781, 813, 844, 875, 906, 938, 969, 1000, 1008, 1031, 1050, 1063,
            1094, 1125, 1156, 1188, 1219, 1250, 1281, 1313, 1344, 1375, 1406,
            1438, 1469, 1500, 1531, 1563, 1575, 1594, 1625, 1656, 1688, 1719,
            1750, 1781, 1813, 1844, 1875, 1906, 1938, 1969, 2000, 2031, 2063,
            2094, 2125, 2156, 2188, 2219, 2250, 2281, 2313, 2344, 2375, 2406,
            2438, 2469, 2500, 2531, 2563, 2594, 2625, 2656, 2688, 2719, 2750,
            2781, 2813, 2844, 2875, 2906, 2938, 2969, 3000, 3031, 3063, 3094,
            3125, 3156, 3188, 3219, 3250, 3281, 3313, 3344, 3375, 3406, 3438,
            3469, 3500, 3531, 3563, 3594, 3625, 3656, 3688, 3719, 3750, 3781,
            3813, 3844, 3875, 3906, 3938, 4000, 4063, 4125, 4188, 4250, 4313,
            4375, 4438, 4500, 4563, 4625, 4688, 4750, 4813, 4875, 4938, 5000,
            5063, 5125, 5188, 5250, 5313, 5375, 5438, 5500, 5563, 5625, 5688,
            5750, 5813, 5875, 5938, 6000, 6063, 6125, 6188, 6250, 6313, 6375,
            6438, 6500, 6563, 6625, 6688, 6750, 6813, 6875, 6938, 7000, 7063,
            7125, 7188, 7250, 7313, 7375, 7438, 7500, 7563, 7625, 7688, 7750,
            7813, 7875
        ]
    );
    try_dispatch_windows!(
        fixture,
        "etrca",
        run_etrca,
        36,
        64,
        [
            31, 63, 94, 125, 156, 188, 219, 250, 281, 313, 315, 336, 344, 375,
            406, 438, 469, 500, 525, 563, 625, 672, 688, 750, 813, 875, 938,
            1000, 1008, 1050, 1575, 7875
        ]
    );
    try_dispatch_windows!(
        fixture,
        "etrca",
        run_etrca,
        4,
        32,
        [
            31, 63, 94, 125, 156, 176, 188, 219, 250, 275, 313, 352, 375, 438,
            500, 528, 550
        ]
    );
    try_dispatch_windows!(
        fixture,
        "rcca",
        run_rcca,
        20,
        8,
        [
            31, 63, 94, 125, 156, 188, 219, 250, 281, 313, 315, 336, 344, 375,
            406, 438, 469, 500, 531, 563, 594, 625, 656, 672, 688, 719, 750,
            781, 813, 844, 875, 906, 938, 969, 1000, 1008, 1031, 1050, 1063,
            1094, 1125, 1156, 1188, 1219, 1250, 1281, 1313, 1344, 1375, 1406,
            1438, 1469, 1500, 1531, 1563, 1575, 1594, 1625, 1656, 1688, 1719,
            1750, 1781, 1813, 1844, 1875, 1906, 1938, 1969, 2000, 2031, 2063,
            2094, 2125, 2156, 2188, 2219, 2250, 2281, 2313, 2344, 2375, 2406,
            2438, 2469, 2500, 2531, 2563, 2594, 2625, 2656, 2688, 2719, 2750,
            2781, 2813, 2844, 2875, 2906, 2938, 2969, 3000, 3031, 3063, 3094,
            3125, 3156, 3188, 3219, 3250, 3281, 3313, 3344, 3375, 3406, 3438,
            3469, 3500, 3531, 3563, 3594, 3625, 3656, 3688, 3719, 3750, 3781,
            3813, 3844, 3875, 3906, 3938, 4000, 4063, 4125, 4188, 4250, 4313,
            4375, 4438, 4500, 4563, 4625, 4688, 4750, 4813, 4875, 4938, 5000,
            5063, 5125, 5188, 5250, 5313, 5375, 5438, 5500, 5563, 5625, 5688,
            5750, 5813, 5875, 5938, 6000, 6063, 6125, 6188, 6250, 6313, 6375,
            6438, 6500, 6563, 6625, 6688, 6750, 6813, 6875, 6938, 7000, 7063,
            7125, 7188, 7250, 7313, 7375, 7438, 7500, 7563, 7625, 7688, 7750,
            7813, 7875
        ]
    );
    try_dispatch_windows!(
        fixture,
        "rcca",
        run_rcca,
        36,
        64,
        [
            31, 63, 94, 125, 156, 188, 219, 250, 281, 313, 315, 336, 344, 375,
            406, 438, 469, 500, 525, 563, 625, 672, 688, 750, 813, 875, 938,
            1000, 1008, 1050, 1575, 7875
        ]
    );
    try_dispatch_windows!(
        fixture,
        "rcca",
        run_rcca,
        4,
        32,
        [
            31, 63, 94, 125, 156, 176, 188, 219, 250, 275, 313, 352, 375, 438,
            500, 528, 550
        ]
    );
    panic!(
        "unsupported benchmark shape algorithm={} classes={} channels={} window={}",
        fixture.algorithm, fixture.classes, fixture.channels, fixture.window
    )
}

fn run_etrca<
    const CLASSES: usize,
    const CHANNELS: usize,
    const WINDOW: usize,
>(
    fixture: &Fixture,
) -> BenchmarkResult {
    validate_fixture::<CLASSES, CHANNELS, WINDOW>(fixture);

    let bank = EtRcaBank::new(
        spatial_filters_to_array::<CLASSES, CHANNELS>(
            &fixture.spatial_filters,
        ),
        templates_to_array::<CLASSES, WINDOW>(&fixture.projected_templates),
    );
    let mut exact_predictions = Vec::with_capacity(fixture.trials_i32.len());

    for trial in &fixture.trials_i32 {
        let trial = trial_to_array::<CHANNELS, WINDOW>(trial);
        let mut decoder = CvepDecoder::<CHANNELS, WINDOW>::new();
        push_trial(&mut decoder, &trial);
        exact_predictions
            .push(decoder.predict_etrca(&bank).unwrap().class_index);
    }

    BenchmarkResult {
        algorithm: fixture.algorithm.clone(),
        dataset: fixture.dataset.clone(),
        subject: fixture.subject,
        classes: fixture.classes,
        channels: fixture.channels,
        window: fixture.window,
        benchmark_accuracy: class_accuracy(
            &fixture.benchmark_labels,
            &fixture.benchmark_predictions,
        ),
        rust_exact_accuracy: class_accuracy(
            &fixture.benchmark_labels,
            &exact_predictions,
        ),
        rust_exact_match_rate: class_accuracy(
            &fixture.benchmark_predictions,
            &exact_predictions,
        ),
        rust_exact_predictions: exact_predictions,
    }
}

fn run_rcca<
    const CLASSES: usize,
    const CHANNELS: usize,
    const WINDOW: usize,
>(
    fixture: &Fixture,
) -> BenchmarkResult {
    validate_fixture::<CLASSES, CHANNELS, WINDOW>(fixture);

    let bank = RccaBank::new(
        spatial_filters_to_array::<CLASSES, CHANNELS>(
            &fixture.spatial_filters,
        ),
        templates_to_array::<CLASSES, WINDOW>(&fixture.projected_templates),
    );

    let mut exact_predictions = Vec::with_capacity(fixture.trials_i32.len());
    for trial in &fixture.trials_i32 {
        let trial = trial_to_array::<CHANNELS, WINDOW>(trial);
        let mut decoder = CvepDecoder::<CHANNELS, WINDOW>::new();
        push_trial(&mut decoder, &trial);
        exact_predictions
            .push(decoder.predict_rcca(&bank).unwrap().class_index);
    }

    BenchmarkResult {
        algorithm: fixture.algorithm.clone(),
        dataset: fixture.dataset.clone(),
        subject: fixture.subject,
        classes: fixture.classes,
        channels: fixture.channels,
        window: fixture.window,
        benchmark_accuracy: class_accuracy(
            &fixture.benchmark_labels,
            &fixture.benchmark_predictions,
        ),
        rust_exact_accuracy: class_accuracy(
            &fixture.benchmark_labels,
            &exact_predictions,
        ),
        rust_exact_match_rate: class_accuracy(
            &fixture.benchmark_predictions,
            &exact_predictions,
        ),
        rust_exact_predictions: exact_predictions,
    }
}

fn validate_fixture<
    const CLASSES: usize,
    const CHANNELS: usize,
    const WINDOW: usize,
>(
    fixture: &Fixture,
) {
    assert_eq!(fixture.classes, CLASSES);
    assert_eq!(fixture.channels, CHANNELS);
    assert_eq!(fixture.window, WINDOW);
    assert_eq!(
        fixture.benchmark_labels.len(),
        fixture.benchmark_predictions.len()
    );
    assert_eq!(fixture.benchmark_labels.len(), fixture.trials_i32.len());
}

fn push_trial<const CHANNELS: usize, const WINDOW: usize>(
    decoder: &mut CvepDecoder<CHANNELS, WINDOW>,
    trial: &[[i32; WINDOW]; CHANNELS],
) {
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

fn spatial_filters_to_array<const CLASSES: usize, const CHANNELS: usize>(
    filters: &[Vec<f32>],
) -> [[f32; CHANNELS]; CLASSES] {
    let mut out = [[0.0; CHANNELS]; CLASSES];
    for (class_idx, filter) in filters.iter().enumerate() {
        assert!(class_idx < CLASSES);
        assert_eq!(filter.len(), CHANNELS);
        for (channel_idx, value) in filter.iter().enumerate() {
            out[class_idx][channel_idx] = *value;
        }
    }
    out
}

fn templates_to_array<const CLASSES: usize, const WINDOW: usize>(
    templates: &[Vec<f32>],
) -> [[f32; WINDOW]; CLASSES] {
    let mut out = [[0.0; WINDOW]; CLASSES];
    for (class_idx, template) in templates.iter().enumerate() {
        assert!(class_idx < CLASSES);
        assert_eq!(template.len(), WINDOW);
        for (sample_idx, value) in template.iter().enumerate() {
            out[class_idx][sample_idx] = *value;
        }
    }
    out
}

fn trial_to_array<const CHANNELS: usize, const WINDOW: usize>(
    trial: &[Vec<i32>],
) -> [[i32; WINDOW]; CHANNELS] {
    let mut out = [[0; WINDOW]; CHANNELS];
    for (channel_idx, samples) in trial.iter().enumerate() {
        assert!(channel_idx < CHANNELS);
        assert_eq!(samples.len(), WINDOW);
        for (sample_idx, value) in samples.iter().enumerate() {
            out[channel_idx][sample_idx] = *value;
        }
    }
    out
}
