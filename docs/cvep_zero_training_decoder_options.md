# CVEP Zero-Training Decoder Options

This note compares the main decoder families that came up in the linked
c-VEP / ERP papers, with an emphasis on whether they require supervised
calibration and whether they are realistic on a microcontroller.

It is intentionally deployment-oriented rather than mathematically complete.
The papers mostly discuss behavior and usability, not asymptotic cost, so the
complexity estimates below are engineering inferences from the algorithm
structure and from the current runtime design in this repository.

For a dataset-by-dataset readiness and evaluation-order recommendation, see
[docs/cvep_dataset_readiness_for_zero_training.md](/Users/peranpl1/Documents/repos/oss/dcmini-fw/docs/cvep_dataset_readiness_for_zero_training.md).

For a source-to-implementation correctness review of the current Rust modules,
see
[docs/cvep_source_implementation_review.md](/Users/peranpl1/Documents/repos/oss/dcmini-fw/docs/cvep_source_implementation_review.md).

For the concrete offline export and benchmark workflow now available for the
UMM path, see
[docs/cvep_offline_workflow.md](/Users/peranpl1/Documents/repos/oss/dcmini-fw/docs/cvep_offline_workflow.md).

## Scope

The papers point to these alternatives for the same decoding problem:

- `eTRCA` as the supervised baseline
- zero-training CCA / reconvolution CCA
- cumulative zero-training CCA
- UMM
- cumulative UMM

The current crate already contains:

- a projected-correlation runtime for exported `eTRCA` / `rCCA` banks in
  [crates/cvep-decoder/src/decoder.rs](/Users/peranpl1/Documents/repos/oss/dcmini-fw/crates/cvep-decoder/src/decoder.rs)
- an online adaptive `urCCA`-style runtime in
  [crates/cvep-decoder/src/urcca.rs](/Users/peranpl1/Documents/repos/oss/dcmini-fw/crates/cvep-decoder/src/urcca.rs)

## Notation

- `K`: number of classes / targets
- `C`: number of EEG channels
- `W`: samples in one decoding window
- `F`: stimulus-model feature count used by CCA
  - for a simple encoding model, `F` is often small
- `E`: number of stimulus-locked epochs in one trial
- `P`: feature count per epoch for UMM after any spatial or temporal projection
- `T`: number of previously processed trials retained in cumulative methods

Two different costs matter:

- host-side setup cost: anything computed before deployment or before a run
- on-device online cost: what the MCU must do per trial or per update

There is also a deployment detail that changes how the zero-training options
should be interpreted in this repository:

- the embedded decoder will run continuously,
- the stimulator will continuously emit exact stimulus-presentation events,
- the decoder does not need to behave like an offline script that starts from
  scratch for every isolated `1 s` trial.

That means the practical embedded question is not:

- "can a zero-training decoder learn everything it needs from only one fresh
  `1 s` window?"

It is:

- "can a zero-training decoder make a decision from roughly `1 s` of fresh EEG
  while carrying synchronized state accumulated from the ongoing event stream?"

That distinction strongly favors the cumulative methods.

## Continuous-Stream Interpretation

Because the stimulator provides exact event timing continuously, the embedded
runtime can retain several kinds of information even if the desired *decision
latency* is only around `1 s`:

- **within-window evidence:** the newest EEG chunk being scored right now
- **within-trial accumulation:** overlapping or repeated short windows inside the
  current fixation period
- **across-trial state:** running covariance / mean statistics updated from past
  decisions
- **stimulus synchronization:** exact knowledge of which code bit was on screen
  at each sample boundary

This does **not** automatically make `1 s` zero-training decoding easy. It does
mean the MCU can approach each `1 s` decision with a much better initialized
model than an offline one-shot benchmark would suggest.

In practice, the most useful consequence is:

- keep the *decision window* short,
- but let the decoder keep *state* over much longer periods.

That is the most important systems-level argument for cumulative CCA, and to a
lesser extent cumulative UMM.

## Summary Table

| Method            | Needs supervised calibration? | Uses known stimulus codebook?        | Online adaptation? | Typical MCU fit                                         |
| ----------------- | ----------------------------- | ------------------------------------ | ------------------ | ------------------------------------------------------- |
| `eTRCA`           | Yes                           | Not required                         | No                 | Excellent at inference, but not zero-training           |
| Instantaneous CCA | No                            | Yes                                  | No                 | Possible, but heavier math than projected correlation   |
| Cumulative CCA    | No initial calibration        | Yes                                  | Yes, self-updating | Plausible if `C` and `F` are small                      |
| Instantaneous UMM | No                            | Yes, for target/nontarget assignment | No                 | Plausible if epoch features are compact                 |
| Cumulative UMM    | No initial calibration        | Yes                                  | Yes, self-updating | Plausible, but state can grow if covariance is retained |

## 1. `eTRCA` Baseline

`eTRCA` is included here as the reference point, even though it is not
zero-training.

High-level idea:

- collect labeled calibration trials
- estimate class-specific spatial filters and response templates
- at runtime, project the current trial and correlate it with each class
  template

This repository's runtime path is exactly that projected-correlation form:

- bank storage in
  [crates/cvep-decoder/src/banks.rs](/Users/peranpl1/Documents/repos/oss/dcmini-fw/crates/cvep-decoder/src/banks.rs)
- scoring in
  [crates/cvep-decoder/src/decoder.rs](/Users/peranpl1/Documents/repos/oss/dcmini-fw/crates/cvep-decoder/src/decoder.rs)
- supervised fitting in
  [crates/cvep-decoder/python/cvep_bench/export/pyntbci_etrca.py](/Users/peranpl1/Documents/repos/oss/dcmini-fw/crates/cvep-decoder/python/cvep_bench/export/pyntbci_etrca.py)

Why it is not a zero-training option:

- the model depends on labeled calibration data to learn the spatial filters
  and templates
- removing that step means it is no longer `eTRCA` in the intended sense

Complexity:

- host-side training:
  - roughly linear in training data volume plus covariance / eigenvalue work
  - practical estimate: `O(N * C * W + K * C^3)` where `N` is training trials
- on-device inference per trial:
  - `O(K * C * W)`
- persistent MCU memory:
  - filters: `O(K * C)`
  - templates: `O(K * W)`
  - decoder ring buffer: `O(C * W)`

MCU take:

- this is the cheapest runtime of the group
- if you can tolerate a calibration session, it is the easiest option to ship

## 2. Instantaneous Zero-Training CCA

This is the main zero-training family described by Thielen et al. for c-VEP.
The 2021 paper frames the progression from full calibration to no calibration,
and the 2024 c-VEP paper compares zero-training CCA against UMM.

High-level idea:

- start from the known stimulation codebook
- derive class-specific stimulus encodings or predicted responses
- for one incoming trial, score each class by how well the EEG and the
  class-specific encoding agree under CCA
- make a decision without relying on any subject-specific training set

This is an "instantaneous" classifier:

- no prior labeled calibration trials
- no cumulative state from past online decisions

Why it solves the same problem as `eTRCA`:

- it still outputs one class decision per trial
- it uses the same stimulus protocol and codebook
- the difference is how the class model is obtained:
  - `eTRCA`: learned from labeled EEG
  - zero-training CCA: derived from the known stimulus sequence

Online complexity, naive full CCA form:

- build EEG statistics once per trial:
  - `O(C^2 * W)`
- per class, build cross-statistics against the class encoding:
  - `O(C * F * W + F^2 * W)`
- per class, solve or approximate the top canonical correlation:
  - practical bound: `O(C^3 + F^3)`
- full per-trial estimate:
  - `O(C^2 * W + K * (C * F * W + F^2 * W + C^3 + F^3))`

Persistent MCU memory:

- class encodings: `O(K * F * W)`
- scratch covariances: `O(C^2 + F^2 + C * F)`
- trial buffer: `O(C * W)`

MCU take:

- this is substantially heavier than projected-correlation `eTRCA`
- if `C` and `F` are both small and fixed, it is still manageable
- if you need a true no-calibration path, this is the cleanest conceptual
  alternative

## 3. Cumulative Zero-Training CCA

This keeps the zero-calibration start, but updates its internal statistics
after each predicted trial.

High-level idea:

- score the current trial against all class encodings
- choose the winning class
- treat that trial as pseudo-labeled and update running CCA state
- future decisions use the accumulated state

This is the closest match to the existing `UrCcaDecoder`:

- stateful online update in
  [crates/cvep-decoder/src/urcca.rs](/Users/peranpl1/Documents/repos/oss/dcmini-fw/crates/cvep-decoder/src/urcca.rs)
- host-side export of class encodings in
  [crates/cvep-decoder/python/cvep_bench/export/pyntbci_urcca.py](/Users/peranpl1/Documents/repos/oss/dcmini-fw/crates/cvep-decoder/python/cvep_bench/export/pyntbci_urcca.py)

Important distinction:

- this is still zero-training in the sense of no supervised calibration session
- it is not zero-learning; it adapts online from previous predictions

Online complexity:

- same class-scoring order as instantaneous CCA
- plus one winning-state update
- practical per-trial estimate:
  - `O(C^2 * W + K * (C * F * W + F^2 * W + C^3 + F^3))`
- asymptotically this is the same order as instantaneous CCA
- in practice it costs a bit more because state updates happen every trial

Persistent MCU memory:

- class encodings: `O(K * F * W)`
- running state:
  - `avg_x`: `O(C)`
  - `avg_y`: `O(F)`
  - `cov_x`: `O(C^2)`
  - `cov_y`: `O(F^2)`
  - `cov_xy`: `O(C * F)`
- total persistent state beyond the bank:
  - `O(C^2 + F^2 + C * F)`

MCU take:

- more stateful than instantaneous CCA, but not asymptotically worse
- often the best candidate if you want no calibration and can afford matrix
  state
- it benefits directly from a continuously synchronized event stream, because it
  can treat each new decision as another aligned update to a subject/session
  specific response model
- the main deployment risk is error reinforcement:
  - if early predictions are wrong, the model can drift in the wrong direction

## 4. Instantaneous UMM

UMM stands for unsupervised mean-difference maximization. The 2024 c-VEP paper
introduces it into the c-VEP setting as a zero-training alternative to CCA,
and the 2025 ERP paper evaluates the same family on ERP/P300 data.

High-level idea:

- split a trial into stimulus-locked epochs
- for each candidate class, use the known codebook to propose which epochs
  should behave like target epochs and which should behave like non-target
  epochs
- score the candidate by how strongly those two groups separate

This is a useful contrast with CCA:

- CCA matches EEG to a stimulus-driven encoding model
- UMM tries to maximize separation between hypothesized target and nontarget
  responses without supervised labels

Because the exact published implementation is not open in the paper pages I
could access, the cost model here is approximate and should be treated as an
engineering bound rather than a paper claim.

Online complexity, generic form:

- epoch extraction or feature projection:
  - `O(E * P)`
- per class, accumulate target / nontarget statistics:
  - `O(E * P)`
- if using only mean-separation scoring:
  - full per-trial cost `O(K * E * P)`
- if using covariance-aware or Mahalanobis-style scoring:
  - full per-trial cost rises toward `O(K * E * P^2 + P^3)`

Persistent MCU memory:

- if mean-only:
  - `O(P)` or `O(K * P)` depending on implementation
- if covariance-aware:
  - `O(P^2)` shared covariance, or worse if class-specific

MCU take:

- UMM may be cheaper than full CCA if you keep `P` small
- UMM may be worse than CCA if you retain dense covariance matrices in a large
  epoch feature space
- it is attractive when you already have an ERP-style epoching pipeline

## 5. Cumulative UMM

This is the self-updating version of UMM.

High-level idea:

- classify the current trial with the unsupervised mean-difference criterion
- use the predicted target assignment as pseudo-label information
- update the target / nontarget summary statistics

Like cumulative CCA, this remains zero-calibration but not zero-learning.

Online complexity:

- same scoring cost as instantaneous UMM
- plus summary-statistic update
- practical per-trial estimate:
  - mean-only form: `O(K * E * P)`
  - covariance-aware form: `O(K * E * P^2 + P^3)`

Persistent MCU memory:

- mean-only state:
  - `O(P)`
- covariance-aware state:
  - `O(P^2)`
- if multiple summary blocks are kept:
  - up to `O(T * P)` or `O(T * P^2)`, though an online implementation would
    typically compress this back to fixed-size running statistics

MCU take:

- likely more MCU-friendly than cumulative CCA if implemented with compact
  epoch features and simple running moments
- likely less MCU-friendly if it depends on dense covariance or Mahalanobis
  updates in a large feature space
- it also benefits from continuous event timing, especially because covariance
  can be accumulated continuously, but it still depends more heavily than CCA on
  the exact target / nontarget assignment logic and confidence model

## Complexity Comparison

The table below focuses on on-device online cost, because that is the limiting
factor for MCU deployment.

| Method            | Per-trial compute                                    | Persistent model/state on MCU                                       | Main cost driver                   |
| ----------------- | ---------------------------------------------------- | ------------------------------------------------------------------- | ---------------------------------- |
| `eTRCA` inference | `O(K * C * W)`                                       | `O(K * C + K * W + C * W)`                                          | class-template correlation         |
| Instantaneous CCA | `O(C^2 * W + K * (C * F * W + F^2 * W + C^3 + F^3))` | `O(K * F * W + C^2 + F^2 + C * F + C * W)`                          | covariance + CCA solve             |
| Cumulative CCA    | same order as instantaneous CCA                      | same as above, plus persistent running state `O(C^2 + F^2 + C * F)` | covariance update + CCA solve      |
| Instantaneous UMM | `O(K * E * P)` to `O(K * E * P^2 + P^3)`             | `O(P)` to `O(P^2)`                                                  | epoch feature dimension            |
| Cumulative UMM    | same order as instantaneous UMM                      | `O(P)` to `O(P^2)` fixed-state form                                 | running target/nontarget summaries |

## Deployment Implications

If the requirement is "no supervised calibration session", the decision is
mostly about whether you can afford full online model adaptation on the MCU.

For this repository's intended embedded mode, one further implication matters:

- **a `1 s` latency target does not imply a stateless `1 s` decoder.**

If the runtime is always receiving event timing from the stimulator, then the
real comparison is:

- `eTRCA` / `rCCA`: short-latency decisions from a supervised bank
- cumulative CCA / UMM: short-latency decisions from a continuously adapting,
  zero-calibration state machine

That makes cumulative CCA much more relevant than a naive reading of
"zero-training at `1 s`" would suggest.

### Best fit when MCU resources are tight

- `eTRCA` remains the cheapest runtime, but it is not zero-training
- among zero-training options, a compact UMM implementation may end up being
  cheaper than CCA if:
  - epoch features are aggressively reduced
  - covariance handling is simplified

### Best fit when zero-training matters most

- cumulative CCA is the most direct replacement for supervised template-based
  decoders in c-VEP
- it also aligns best with the direction already present in this repository via
  `urCCA`
- it is also the method that best exploits the planned always-on event stream,
  because it can continuously refine session-specific statistics while still
  emitting relatively short-window decisions

### Best fit when you want low implementation risk in this repo

- cumulative CCA / `urCCA`
- reason:
  - the repository already has the core runtime shape
  - the stimulus-driven encoding-bank workflow already exists
  - the remaining work is mainly benchmarking and update-policy hardening

### Highest-risk option for MCU deployment

- full dense-covariance CCA with large `C`, `F`, or both
- the cubic solve cost and quadratic state are the first things that become
  painful on a small microcontroller

## Practical Recommendation

If your decision criterion is "what is the most realistic zero-training
alternative to evaluate next on an MCU?", the order I would use is:

1. cumulative CCA / `urCCA`
2. instantaneous CCA
3. UMM, only if you are willing to build a new epoch-level runtime

Reason:

- cumulative CCA is already closest to the current codebase
- instantaneous CCA is the cleanest no-history ablation
- UMM is credible from the papers, but it is a more distinct implementation
  path and the public material available here was not detailed enough to pin
  down an exact MCU-oriented formulation

## Current Repository Findings

The current package benchmarks in `python/cvep-bench/` support a more concrete
deployment-oriented interpretation:

- `rCCA` and `eTRCA` are still the strongest performers at roughly `1 s`
- cumulative zero-training CCA becomes strong by roughly `2.1-4.2 s`
- UMM remains much weaker than CCA in the current implementation

Representative `Thielen2021` results at `125 Hz` from the current benchmark
stack are:

| Method | 1.05 s | 2.1 s | 4.2 s |
|---|---:|---:|---:|
| `rcca` | 0.7050 | 0.8800 | 0.9333 |
| `etrca` | 0.6867 | 0.8267 | 0.9017 |
| `instantaneous_cca` | 0.0983 | 0.3233 | 0.6117 |
| `cumulative_cca` | 0.1000 | 0.4550 | 0.8333 |
| `instantaneous_umm` | 0.1100 | 0.1617 | 0.1600 |
| `cumulative_umm` | 0.1000 | 0.1783 | 0.1883 |

The key deployment takeaway is:

- if you need the very best `~1 s` accuracy, the supervised projected baselines
  still win,
- if you need a true no-calibration path, cumulative CCA is the method that is
  currently both plausible in the literature and promising in the codebase,
- the most realistic target is not "instantaneous zero-training at `1 s` with no
  prior state", but rather "continuous zero-training adaptation that can emit a
  decision after about `1-4 s` of fresh evidence."

For a more detailed snapshot of the current benchmark outputs and preprocessing
diagnostics, see
[docs/cvep_decoder_benchmark_findings.md](/Users/peranpl1/Documents/repos/oss/dcmini-fw/docs/cvep_decoder_benchmark_findings.md).

For the concrete next prototype that evaluates short fresh windows with
continuous retained decoder state, see
[docs/cvep_continuous_state_prototype_plan.md](/Users/peranpl1/Documents/repos/oss/dcmini-fw/docs/cvep_continuous_state_prototype_plan.md).

## How To Prototype This With Current Data And Tooling

The current `cvep_bench` package is enough to prototype the continuous-state
story with the available offline datasets.

### 1. Establish the supervised ceiling

Use the projected benchmark path to measure `eTRCA` and `rCCA` at the target
latencies:

```bash
uv run --package cvep-bench benchmark_pyntbci_vs_rust \
  --profile matched_embedded_125 \
  --datasets Thielen2021 \
  --algorithms etrca rcca \
  --window-seconds-grid 1.05 2.1 4.2 5.25 10.5 31.5 \
  --skip-rust
```

This gives the practical upper bound if calibration is allowed.

### 2. Measure the zero-training fixed-window baseline

Use the dedicated zero-training CCA path:

```bash
uv run --package cvep-bench benchmark_cca_vs_rust \
  --profile matched_embedded_125 \
  --datasets Thielen2021 \
  --algorithms instantaneous_cca cumulative_cca \
  --window-seconds-grid 1.05 2.1 4.2 5.25 10.5 31.5 \
  --skip-rust
```

This answers the offline question, "how much does past pseudo-labeled state help
if each decision is still reported at fixed trial cutoffs?"

### 3. Probe within-trial evidence accumulation

Use the sliding-window CCA benchmark to simulate continuous scoring inside the
same fixation period:

```bash
uv run --package cvep-bench benchmark_cca_sliding_windows \
  --datasets Thielen2021 \
  --target-fs 125 \
  --window-seconds-grid 1.0 \
  --step-seconds 0.25
```

This does **not** reproduce the full embedded deployment story, but it is the
best current offline proxy for:

- repeated short-window inference,
- within-trial score accumulation,
- dynamic-stopping style behavior.

### 4. Probe long-lived zero-training state

The most deployment-relevant experiments right now are the cumulative ones:

- `benchmark_cca_vs_rust` with `cumulative_cca`
- `benchmark_umm_vs_rust` with `cumulative_umm`

These let us estimate how much the decoder improves once it has already seen a
stream of previous events and decisions.

### 5. Check preprocessing fidelity explicitly

Before trusting any zero-training result, use the waveform diagnostics:

```bash
uv run --package cvep-bench compare_thielen2021_packaged_vs_raw \
  --subject 1 \
  --target-fs 125 \
  --trialtime 4.2
```

and, if needed,

```bash
uv run --package cvep-bench compare_reference_vs_causal_preprocessing \
  --dataset Thielen2021 \
  --subject 1 \
  --target-fs 125 \
  --trial-seconds 4.2
```

This is especially important for zero-training methods because they are more
timing-sensitive than the projected baselines.

### 6. Recommended prototype order

Given the current code and results, the most useful prototype path is:

1. `rCCA` / `eTRCA` as the latency ceiling
2. cumulative zero-training CCA as the main no-calibration candidate
3. sliding-window CCA as a proxy for dynamic stopping
4. UMM only after the CCA path is well understood

That order matches both the literature and the current benchmark evidence in
this repository.

## Sources

Primary sources used:

- Thielen, Marsman, Farquhar, Desain (2021), "From full calibration to zero
  training for a code-modulated visual evoked potentials for brain-computer
  interface"
  - DOI: <https://doi.org/10.1088/1741-2552/abecef>
  - PubMed abstract: <https://pubmed.ncbi.nlm.nih.gov/33690182/>
- Thielen, Sosulski, Tangermann (2024), "Exploring new territory:
  Calibration-free decoding for c-VEP BCI"
  - DOI: <https://doi.org/10.3217/978-3-99161-014-4-057>
  - Lab publication page: <https://neurotechlab.socsci.ru.nl/publication/thi-sos-tan-24/>
- Thielen, Tangermann (2025), "Exploring new territory II: Calibration-free
  decoding for ERP BCI"
  - DOI: <https://doi.org/10.1109/SMC58881.2025.11342596>
  - SMC 2025 program abstract:
    <https://conf.papercept.net/conferences/conferences/SMC25/program/SMC25_ContentListWeb_3.html>
- Neurotechnology lab project page:
  - <https://neurotechlab.socsci.ru.nl/jobs/nt_decoding/>

Repository references used:

- [crates/cvep-decoder/src/decoder.rs](/Users/peranpl1/Documents/repos/oss/dcmini-fw/crates/cvep-decoder/src/decoder.rs)
- [crates/cvep-decoder/src/banks.rs](/Users/peranpl1/Documents/repos/oss/dcmini-fw/crates/cvep-decoder/src/banks.rs)
- [crates/cvep-decoder/src/urcca.rs](/Users/peranpl1/Documents/repos/oss/dcmini-fw/crates/cvep-decoder/src/urcca.rs)
- [crates/cvep-decoder/python/cvep_bench/export/pyntbci_etrca.py](/Users/peranpl1/Documents/repos/oss/dcmini-fw/crates/cvep-decoder/python/cvep_bench/export/pyntbci_etrca.py)
- [crates/cvep-decoder/python/cvep_bench/export/pyntbci_rcca.py](/Users/peranpl1/Documents/repos/oss/dcmini-fw/crates/cvep-decoder/python/cvep_bench/export/pyntbci_rcca.py)
- [crates/cvep-decoder/python/cvep_bench/export/pyntbci_urcca.py](/Users/peranpl1/Documents/repos/oss/dcmini-fw/crates/cvep-decoder/python/cvep_bench/export/pyntbci_urcca.py)
