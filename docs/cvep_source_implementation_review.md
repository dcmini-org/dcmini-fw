# CVEP Source Implementation Review

This note compares the current `cvep-decoder` implementations against the
accessible primary-source descriptions for the zero-training decoder families
discussed in:

- Thielen et al. (2021), JNE: <https://doi.org/10.1088/1741-2552/abecef>
- Thielen, Sosulski, Tangermann (2024), Graz BCI:
  <https://doi.org/10.3217/978-3-99161-014-4-057>
- Thielen, Tangermann (2025), IEEE SMC:
  <https://doi.org/10.1109/SMC58881.2025.11342596>

The goal here is narrower than a full literature summary: determine whether the
new Rust modules are source-faithful enough to be called implementations of the
same algorithms, or whether they should be treated as approximations.

## Bottom Line

| Module | Status vs source | Notes |
| --- | --- | --- |
| [crates/cvep-decoder/src/instantaneous_cca.rs](/Users/peranpl1/Documents/repos/oss/dcmini-fw/crates/cvep-decoder/src/instantaneous_cca.rs) | Broadly aligned | Stateless zero-training CCA over known encodings. |
| [crates/cvep-decoder/src/cumulative_cca.rs](/Users/peranpl1/Documents/repos/oss/dcmini-fw/crates/cvep-decoder/src/cumulative_cca.rs) | Broadly aligned | Matches the `urCCA` update pattern used in the PyntBCI reference code. |
| [crates/cvep-decoder/src/instantaneous_umm.rs](/Users/peranpl1/Documents/repos/oss/dcmini-fw/crates/cvep-decoder/src/instantaneous_umm.rs) | More closely aligned | Captures the mean-difference / Mahalanobis scoring idea and supports tapered block-Toeplitz covariance when the feature layout is known. |
| [crates/cvep-decoder/src/cumulative_umm.rs](/Users/peranpl1/Documents/repos/oss/dcmini-fw/crates/cvep-decoder/src/cumulative_umm.rs) | More closely aligned | Includes confidence-weighted cumulative updates and tapered block-Toeplitz covariance scoring when the feature layout is known. |

## Sources Used

Primary-source material that was directly reachable:

- 2021 paper abstract via PubMed:
  <https://pubmed.ncbi.nlm.nih.gov/33690182/>
- 2024 lab publication page:
  <https://neurotechlab.socsci.ru.nl/publication/thi-sos-tan-24/>
- 2024 lab poster page:
  <https://neurotechlab.socsci.ru.nl/poster/thi-sos-tan-24-b/>
- 2025 IEEE SMC program abstract:
  <https://conf.papercept.net/conferences/conferences/SMC25/program/SMC25_ContentListWeb_3.html>
- UMM publication page:
  <https://neurotechlab.socsci.ru.nl/publication/sosul-et-al-23-a/>
- Block-Toeplitz covariance publication page:
  <https://neurotechlab.socsci.ru.nl/publication/sosul-et-al-22-b/>
- PyntBCI reference implementation:
  <https://github.com/thijor/pyntbci/blob/main/pyntbci/classifiers.py>

The 2021 and 2025 full texts were not directly accessible from the crawler, so
the comparison for those relies on the accessible abstracts plus the 2024
poster / publication material that explicitly names the zero-training variants.

## Findings

### 1. `InstantaneousCcaDecoder` is consistent with the zero-training CCA family

The accessible 2024/2025 material describes an instantaneous calibration-free
CCA variant that decodes each trial independently from the known stimulus
encoding. The Rust module does that:

- it uses a known encoding bank per class in
  [crates/cvep-decoder/src/instantaneous_cca.rs#L19](/Users/peranpl1/Documents/repos/oss/dcmini-fw/crates/cvep-decoder/src/instantaneous_cca.rs#L19)
- it computes trial EEG covariance once per trial in
  [crates/cvep-decoder/src/instantaneous_cca.rs#L46](/Users/peranpl1/Documents/repos/oss/dcmini-fw/crates/cvep-decoder/src/instantaneous_cca.rs#L46)
- it scores each class by top canonical correlation against the class encoding in
  [crates/cvep-decoder/src/instantaneous_cca.rs#L65](/Users/peranpl1/Documents/repos/oss/dcmini-fw/crates/cvep-decoder/src/instantaneous_cca.rs#L65)

I do not see a source-level mismatch here from the accessible material. This is
best read as the stateless zero-training CCA baseline.

### 2. `CumulativeCcaDecoder` matches the `urCCA` update pattern closely

The 2024 poster describes a cumulative CCA variant that updates from previously
classified trials. The reference PyntBCI `urCCA` code does this by fitting a
CCA for each class against the current trial, predicting the winner, and then
copying the winning CCA state forward to all classes.

Reference code:

- `urCCA.fit/predict/update` in PyntBCI:
  <https://github.com/thijor/pyntbci/blob/main/pyntbci/classifiers.py>

Rust behavior:

- score all classes from the shared running state in
  [crates/cvep-decoder/src/cumulative_cca.rs#L130](/Users/peranpl1/Documents/repos/oss/dcmini-fw/crates/cvep-decoder/src/cumulative_cca.rs#L130)
- update persistent running state only with the winning class in
  [crates/cvep-decoder/src/cumulative_cca.rs#L195](/Users/peranpl1/Documents/repos/oss/dcmini-fw/crates/cvep-decoder/src/cumulative_cca.rs#L195)

Because all class hypotheses are scored from the same pre-update shared state,
this is operationally very close to the PyntBCI `urCCA` pattern. I do not see a
clear correctness issue here.

### 3. `InstantaneousUmmDecoder` now supports tapered block-Toeplitz covariance

The accessible UMM material describes UMM as a mean-difference decoder with a
structured covariance estimate. The 2024 poster explicitly refers to
`UMM_t11`, where the suffix indicates use of a tapered block-Toeplitz covariance
matrix. The Rust implementation now uses:

- empirical covariance over epochs in
  [crates/cvep-decoder/src/instantaneous_umm.rs#L138](/Users/peranpl1/Documents/repos/oss/dcmini-fw/crates/cvep-decoder/src/instantaneous_umm.rs#L138)
- optional tapered block-Toeplitz projection, using channel-prime feature order,
  in [crates/cvep-decoder/src/instantaneous_umm.rs#L313](/Users/peranpl1/Documents/repos/oss/dcmini-fw/crates/cvep-decoder/src/instantaneous_umm.rs#L313)
- simple diagonal regularization in
  [crates/cvep-decoder/src/instantaneous_umm.rs#L381](/Users/peranpl1/Documents/repos/oss/dcmini-fw/crates/cvep-decoder/src/instantaneous_umm.rs#L381)
- Mahalanobis scoring of the target-minus-nontarget mean difference in
  [crates/cvep-decoder/src/instantaneous_umm.rs#L222](/Users/peranpl1/Documents/repos/oss/dcmini-fw/crates/cvep-decoder/src/instantaneous_umm.rs#L222)

This is substantially closer to the published `UMM_t11` variant than the
previous dense-covariance implementation. The remaining uncertainty is whether
the exact covariance preprocessing and feature layout match the authors'
reference pipeline in all details.

### 4. `CumulativeUmmDecoder` now includes confidence weighting and tapered block-Toeplitz covariance

The 2024 poster describes cumulative UMM as using:

- covariance from previous trials
- target and nontarget means weighted by classification confidence

The 2023 confidence paper abstract also states that confidence is derived from
the score comparison between the winning class and the runner-up class.

The current Rust implementation now:

- scores using covariance from prior accepted trials when enough weighted state
  exists, otherwise falling back to the current trial covariance
- optionally projects that covariance onto the tapered block-Toeplitz family
  before scoring
- combines target and nontarget means with a confidence-derived weight in
  [crates/cvep-decoder/src/cumulative_umm.rs#L164](/Users/peranpl1/Documents/repos/oss/dcmini-fw/crates/cvep-decoder/src/cumulative_umm.rs#L164)
- updates persistent state with confidence-weighted running summaries in
  [crates/cvep-decoder/src/cumulative_umm.rs#L209](/Users/peranpl1/Documents/repos/oss/dcmini-fw/crates/cvep-decoder/src/cumulative_umm.rs#L209)
- derives confidence from the winner-vs-runner-up score gap in
  [crates/cvep-decoder/src/cumulative_umm.rs#L268](/Users/peranpl1/Documents/repos/oss/dcmini-fw/crates/cvep-decoder/src/cumulative_umm.rs#L268)

This closes the largest gaps from the accessible source material. The remaining
uncertainty is narrower: the confidence function is plausible and source-aligned
in spirit, but I have not yet verified that it exactly matches the authors'
reference implementation.

To avoid hiding that uncertainty in the API, the current Rust code now makes
both dimensions explicit:

- the block-Toeplitz covariance path takes an explicit feature layout via
  [crates/cvep-decoder/src/instantaneous_umm.rs#L6](/Users/peranpl1/Documents/repos/oss/dcmini-fw/crates/cvep-decoder/src/instantaneous_umm.rs#L6)
- the cumulative UMM path takes an explicit confidence model via
  [crates/cvep-decoder/src/cumulative_umm.rs#L8](/Users/peranpl1/Documents/repos/oss/dcmini-fw/crates/cvep-decoder/src/cumulative_umm.rs#L8)

## What I Trust Today

Safe to treat as reasonably correct implementations of the intended family:

- instantaneous zero-training CCA
- cumulative zero-training CCA / `urCCA`-style decoding

Still not safe to present as paper-exact yet:

- instantaneous UMM, unless the chosen feature layout matches the deployed
  feature pipeline
- cumulative UMM, unless one of the exposed confidence models is verified
  against the authors' implementation

## Recommended Next Step

If we want to make the UMM path source-faithful enough for a serious benchmark,
the next work should be:

1. verify whether the current winner-vs-runner-up confidence rule matches the
   2023/2024 UMM references closely enough
2. verify that the feature vectors supplied to the UMM decoders are indeed in
   the chosen `channels x time` layout
3. only then compare UMM against CCA as peer algorithms

Until then, the CCA modules are suitable for benchmarking as source-aligned
zero-training decoders, while the UMM modules should be treated as exploratory
approximations.
