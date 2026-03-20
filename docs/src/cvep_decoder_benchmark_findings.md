# CVEP Decoder Benchmark Findings

## Scope

This note captures the current benchmark findings from the unified
`cvep_bench` package, focusing on cross-family decoder comparisons, the current
understanding of zero-training preprocessing fidelity, and the first
continuous-state cumulative CCA prototype.

Unless otherwise noted, the results below come from:

- dataset: `Thielen2021`
- evaluation: chronological `5`-fold CV, aggregate reported over `30` subjects
- primary deployment profile: `matched_embedded_125`
- command family: `uv run <command> ...` from the workspace root

## What each benchmark measures

The current benchmark stack answers several different questions. The outputs are
easier to interpret if these are kept separate.

### `benchmark_pyntbci_vs_rust`

Measures:

- supervised projected-correlation decoders built from PyNTBCI exports
- currently used for `eTRCA` and `rCCA`
- answers: "what accuracy do we get from the supervised / exported bank path?"

Main outputs used here:

- `cross_family_projected_125.json`
- `etrca_sensitivity_*.json`

### `benchmark_cca_vs_rust`

Measures:

- zero-training fixed-window CCA
- both `instantaneous_cca` and `cumulative_cca`
- answers: "how good is zero-training CCA if we decode at fixed trial cutoffs?"

Main outputs used here:

- `cross_family_cca_125.json`
- `cca_zero_training_125_probe.json`
- `cca_zero_training_250_probe.json`
- `cca_fixed_baseline_s4_250.json`

### `benchmark_umm_vs_rust` and `benchmark_umm_variants`

Measure:

- UMM fixed-window performance
- UMM configuration sweeps over epoch, lag, layout, demeaning, and confidence
  settings
- answer: "does UMM become competitive under any plausible feature settings?"

Main outputs used here:

- `cross_family_umm_125.json`
- `umm_variant_probe_125_s4.json`
- `umm_tuned_probe_125.json`

### `compare_thielen2021_packaged_vs_raw`

Measures:

- waveform-level agreement between the packaged PyNTBCI `Thielen2021` tensors
  and the raw reconstruction path
- answers: "does our raw loader/preprocessing path still resemble the packaged
  reference?"

Main outputs used here:

- `preproc_packaged_vs_raw_125.json`
- `preproc_packaged_vs_raw_250.json`

### `compare_reference_vs_causal_preprocessing` and `benchmark_causal_preprocessing_vs_reference`

Measure:

- waveform-level and decoder-level differences between the current reference
  preprocessing path and a causal SOS-filter path
- answer: "how much accuracy or waveform shape changes when we force a more
  MCU-like causal preprocessing path?"

### `benchmark_continuous_state_cca`

Measures:

- a prototype of continuous-state zero-training CCA where the decoder scores
  short trailing windows while optionally retaining state across time and across
  previous emitted decisions
- answers: "can short fresh windows plus retained state do better than the
  fixed-window zero-training baselines?"

Main outputs used here:

- `continuous_state_cca_smoke_v2.json`
- `continuous_state_cca_s4_250_v3.json`

## Best results so far

These are the most useful headline results currently available.

### Best supervised short-window performance

- best `~1 s` result seen so far:
  - `matched_250_1.05` `eTRCA`: `0.7060`
  - `125 Hz` cross-family `rCCA`: `0.7050`

### Best zero-training fixed-window performance

- best zero-training result near `1 s`:
  - `cumulative_cca @ 250 Hz, 1.05 s`: `0.0767`
- best zero-training result at `2.1 s`:
  - `cumulative_cca @ 250 Hz, 2.1 s`: `0.4133`
- best zero-training result at `4.2 s`:
  - `cumulative_cca @ 125 Hz, 4.2 s`: `0.8333`
  - `cumulative_cca @ 250 Hz, 4.2 s`: `0.8433`

### Best continuous-state prototype result so far

On the first optimistic 2-subject smoke run:

- `hybrid_continuous_cumulative`
- `margin_threshold=0.1`
- `update_policy=confidence_gated`

gave roughly:

- accuracy: `0.3375`
- mean decision time: `1.84 s`

But after making the update policy more realistic
(`emitted_offset_only`, `min_consecutive_winners=2`), the best 4-subject result
became:

- `within_trial_accumulated`, `margin_threshold=0.05`
- accuracy: `0.1750`
- mean decision time: `3.14 s`

So the current prototype is not yet a compelling improvement over the simpler
fixed-window cumulative CCA baseline.

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
- The current fixed-window results still support cumulative CCA as the only
  serious zero-training candidate in this codebase.

Related outputs:

- `crates/cvep-decoder/data/cca_zero_training_125_probe.json`
- `crates/cvep-decoder/data/cca_zero_training_250_probe.json`

## Preprocessing and waveform fidelity

We compared packaged `Thielen2021` data against the raw reconstruction path
after preprocessing and resampling.

### Raw vs packaged agreement (`compare_thielen2021_packaged_vs_raw`)

- `250 Hz`: mean trial correlation about `0.95`
- `125 Hz`: mean trial correlation about `0.78`
- `240 Hz` parity checks previously showed near-perfect agreement (`~0.999`)

Key interpretation:

- the preprocessing/resampling path looks plausible,
- but `125 Hz` is visibly a less faithful approximation of the packaged signal
  than `250 Hz` or `240 Hz`,
- this likely hurts timing-sensitive zero-training methods more than it hurts
  the supervised projected baselines.

### Reference vs causal preprocessing (`compare_reference_vs_causal_preprocessing` / `benchmark_causal_preprocessing_vs_reference`)

On the current small probes, the causal path does not yet show dramatic decoder
differences from the reference path. For example, on a `Thielen2021` subject-1
probe at `125 Hz`, `eTRCA` and `rCCA` matched exactly on the tested windows.

This does **not** prove the causal path is universally equivalent. It only means
that our current causal/reference comparison scripts are working again and do
not yet show an obvious first-order discrepancy on the small probe conditions.

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

## Continuous-state CCA prototype status

We implemented a first continuous-state cumulative CCA prototype to answer the
embedded question:

- can we make short-window decisions while retaining synchronized state across
  the event stream and across earlier emitted decisions?

The current prototype supports:

- `stateless_instantaneous`
- `within_trial_accumulated`
- `cross_trial_cumulative`
- `hybrid_continuous_cumulative`

and stop rules:

- `fixed_dwell`
- `margin_threshold`

Current status:

- the prototype infrastructure works,
- but the current cross-trial adaptation policy is not yet delivering the hoped
  for benefit,
- and we do **not** yet see convincing evidence that longer use reliably
  improves accuracy.

Representative stricter prototype result (`4` subjects, `250 Hz`, `window=1.0 s`,
`update=0.25 s`, `max_dwell=4.2 s`, confidence-gated updates, emitted-offset-only,
minimum `2` consecutive winners):

| Mode | Stop rule | Accuracy | Mean decision time |
|---|---|---:|---:|
| `stateless_instantaneous` | `fixed_dwell` | 0.0500 | 4.20 s |
| `within_trial_accumulated` | `margin_threshold=0.05` | 0.1750 | 3.14 s |
| `cross_trial_cumulative` | `margin_threshold=0.10` | 0.0500 | 3.83 s |
| `hybrid_continuous_cumulative` | `margin_threshold=0.10` | 0.1250 | 2.32 s |

Comparison to fixed-window cumulative CCA on the same `4`-subject, `250 Hz`
condition:

- `cumulative_cca @ 1.05 s`: `0.1375`
- `cumulative_cca @ 2.1 s`: `0.5250`
- `cumulative_cca @ 4.2 s`: `0.9875`

Interpretation:

- the prototype does **not** yet beat the fixed-window cumulative baseline in a
  compelling way,
- the most useful gain so far appears to come from within-trial score
  accumulation,
- the current cross-trial state update rule is still too naive.

This means the continuous-state story remains promising as a research direction,
but it is not yet validated by the current implementation.

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
