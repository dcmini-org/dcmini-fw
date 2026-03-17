use cvep_decoder::{
    CumulativeUmmDecoder, InstantaneousUmmDecoder, UmmBlockStructure,
    UmmCodebook, UmmConfidenceModel,
};
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
    feature_count: usize,
    epochs_per_trial: usize,
    channels: usize,
    timepoints: usize,
    layout: String,
    regularization: f32,
    confidence_model: Option<String>,
    codebook: Vec<Vec<u8>>,
    benchmark_predictions: Vec<usize>,
    benchmark_labels: Vec<usize>,
    features_f32: Vec<Vec<Vec<f32>>>,
    features_i32: Vec<Vec<Vec<i32>>>,
}

#[derive(Serialize)]
struct BenchmarkResult {
    algorithm: String,
    dataset: String,
    subject: usize,
    classes: usize,
    feature_count: usize,
    epochs_per_trial: usize,
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
        .expect("failed to read UMM benchmark fixture");
    let fixture: Fixture = serde_json::from_str(&text)
        .expect("failed to parse UMM benchmark fixture");

    let result = std::thread::Builder::new()
        .name("umm-benchmark".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || dispatch_fixture(&fixture))
        .expect("failed to spawn UMM benchmark thread")
        .join()
        .expect("UMM benchmark thread panicked");
    println!(
        "{}",
        serde_json::to_string_pretty(&result)
            .expect("failed to serialize UMM benchmark result")
    );
}

fn parse_fixture_path() -> PathBuf {
    let mut args = env::args_os();
    let _exe = args.next();
    let Some(path) = args.next() else {
        panic!("usage: umm_benchmark <fixture.json>");
    };
    PathBuf::from(path)
}

macro_rules! try_dispatch_umm {
    ($fixture:expr, $classes:literal, $features:literal, [$($epochs:literal),* $(,)?]) => {
        $(
            if $fixture.classes == $classes
                && $fixture.feature_count == $features
                && $fixture.epochs_per_trial == $epochs
            {
                return run_fixture::<$classes, $features, $epochs>($fixture);
            }
        )*
    };
}

fn dispatch_fixture(fixture: &Fixture) -> BenchmarkResult {
    try_dispatch_umm!(
        fixture,
        20,
        600,
        [
            12, 27, 42, 57, 72, 87, 102, 117, 132, 147, 162, 177, 192, 207,
            222, 237, 243, 246, 249, 252, 267, 282, 297, 312, 327, 342, 357,
            372, 387, 402, 417, 432, 447, 462, 477, 492, 507, 522, 537, 552,
            567, 582, 597, 612, 627, 642, 657, 672, 687, 702, 717, 732, 747,
            762, 777, 792, 807, 822, 837, 852, 867, 882, 897, 912, 927, 942,
            957, 972, 987, 1002, 1017, 1032, 1047, 1062, 1077, 1092, 1107,
            1122, 1137, 1152, 1167, 1182, 1197, 1212, 1227, 1242, 1257, 1272,
            1287, 1302, 1317, 1332, 1347, 1362, 1377, 1392, 1407, 1422, 1437,
            1452, 1467, 1482, 1497, 1512, 1527, 1542, 1557, 1572, 1587, 1602,
            1617, 1632, 1647, 1662, 1677, 1692, 1707, 1722, 1737, 1752, 1767,
            1782, 1797, 1812, 1827, 1842, 1857, 1872, 1881, 1884, 1887, 1890
        ]
    );
    panic!(
        "unsupported UMM benchmark shape classes={} features={} epochs={}",
        fixture.classes, fixture.feature_count, fixture.epochs_per_trial
    );
}

fn run_fixture<
    const CLASSES: usize,
    const FEATURES: usize,
    const EPOCHS: usize,
>(
    fixture: &Fixture,
) -> BenchmarkResult {
    validate_fixture::<CLASSES, FEATURES, EPOCHS>(fixture);
    let codebook = codebook_to_array::<CLASSES, EPOCHS>(&fixture.codebook);
    let codebook = UmmCodebook::new(&codebook);
    let covariance_structure = Some(block_structure(fixture));
    let confidence_model = fixture
        .confidence_model
        .as_deref()
        .map(parse_confidence_model)
        .unwrap_or(UmmConfidenceModel::InferredNormalizedMargin);

    let rust_exact_predictions = if fixture.algorithm == "instantaneous_umm" {
        run_instantaneous_exact::<CLASSES, FEATURES, EPOCHS>(
            fixture,
            codebook,
            covariance_structure,
        )
    } else if fixture.algorithm == "cumulative_umm" {
        run_cumulative_exact::<CLASSES, FEATURES, EPOCHS>(
            fixture,
            codebook,
            covariance_structure,
            confidence_model,
        )
    } else {
        panic!("unsupported UMM algorithm {}", fixture.algorithm);
    };

    let rust_fixed_predictions = if fixture.algorithm == "instantaneous_umm" {
        run_instantaneous_fixed::<CLASSES, FEATURES, EPOCHS>(
            fixture,
            codebook,
            covariance_structure,
        )
    } else if fixture.algorithm == "cumulative_umm" {
        run_cumulative_fixed::<CLASSES, FEATURES, EPOCHS>(
            fixture,
            codebook,
            covariance_structure,
            confidence_model,
        )
    } else {
        panic!("unsupported UMM algorithm {}", fixture.algorithm);
    };

    BenchmarkResult {
        algorithm: fixture.algorithm.clone(),
        dataset: fixture.dataset.clone(),
        subject: fixture.subject,
        classes: fixture.classes,
        feature_count: fixture.feature_count,
        epochs_per_trial: fixture.epochs_per_trial,
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
    const FEATURES: usize,
    const EPOCHS: usize,
>(
    fixture: &Fixture,
) {
    assert_eq!(fixture.classes, CLASSES);
    assert_eq!(fixture.feature_count, FEATURES);
    assert_eq!(fixture.epochs_per_trial, EPOCHS);
    assert_eq!(fixture.channels * fixture.timepoints, FEATURES);
    assert_eq!(fixture.codebook.len(), CLASSES);
    assert_eq!(
        fixture.benchmark_labels.len(),
        fixture.benchmark_predictions.len()
    );
    assert_eq!(fixture.benchmark_labels.len(), fixture.features_f32.len());
    assert_eq!(fixture.benchmark_labels.len(), fixture.features_i32.len());
}

fn block_structure(fixture: &Fixture) -> UmmBlockStructure {
    match fixture.layout.as_str() {
        "channel_prime" => UmmBlockStructure::channel_prime(
            fixture.channels,
            fixture.timepoints,
        ),
        "time_prime" => {
            UmmBlockStructure::time_prime(fixture.channels, fixture.timepoints)
        }
        other => panic!("unsupported UMM feature layout {other}"),
    }
}

fn parse_confidence_model(name: &str) -> UmmConfidenceModel {
    match name {
        "inferred_normalized_margin" => {
            UmmConfidenceModel::InferredNormalizedMargin
        }
        "margin_over_winner" => UmmConfidenceModel::MarginOverWinner,
        other => panic!("unsupported UMM confidence model {other}"),
    }
}

fn run_instantaneous_exact<
    const CLASSES: usize,
    const FEATURES: usize,
    const EPOCHS: usize,
>(
    fixture: &Fixture,
    codebook: UmmCodebook<'_, CLASSES, EPOCHS>,
    covariance_structure: Option<UmmBlockStructure>,
) -> Vec<usize> {
    let decoder = match covariance_structure {
        Some(structure) => {
            InstantaneousUmmDecoder::new_tapered_block_toeplitz(
                codebook,
                fixture.regularization,
                structure,
            )
        }
        None => InstantaneousUmmDecoder::new(codebook, fixture.regularization),
    };
    let mut predictions = Vec::with_capacity(fixture.features_f32.len());
    for trial in &fixture.features_f32 {
        let epochs = features_f32_to_array::<FEATURES, EPOCHS>(trial);
        predictions.push(decoder.observe_f32(&epochs).class_index);
    }
    predictions
}

fn run_instantaneous_fixed<
    const CLASSES: usize,
    const FEATURES: usize,
    const EPOCHS: usize,
>(
    fixture: &Fixture,
    codebook: UmmCodebook<'_, CLASSES, EPOCHS>,
    covariance_structure: Option<UmmBlockStructure>,
) -> Vec<usize> {
    let decoder = match covariance_structure {
        Some(structure) => {
            InstantaneousUmmDecoder::new_tapered_block_toeplitz(
                codebook,
                fixture.regularization,
                structure,
            )
        }
        None => InstantaneousUmmDecoder::new(codebook, fixture.regularization),
    };
    let mut predictions = Vec::with_capacity(fixture.features_i32.len());
    for trial in &fixture.features_i32 {
        let epochs = features_i32_to_array::<FEATURES, EPOCHS>(trial);
        predictions.push(decoder.observe_i32(&epochs).class_index);
    }
    predictions
}

fn run_cumulative_exact<
    const CLASSES: usize,
    const FEATURES: usize,
    const EPOCHS: usize,
>(
    fixture: &Fixture,
    codebook: UmmCodebook<'_, CLASSES, EPOCHS>,
    covariance_structure: Option<UmmBlockStructure>,
    confidence_model: UmmConfidenceModel,
) -> Vec<usize> {
    let mut decoder = match covariance_structure {
        Some(structure) => CumulativeUmmDecoder::new_tapered_block_toeplitz(
            codebook,
            fixture.regularization,
            structure,
        ),
        None => CumulativeUmmDecoder::new(codebook, fixture.regularization),
    }
    .with_confidence_model(confidence_model);
    let mut predictions = Vec::with_capacity(fixture.features_f32.len());
    for trial in &fixture.features_f32 {
        let epochs = features_f32_to_array::<FEATURES, EPOCHS>(trial);
        predictions.push(decoder.observe_f32(&epochs).class_index);
    }
    predictions
}

fn run_cumulative_fixed<
    const CLASSES: usize,
    const FEATURES: usize,
    const EPOCHS: usize,
>(
    fixture: &Fixture,
    codebook: UmmCodebook<'_, CLASSES, EPOCHS>,
    covariance_structure: Option<UmmBlockStructure>,
    confidence_model: UmmConfidenceModel,
) -> Vec<usize> {
    let mut decoder = match covariance_structure {
        Some(structure) => CumulativeUmmDecoder::new_tapered_block_toeplitz(
            codebook,
            fixture.regularization,
            structure,
        ),
        None => CumulativeUmmDecoder::new(codebook, fixture.regularization),
    }
    .with_confidence_model(confidence_model);
    let mut predictions = Vec::with_capacity(fixture.features_i32.len());
    for trial in &fixture.features_i32 {
        let epochs = features_i32_to_array::<FEATURES, EPOCHS>(trial);
        predictions.push(decoder.observe_i32(&epochs).class_index);
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

fn codebook_to_array<const CLASSES: usize, const EPOCHS: usize>(
    codebook: &[Vec<u8>],
) -> Box<[[u8; EPOCHS]; CLASSES]> {
    let mut out = Box::new([[0u8; EPOCHS]; CLASSES]);
    for (class_idx, labels) in codebook.iter().enumerate() {
        assert!(class_idx < CLASSES);
        assert_eq!(labels.len(), EPOCHS);
        for (epoch_idx, value) in labels.iter().enumerate() {
            out[class_idx][epoch_idx] = *value;
        }
    }
    out
}

fn features_f32_to_array<const FEATURES: usize, const EPOCHS: usize>(
    trial: &[Vec<f32>],
) -> Box<[[f32; EPOCHS]; FEATURES]> {
    let mut out = Box::new([[0.0f32; EPOCHS]; FEATURES]);
    for (feature_idx, epochs) in trial.iter().enumerate() {
        assert!(feature_idx < FEATURES);
        assert_eq!(epochs.len(), EPOCHS);
        for (epoch_idx, value) in epochs.iter().enumerate() {
            out[feature_idx][epoch_idx] = *value;
        }
    }
    out
}

fn features_i32_to_array<const FEATURES: usize, const EPOCHS: usize>(
    trial: &[Vec<i32>],
) -> Box<[[i32; EPOCHS]; FEATURES]> {
    let mut out = Box::new([[0i32; EPOCHS]; FEATURES]);
    for (feature_idx, epochs) in trial.iter().enumerate() {
        assert!(feature_idx < FEATURES);
        assert_eq!(epochs.len(), EPOCHS);
        for (epoch_idx, value) in epochs.iter().enumerate() {
            out[feature_idx][epoch_idx] = *value;
        }
    }
    out
}
