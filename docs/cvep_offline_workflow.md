# CVEP Offline Workflow

This repository now includes offline export paths for benchmarking the firmware
CVEP decoder against `pyntbci` on the open Thielen 2021 c-VEP data.

For a deployment-oriented comparison of supervised and zero-training decoder
families, see
[docs/cvep_zero_training_decoder_options.md](/Users/peranpl1/Documents/repos/oss/dcmini-fw/docs/cvep_zero_training_decoder_options.md).
For dataset readiness and suggested benchmark order, see
[docs/cvep_dataset_readiness_for_zero_training.md](/Users/peranpl1/Documents/repos/oss/dcmini-fw/docs/cvep_dataset_readiness_for_zero_training.md).

## Scope

The intended workflow is:

1. Use `pyntbci` offline to fit an `eTRCA` or `rCCA` model, or export a fixed
   `urCCA` encoding bank, on open benchmark data or local DC-mini calibration
   recordings.
2. Export the exact runtime parameters as class-specific spatial filters plus
   projected templates.
3. Feed the exact exported bank into the Rust `cvep-decoder` crate on device.

The current exporter targets the same `.npz` dataset format used by PyntBCI's
`thielen2021_sub-XX.npz` example files:

- `X`: `(trials, channels, samples)`
- `y`: `(trials,)`
- `fs`: scalar sampling rate
- `V`: optional code matrix

## Install

The exporter scripts embed their Python dependencies, so the simplest path is:

```bash
uv run --script crates/cvep-decoder/scripts/export_pyntbci_etrca.py --help
```

If you prefer to manage the environment yourself, installing `pyntbci`
directly is sufficient:

```bash
pip install pyntbci
```

To download the broader open c-VEP benchmark set into
[crates/cvep-decoder/data](/Users/peranpl1/Documents/repos/oss/dcmini-fw/crates/cvep-decoder/data),
run:

```bash
uv run --script crates/cvep-decoder/scripts/download_cvep_datasets.py
```

To download multiple datasets concurrently, add `--jobs`:

```bash
uv run --script crates/cvep-decoder/scripts/download_cvep_datasets.py --jobs 2
```

The live progress display tracks byte progress for the current file being
downloaded in each dataset job. It does not precompute total bytes for the
entire dataset.

## Benchmark

To benchmark the real Rust `cvep-decoder` projected-correlation runtime against
`pyntbci` on the downloaded datasets, run:

```bash
uv run --script crates/cvep-decoder/scripts/benchmark_pyntbci_vs_rust.py \
  --datasets Thielen2021 CastillosCVEP100 \
  --algorithms etrca rcca \
  --max-subjects 1 \
  --fold-index 0
```

The benchmark script:

- loads full trial windows directly from the local dataset files,
- applies raw EEG preprocessing before fitting:
  - notch filtering at `50 Hz` harmonics up to Nyquist,
  - `1-65 Hz` band-pass filtering,
  - epoching with a `0.5 s` pre-trial buffer to absorb filter and resample edge artifacts,
  - resampling after epoching, then cropping back to the intended trial window,
- resamples them to `250 Hz` by default to match the DC-mini ADS1299 base rate,
- uses full trial windows for each dataset:
  - `Thielen2015`: `4.2 s`
  - `Thielen2021`: `31.5 s`
  - `Castillos*`: `2.2 s`
- fits `pyntbci` on each subject/fold,
- exports a fixture and replays it through the real Rust benchmark binary,
- writes JSON, CSV, and HTML summaries under
  [crates/cvep-decoder/data](/Users/peranpl1/Documents/repos/oss/dcmini-fw/crates/cvep-decoder/data).

For a broader run across all downloaded datasets:

```bash
uv run --script crates/cvep-decoder/scripts/benchmark_pyntbci_vs_rust.py \
  --algorithms etrca rcca
```

To compute subject-level statistics from the benchmark JSON:

```bash
uv run --script crates/cvep-decoder/scripts/analyze_cvep_benchmark_results.py \
  --input-json crates/cvep-decoder/data/benchmark_results.json
```

The analysis step writes:

- `benchmark_analysis.json`
- `benchmark_analysis.csv`
- `benchmark_analysis.html`

It aggregates folds within each subject first, then reports:

- mean and standard deviation of subject-level accuracy
- bootstrap `95%` confidence intervals for means
- paired deltas between `pyntbci` and the Rust exact path
- paired t-tests, Wilcoxon signed-rank tests, and Cohen's `dz`

The current benchmark binary supports these resampled runtime shapes:

- `250 Hz`
  - `Thielen2021`: `(classes=20, channels=8, window=7875)`
  - `Castillos*`: `(classes=4, channels=32, window=550)`
- `Thielen2015`: `(classes=36, channels=64, window=1008)`
- `Thielen2021` short-window legacy shape: `(classes=20, channels=8, window=1008)`
- `Castillos*`: `(classes=4, channels=32, window=528)`

`urCCA` and the UMM family are not part of this projected-correlation benchmark
path. The current script targets the two projected-correlation runtimes that
already replay cleanly through the Rust binary.

Important constraint:

- the benchmark script now requires `target_fs` to divide `250 Hz` exactly,
  because that is the DC-mini base sample rate,
- exact `eTRCA` on `Thielen2015` is incompatible with that constraint because
  `1.05 s` code cycles require a multiple of `20 Hz`, and no integer divisor of
  `250 Hz` satisfies that.

## Reference Reproduction

To reproduce `pyntbci`'s `example_3_etrca.py` more directly, using the packaged
preprocessed `thielen2021_sub-XX.npz` files that ship with `pyntbci`, run:

```bash
MPLCONFIGDIR=/tmp/matplotlib \
uv run --script crates/cvep-decoder/scripts/benchmark_pyntbci_example3_etrca.py
```

This reference script:

- uses the same packaged `Thielen2021` example data as `pyntbci`,
- keeps the example's `4.2 s` trial window and `2.1 s` cycle size,
- runs chronological `5`-fold cross-validation over the `5` packaged subjects,
- computes the example-style learning and decoding curves,
- replays each fold through the Rust projected-correlation runtime,
- writes JSON, CSV, and HTML outputs under
  [crates/cvep-decoder/data](/Users/peranpl1/Documents/repos/oss/dcmini-fw/crates/cvep-decoder/data).

On the current packaged example set, this script reproduces the same headline
average shown in the example docs:

- average `pyntbci` accuracy across subjects: `0.73`
- Rust exact replay: identical fold decisions

Use this script when the goal is to match the `pyntbci` example results. Keep
the main `benchmark_pyntbci_vs_rust.py` path for the raw-dataset, DC-mini-style
evaluation.

## Export

Run the `eTRCA` exporter with a local Thielen 2021 file:

```bash
python crates/cvep-decoder/scripts/export_pyntbci_etrca.py \
  --input /path/to/thielen2021_sub-01.npz \
  --output /tmp/thielen2021_sub-01_etrca_bank.npz \
  --metadata-json /tmp/thielen2021_sub-01_etrca_bank.json
```

Run the `rCCA` exporter:

```bash
python crates/cvep-decoder/scripts/export_pyntbci_rcca.py \
  --input /path/to/thielen2021_sub-01.npz \
  --output /tmp/thielen2021_sub-01_rcca_bank.npz \
  --metadata-json /tmp/thielen2021_sub-01_rcca_bank.json
```

Run the `urCCA` exporter:

```bash
python crates/cvep-decoder/scripts/export_pyntbci_urcca.py \
  --input /path/to/thielen2021_sub-01.npz \
  --output /tmp/thielen2021_sub-01_urcca_bank.npz \
  --metadata-json /tmp/thielen2021_sub-01_urcca_bank.json
```

The exporters:

- uses the first `4.2 s` of each trial by default,
- performs the same chronological `5`-fold split pattern used in the PyntBCI
  example,
- fits `pyntbci.classifiers.eTRCA`, `pyntbci.classifiers.rCCA`, or exports the
  fixed-length encoding bank used by `pyntbci.classifiers.urCCA`,
- exports exact runtime parameters for the Rust projected-correlation runtime,
- reports holdout accuracy for both PyntBCI's predictor and an exact projected
  correlation reimplementation.

## UMM Export And Benchmark

The UMM path uses host-side epoch-feature extraction rather than projected
templates. It is intentionally explicit about:

- epoch response window length
- stimulus-locked epoch stride
- flattened feature layout
- cumulative confidence model

Export UMM features for one dataset subject:

```bash
uv run --script crates/cvep-decoder/scripts/export_umm_features.py \
  --dataset Thielen2021 \
  --subject 1 \
  --output /tmp/thielen2021_sub-01_umm_features.npz \
  --epoch-seconds 0.3 \
  --layout channel_prime
```

This export contains:

- `features`: `(trials, features, epochs)`
- `codebook`: `(classes, epochs)` binary target / non-target labels
- `labels`: `(trials,)`
- `metadata`: JSON blob with epoch length, stride, layout, and trial-window info

Benchmark UMM variants directly on the downloaded datasets:

```bash
uv run --script crates/cvep-decoder/scripts/benchmark_umm_variants.py \
  --datasets Thielen2021 Thielen2015 \
  --max-subjects 1 \
  --fold-index 0 \
  --epoch-seconds-grid 0.3 \
  --layouts channel_prime time_prime \
  --confidence-models inferred_normalized_margin margin_over_winner
```

This benchmark:

- reuses the same raw dataset loaders as
  [benchmark_pyntbci_vs_rust.py](/Users/peranpl1/Documents/repos/oss/dcmini-fw/crates/cvep-decoder/scripts/benchmark_pyntbci_vs_rust.py)
- extracts explicit stimulus-locked epoch features
- evaluates both instantaneous and cumulative UMM
- sweeps feature layout and confidence model
- writes JSON, CSV, and HTML summaries under
  [crates/cvep-decoder/data](/Users/peranpl1/Documents/repos/oss/dcmini-fw/crates/cvep-decoder/data)

Important source note:

- the accessible UMM sources confirm that cumulative confidence depends on the
  winning class relative to the runner-up class
- they do not expose one exact public formula
- the benchmark therefore treats the confidence transform as an explicit
  benchmark dimension instead of hiding one inferred choice as if it were
  source-confirmed

Benchmark the corrected Python UMM reference against the Rust UMM runtime:

```bash
uv run --script crates/cvep-decoder/scripts/benchmark_umm_vs_rust.py \
  --datasets Thielen2021 \
  --subjects 1 \
  --fold-index 0 \
  --window-seconds-grid 4.2 31.5 \
  --epoch-seconds 0.3 \
  --epoch-schedule fractional_onset \
  --lag-seconds 0.05 \
  --layout channel_prime \
  --confidence-model inferred_normalized_margin
```

This path:

- uses the corrected fractional-onset stimulus timing for UMM features
- compares Python reference predictions against the Rust UMM runtime
- writes a benchmark-results-style CSV with:
  - Python reference accuracy
  - Rust exact float-path accuracy
  - Rust fixed integer-path accuracy
- currently targets the tractable Thielen2021 `8 x 75 = 600` feature shape

## Artifact format

The `.npz` export contains:

- `class_labels`
- `fs`
- `projected_templates`
- `spatial_filters`
- `projected_template_norms`
- `pyntbci_accuracy`
- `etrca_exact_accuracy`
- `metadata`

The recommended firmware path uses `projected_templates`,
`projected_template_norms`, and `spatial_filters` because that reproduces the
reference projected-correlation scoring used by exact `eTRCA` and the current
`rCCA` export.

## Notes

- The exporter currently assumes the eTRCA model uses one spatial component.
- The Rust decoder now supports exact `eTRCA` / `rCCA` projected correlation
  and a fixed-length `urCCA` adaptive path.
- The current `rCCA` exporter targets the default no-decoding-matrix setup,
  i.e. `decoding_length = 1 / fs`. Supporting longer decoding filters would
  require a richer MCU runtime bank.
- The current `urCCA` Rust path consumes precomputed full-length encoding
  matrices. Stimulus-to-event expansion remains on the host side.
