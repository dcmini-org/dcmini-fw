# CVEP Decoder Benchmark Findings

## Scope

This note captures the current benchmark findings from the unified
`cvep_bench` package, focusing on cross-family decoder comparisons and the
current understanding of zero-training preprocessing fidelity.

Unless otherwise noted, the results below come from:

- dataset: `Thielen2021`
- evaluation: chronological `5`-fold CV, aggregate reported over `30` subjects
- primary deployment profile: `matched_embedded_125`
- command family: `uv run --package cvep-bench ...`

## Cross-family ranking at 125 Hz

The latest direct comparison across the major decoder families at `125 Hz`
produced the following mean accuracies:

| Algorithm | 1.05 s | 2.1 s | 4.2 s | 5.25 s | 10.5 s | 31.5 s |
|---|---:|---:|---:|---:|---:|---:|
| `rcca` | 0.7050 | 0.8800 | 0.9333 | 0.9433 | 0.9667 | 0.9550 |
| `etrca` | 0.6867 | 0.8267 | 0.9017 | 0.9167 | 0.9567 | 0.9583 |
| `cumulative_cca` | 0.1000 | 0.4550 | 0.8333 | 0.8650 | 0.9350 | 0.9150 |
| `instantaneous_cca` | 0.0983 | 0.3233 | 0.6117 | 0.6750 | 0.8117 | 0.8117 |
| `cumulative_umm` | 0.1000 | 0.1783 | 0.1883 | 0.1883 | 0.1883 | 0.1883 |
| `instantaneous_umm` | 0.1100 | 0.1617 | 0.1600 | 0.1600 | 0.1600 | 0.1600 |

Key interpretation:

- `rcca` is currently the strongest method in this benchmark stack.
- `etrca` remains very strong and is still the best low-latency baseline among
  the non-zero-training methods we care about operationally.
- Zero-training CCA is now clearly functional, but remains a `2.1-4.2 s+`
  story rather than a `~1 s` story.
- UMM remains far behind the other families in the current implementation.

Related outputs:

- `crates/cvep-decoder/data/cross_family_projected_125.json`
- `crates/cvep-decoder/data/cross_family_cca_125.json`
- `crates/cvep-decoder/data/cross_family_umm_125.json`

## eTRCA early-window sensitivity

We re-ran `eTRCA` under matched and legacy configurations to explain why some
historical results looked inconsistent.

| Config | Profile | fs | Requested | Actual | Mean accuracy |
|---|---|---:|---:|---:|---:|
| `legacy_250_1.0` | `legacy` | 250 | 1.000 | 1.000 | 0.5690 |
| `legacy_250_1.05` | `legacy` | 250 | 1.050 | 1.052 | 0.5823 |
| `matched_250_1.0` | `matched_embedded_125` | 250 | 1.000 | 1.000 | 0.6910 |
| `matched_250_1.05` | `matched_embedded_125` | 250 | 1.050 | 1.052 | 0.7060 |
| `legacy_125_1.05` | `legacy` | 125 | 1.050 | 1.048 | 0.4560 |
| `matched_125_1.05` | `matched_embedded_125` | 125 | 1.050 | 1.048 | 0.6707 |

Key interpretation:

- The largest effect is the preprocessing/profile change, not the small window
  definition change from `1.0` to `1.05 s`.
- Under the same matched profile, moving from `250 Hz` to `125 Hz` only causes
  a modest early-window drop for `eTRCA`.
- The old `legacy` profile is simply worse for early `eTRCA` than the newer
  matched embedded path.

Related outputs:

- `crates/cvep-decoder/data/etrca_sensitivity_legacy_250_1p0.json`
- `crates/cvep-decoder/data/etrca_sensitivity_legacy_250_1p05.json`
- `crates/cvep-decoder/data/etrca_sensitivity_matched_250_1p0.json`
- `crates/cvep-decoder/data/etrca_sensitivity_matched_250_1p05.json`
- `crates/cvep-decoder/data/etrca_sensitivity_legacy_125_1p05.json`
- `crates/cvep-decoder/data/etrca_sensitivity_matched_125_1p05.json`

## Zero-training CCA status

The major stimulus-rate/timing bugs in the zero-training CCA path were fixed.
Current interpretation:

- the implementation is now plausibly correct,
- cumulative zero-training CCA becomes strong by `4.2 s`,
- but short-window weakness near `1.05 s` still appears to be a real method
  limitation rather than a gross implementation bug.

Representative results:

### 125 Hz

- `instantaneous_cca`: `0.0983 / 0.3233 / 0.6117` at `1.05 / 2.1 / 4.2 s`
- `cumulative_cca`: `0.1000 / 0.4550 / 0.8333` at `1.05 / 2.1 / 4.2 s`

### 250 Hz

- `instantaneous_cca`: `0.0667 / 0.3133 / 0.6500`
- `cumulative_cca`: `0.0767 / 0.4133 / 0.8433`

Key interpretation:

- `250 Hz` is only modestly better than `125 Hz` in the current benchmark, but
  still has better raw-vs-packaged waveform agreement.
- The remaining CCA gap is mostly a short-window performance problem, not a
  parity/correctness problem.

Related outputs:

- `crates/cvep-decoder/data/cca_zero_training_125_probe.json`
- `crates/cvep-decoder/data/cca_zero_training_250_probe.json`

## Preprocessing and waveform fidelity

We compared packaged `Thielen2021` data against the raw reconstruction path
after preprocessing and resampling.

### Raw vs packaged agreement

- `250 Hz`: mean trial correlation about `0.95`
- `125 Hz`: mean trial correlation about `0.78`
- `240 Hz` parity checks previously showed near-perfect agreement (`~0.999`)

Key interpretation:

- the preprocessing/resampling path looks plausible,
- but `125 Hz` is visibly a less faithful approximation of the packaged signal
  than `250 Hz` or `240 Hz`,
- this likely hurts timing-sensitive zero-training methods more than it hurts
  the supervised projected baselines.

Related outputs:

- `crates/cvep-decoder/data/preproc_packaged_vs_raw_125.json`
- `crates/cvep-decoder/data/preproc_packaged_vs_raw_250.json`
- `crates/cvep-decoder/data/preproc_packaged_vs_raw_125.png`
- `crates/cvep-decoder/data/preproc_packaged_vs_raw_250.png`

## UMM status

UMM remains the least convincing family in the current benchmark stack.

Cross-family results at `125 Hz` stay low even for long windows:

- `instantaneous_umm`: roughly `0.11 / 0.16 / 0.16` at `1.05 / 2.1 / 4.2 s`
- `cumulative_umm`: roughly `0.10 / 0.18 / 0.19`

A reduced design sweep on a 4-subject subset found some configurations that look
better locally, especially with:

- `epoch_seconds=0.3`
- `lag_seconds=0.05`
- `epoch_demean=True`

But those gains did not hold up when re-run across all subjects in the current
benchmark path.

Current interpretation:

- UMM does not look broken due to one obvious preprocessing bug,
- but it still does not look competitive enough in the present implementation,
- so the remaining gap is more likely due to algorithm/configuration mismatch
  than to a simple data-loading issue.

Related outputs:

- `crates/cvep-decoder/data/umm_variant_probe_125_s4.json`
- `crates/cvep-decoder/data/umm_tuned_probe_125.json`

## Current practical recommendation

- For best performance at current fidelity: prioritize `rcca`, then `etrca`.
- For zero-training operation: prioritize cumulative zero-training CCA.
- For early low-latency windows (`~1 s`): do not expect zero-training CCA or UMM
  to match the supervised projected baselines.
- Treat `125 Hz` as the deployment-constrained benchmark and `250 Hz` as the
  higher-fidelity reference path when investigating zero-training methods.
