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
  [crates/cvep-decoder/scripts/export_pyntbci_etrca.py](/Users/peranpl1/Documents/repos/oss/dcmini-fw/crates/cvep-decoder/scripts/export_pyntbci_etrca.py)

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
  [crates/cvep-decoder/scripts/export_pyntbci_urcca.py](/Users/peranpl1/Documents/repos/oss/dcmini-fw/crates/cvep-decoder/scripts/export_pyntbci_urcca.py)

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
- [crates/cvep-decoder/scripts/export_pyntbci_etrca.py](/Users/peranpl1/Documents/repos/oss/dcmini-fw/crates/cvep-decoder/scripts/export_pyntbci_etrca.py)
- [crates/cvep-decoder/scripts/export_pyntbci_rcca.py](/Users/peranpl1/Documents/repos/oss/dcmini-fw/crates/cvep-decoder/scripts/export_pyntbci_rcca.py)
- [crates/cvep-decoder/scripts/export_pyntbci_urcca.py](/Users/peranpl1/Documents/repos/oss/dcmini-fw/crates/cvep-decoder/scripts/export_pyntbci_urcca.py)
