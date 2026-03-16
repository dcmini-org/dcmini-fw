use cvep_decoder::{CumulativeCcaDecoder, InstantaneousCcaDecoder, UrCcaBank};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Deserialize)]
struct Fixture {
    algorithm: String,
    dataset: String,
    subject: usize,
    classes: usize,
    channels: usize,
    features: usize,
    window: usize,
    regularization: f32,
    encodings: Vec<Vec<Vec<f32>>>,
    benchmark_predictions: Vec<usize>,
    benchmark_labels: Vec<usize>,
    trials_f32: Vec<Vec<Vec<f32>>>,
    trials_i32: Vec<Vec<Vec<i32>>>,
}

#[derive(Serialize)]
struct BenchmarkResult {
    algorithm: String,
    dataset: String,
    subject: usize,
    classes: usize,
    channels: usize,
    features: usize,
    window: usize,
    benchmark_accuracy: f32,
    rust_exact_accuracy: f32,
    rust_exact_match_rate: f32,
    rust_fixed_accuracy: f32,
    rust_fixed_match_rate: f32,
    rust_exact_predictions: Vec<usize>,
    rust_fixed_predictions: Vec<usize>,
}

fn main() {
    let path = parse_fixture_path();
    let text = fs::read_to_string(&path)
        .expect("failed to read CCA benchmark fixture");
    let fixture: Fixture = serde_json::from_str(&text)
        .expect("failed to parse CCA benchmark fixture");

    let result = std::thread::Builder::new()
        .name("cca-benchmark".to_string())
        .stack_size(256 * 1024 * 1024)
        .spawn(move || dispatch_fixture(&fixture))
        .expect("failed to spawn CCA benchmark thread")
        .join()
        .expect("CCA benchmark thread panicked");
    println!(
        "{}",
        serde_json::to_string_pretty(&result)
            .expect("failed to serialize CCA benchmark result")
    );
}

fn parse_fixture_path() -> PathBuf {
    let mut args = env::args_os();
    let _exe = args.next();
    let Some(path) = args.next() else {
        panic!("usage: cca_benchmark <fixture.json>");
    };
    PathBuf::from(path)
}

macro_rules! try_dispatch_cca {
    ($fixture:expr, $classes:literal, $channels:literal, $features:literal, [$($window:literal),* $(,)?]) => {
        $(
            if $fixture.classes == $classes
                && $fixture.channels == $channels
                && $fixture.features == $features
                && $fixture.window == $window
            {
                return run_fixture::<$classes, $channels, $features, $window>($fixture);
            }
        )*
    };
}

fn dispatch_fixture(fixture: &Fixture) -> BenchmarkResult {
    try_dispatch_cca!(
        fixture,
        20,
        8,
        150,
        [
            63, 125, 188, 250, 313, 375, 438, 500, 563, 625, 688, 750, 813,
            875, 938, 1000, 1063, 1125, 1188, 1250, 1313, 1375, 1438, 1500,
            1563, 1625, 1688, 1750, 1813, 1875, 1938, 2000, 2063, 2125, 2188,
            2250, 2313, 2375, 2438, 2500, 2563, 2625, 2688, 2750, 2813, 2875,
            2938, 3000, 3063, 3125, 3188, 3250, 3313, 3375, 3438, 3500, 3563,
            3625, 3688, 3750, 3813, 3875, 3938, 4000, 4063, 4125, 4188, 4250,
            4313, 4375, 4438, 4500, 4563, 4625, 4688, 4750, 4813, 4875, 4938,
            5000, 5063, 5125, 5188, 5250, 5313, 5375, 5438, 5500, 5563, 5625,
            5688, 5750, 5813, 5875, 5938, 6000, 6063, 6125, 6188, 6250, 6313,
            6375, 6438, 6500, 6563, 6625, 6688, 6750, 6813, 6875, 6938, 7000,
            7063, 7125, 7188, 7250, 7313, 7375, 7438, 7500, 7563, 7625, 7688,
            7750, 7813, 7875
        ]
    );
    try_dispatch_cca!(
        fixture,
        20,
        8,
        108,
        [
            45, 90, 135, 180, 225, 270, 315, 360, 405, 450, 495, 540, 585,
            630, 675, 720, 765, 810, 855, 900, 945, 990, 1035, 1080, 1125,
            1170, 1215, 1260, 1305, 1350, 1395, 1440, 1485, 1530, 1575, 1620,
            1665, 1710, 1755, 1800, 1845, 1890, 1935, 1980, 2025, 2070, 2115,
            2160, 2205, 2250, 2295, 2340, 2385, 2430, 2475, 2520, 2565, 2610,
            2655, 2700, 2745, 2790, 2835, 2880, 2925, 2970, 3015, 3060, 3105,
            3150, 3195, 3240, 3285, 3330, 3375, 3420, 3465, 3510, 3555, 3600,
            3645, 3690, 3735, 3780, 3825, 3870, 3915, 3960, 4005, 4050, 4095,
            4140, 4185, 4230, 4275, 4320, 4365, 4410, 4455, 4500, 4545, 4590,
            4635, 4680, 4725, 4770, 4815, 4860, 4905, 4950, 4995, 5040, 5085,
            5130, 5175, 5220, 5265, 5310, 5355, 5400, 5445, 5490, 5535, 5580,
            5625, 5670
        ]
    );
    panic!(
        "unsupported CCA benchmark shape classes={} channels={} features={} window={}",
        fixture.classes, fixture.channels, fixture.features, fixture.window
    );
}

fn run_fixture<
    const CLASSES: usize,
    const CHANNELS: usize,
    const FEATURES: usize,
    const WINDOW: usize,
>(
    fixture: &Fixture,
) -> BenchmarkResult {
    validate_fixture::<CLASSES, CHANNELS, FEATURES, WINDOW>(fixture);
    let encodings =
        encodings_to_array::<CLASSES, FEATURES, WINDOW>(&fixture.encodings);
    let bank = UrCcaBank::<CLASSES, FEATURES, WINDOW>::new(&encodings);

    let rust_exact_predictions = if fixture.algorithm == "instantaneous_cca" {
        run_instantaneous_exact::<CLASSES, CHANNELS, FEATURES, WINDOW>(
            fixture, bank,
        )
    } else if fixture.algorithm == "cumulative_cca" {
        run_cumulative_exact::<CLASSES, CHANNELS, FEATURES, WINDOW>(
            fixture, bank,
        )
    } else {
        panic!("unsupported CCA algorithm {}", fixture.algorithm);
    };

    let rust_fixed_predictions = if fixture.algorithm == "instantaneous_cca" {
        run_instantaneous_fixed::<CLASSES, CHANNELS, FEATURES, WINDOW>(
            fixture, bank,
        )
    } else if fixture.algorithm == "cumulative_cca" {
        run_cumulative_fixed::<CLASSES, CHANNELS, FEATURES, WINDOW>(
            fixture, bank,
        )
    } else {
        panic!("unsupported CCA algorithm {}", fixture.algorithm);
    };

    BenchmarkResult {
        algorithm: fixture.algorithm.clone(),
        dataset: fixture.dataset.clone(),
        subject: fixture.subject,
        classes: fixture.classes,
        channels: fixture.channels,
        features: fixture.features,
        window: fixture.window,
        benchmark_accuracy: class_accuracy(
            &fixture.benchmark_labels,
            &fixture.benchmark_predictions,
        ),
        rust_exact_accuracy: class_accuracy(
            &fixture.benchmark_labels,
            &rust_exact_predictions,
        ),
        rust_exact_match_rate: class_accuracy(
            &fixture.benchmark_predictions,
            &rust_exact_predictions,
        ),
        rust_fixed_accuracy: class_accuracy(
            &fixture.benchmark_labels,
            &rust_fixed_predictions,
        ),
        rust_fixed_match_rate: class_accuracy(
            &fixture.benchmark_predictions,
            &rust_fixed_predictions,
        ),
        rust_exact_predictions,
        rust_fixed_predictions,
    }
}

fn validate_fixture<
    const CLASSES: usize,
    const CHANNELS: usize,
    const FEATURES: usize,
    const WINDOW: usize,
>(
    fixture: &Fixture,
) {
    assert_eq!(fixture.classes, CLASSES);
    assert_eq!(fixture.channels, CHANNELS);
    assert_eq!(fixture.features, FEATURES);
    assert_eq!(fixture.window, WINDOW);
    assert_eq!(
        fixture.benchmark_labels.len(),
        fixture.benchmark_predictions.len()
    );
    assert_eq!(fixture.benchmark_labels.len(), fixture.trials_f32.len());
    assert_eq!(fixture.benchmark_labels.len(), fixture.trials_i32.len());
}

fn run_instantaneous_exact<
    const CLASSES: usize,
    const CHANNELS: usize,
    const FEATURES: usize,
    const WINDOW: usize,
>(
    fixture: &Fixture,
    bank: UrCcaBank<'_, CLASSES, FEATURES, WINDOW>,
) -> Vec<usize> {
    let decoder = InstantaneousCcaDecoder::new(bank, fixture.regularization);
    let mut predictions = Vec::with_capacity(fixture.trials_f32.len());
    for trial in &fixture.trials_f32 {
        let trial = trial_f32_to_array::<CHANNELS, WINDOW>(trial);
        predictions.push(decoder.observe_f32(&trial).class_index);
    }
    predictions
}

fn run_instantaneous_fixed<
    const CLASSES: usize,
    const CHANNELS: usize,
    const FEATURES: usize,
    const WINDOW: usize,
>(
    fixture: &Fixture,
    bank: UrCcaBank<'_, CLASSES, FEATURES, WINDOW>,
) -> Vec<usize> {
    let decoder = InstantaneousCcaDecoder::new(bank, fixture.regularization);
    let mut predictions = Vec::with_capacity(fixture.trials_i32.len());
    for trial in &fixture.trials_i32 {
        let trial = trial_i32_to_array::<CHANNELS, WINDOW>(trial);
        predictions.push(decoder.observe_i32(&trial).class_index);
    }
    predictions
}

fn run_cumulative_exact<
    const CLASSES: usize,
    const CHANNELS: usize,
    const FEATURES: usize,
    const WINDOW: usize,
>(
    fixture: &Fixture,
    bank: UrCcaBank<'_, CLASSES, FEATURES, WINDOW>,
) -> Vec<usize> {
    let mut decoder = CumulativeCcaDecoder::new(bank, fixture.regularization);
    let mut predictions = Vec::with_capacity(fixture.trials_f32.len());
    for trial in &fixture.trials_f32 {
        let trial = trial_f32_to_array::<CHANNELS, WINDOW>(trial);
        predictions.push(decoder.observe_f32(&trial).class_index);
    }
    predictions
}

fn run_cumulative_fixed<
    const CLASSES: usize,
    const CHANNELS: usize,
    const FEATURES: usize,
    const WINDOW: usize,
>(
    fixture: &Fixture,
    bank: UrCcaBank<'_, CLASSES, FEATURES, WINDOW>,
) -> Vec<usize> {
    let mut decoder = CumulativeCcaDecoder::new(bank, fixture.regularization);
    let mut predictions = Vec::with_capacity(fixture.trials_i32.len());
    for trial in &fixture.trials_i32 {
        let trial = trial_i32_to_array::<CHANNELS, WINDOW>(trial);
        predictions.push(decoder.observe_i32(&trial).class_index);
    }
    predictions
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

fn encodings_to_array<
    const CLASSES: usize,
    const FEATURES: usize,
    const WINDOW: usize,
>(
    encodings: &[Vec<Vec<f32>>],
) -> [[[f32; WINDOW]; FEATURES]; CLASSES] {
    let mut out = [[[0.0; WINDOW]; FEATURES]; CLASSES];
    for (class_idx, class_encodings) in encodings.iter().enumerate() {
        assert!(class_idx < CLASSES);
        assert_eq!(class_encodings.len(), FEATURES);
        for (feature_idx, feature) in class_encodings.iter().enumerate() {
            assert_eq!(feature.len(), WINDOW);
            for (sample_idx, value) in feature.iter().enumerate() {
                out[class_idx][feature_idx][sample_idx] = *value;
            }
        }
    }
    out
}

fn trial_f32_to_array<const CHANNELS: usize, const WINDOW: usize>(
    trial: &[Vec<f32>],
) -> [[f32; WINDOW]; CHANNELS] {
    let mut out = [[0.0; WINDOW]; CHANNELS];
    for (channel_idx, samples) in trial.iter().enumerate() {
        assert!(channel_idx < CHANNELS);
        assert_eq!(samples.len(), WINDOW);
        for (sample_idx, value) in samples.iter().enumerate() {
            out[channel_idx][sample_idx] = *value;
        }
    }
    out
}

fn trial_i32_to_array<const CHANNELS: usize, const WINDOW: usize>(
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
