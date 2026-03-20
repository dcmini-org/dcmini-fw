# CVEP Dataset Readiness For Zero-Training Decoders

This note answers two practical questions:

1. Do the downloaded datasets contain enough information to implement and test
   the candidate decoder families?
2. Which datasets should be used first, in what order, when evaluating
   zero-training alternatives on this repository?

It is intended to complement `cvep_zero_training_decoder_options.md`.

## Short Answer

Yes, the downloaded datasets are sufficient to implement and benchmark:

- supervised `eTRCA`
- instantaneous zero-training CCA
- cumulative zero-training CCA
- likely UMM
- likely cumulative UMM

The strongest first-choice datasets are:

1. `Thielen2021`
2. `Thielen2015`
3. `CastillosCVEP40`
4. `CastillosCVEP100`

The burst variants should not be first-line validation targets for
stimulus-driven zero-training methods because their event encoding is less
trustworthy from the downloaded files alone.

## What Each Dataset Provides

### Thielen2015

Relevant local files:

- `crates/cvep-decoder/data/MNE-thielen2015-data/dcc/DSC_2018.00047_553_v3/sourcedata/sub-01/test_sync_1/sub-01_test_sync_1.gdf`
- `crates/cvep-decoder/data/MNE-thielen2015-data/dcc/DSC_2018.00047_553_v3/sourcedata/sub-01/test_sync_1/sub-01_test_sync_1.mat`

What is available:

- continuous EEG
- event timing from the GDF status channel
- ground-truth labels from the `.mat` sidecar
- explicit stimulus codes from the `.mat` sidecar
- target subset and layout information

What this supports:

- supervised methods like `eTRCA`
- stimulus-driven zero-training CCA
- cumulative zero-training CCA
- likely UMM variants

Assessment:

- high readiness
- strongest legacy c-VEP dataset in the download for exact stimulus recovery

### Thielen2021

Relevant local files:

- `crates/cvep-decoder/data/MNE-thielen2021-data/dcc/DSC_2018.00122_448_v3/sourcedata/offline/sub-01/block_1/sub-01_20181128_block_1_main_eeg.gdf`
- `crates/cvep-decoder/data/MNE-thielen2021-data/dcc/DSC_2018.00122_448_v3/sourcedata/offline/sub-01/block_1/trainlabels.mat`
- `crates/cvep-decoder/data/MNE-thielen2021-data/dcc/DSC_2018.00122_448_v3/resources/mgold_61_6521_flip_balanced_20.mat`

What is available:

- continuous EEG
- dense event markers in the GDF
- explicit per-block labels
- explicit shared codebook for the 20 targets
- stable trial structure already used by the repo loaders

What this supports:

- supervised methods like `eTRCA`
- stimulus-driven zero-training CCA
- cumulative zero-training CCA
- likely UMM variants

Assessment:

- highest readiness
- best fit for first implementation and first benchmark work
- also best aligned with the repo's current scripts and documentation

### CastillosCVEP40 / CastillosCVEP100

Relevant local files:

- `crates/cvep-decoder/data/MNE-4class-vep-data/records/8255618/files/4Class-CVEP/P1/P1_mseq40.set`
- `crates/cvep-decoder/data/MNE-4class-vep-data/records/8255618/files/4Class-CVEP/P1/P1_mseq100.set`

What is available:

- EEG recordings in EEGLAB format
- per-trial annotations
- class identity embedded in annotation strings
- recoverable binary codewords from the m-sequence annotations

What this supports:

- supervised methods like `eTRCA`
- zero-training CCA, as long as the annotation-derived codebook is accepted as
  the stimulus model
- cumulative zero-training CCA
- likely UMM variants

Assessment:

- medium-high readiness
- suitable after the Thielen datasets
- lower priority than Thielen because the stimulation structure is less cleanly
  packaged and the dataset is only 4-class

### CastillosBurstVEP40 / CastillosBurstVEP100

Relevant local files:

- `crates/cvep-decoder/data/MNE-4class-vep-data/records/8255618/files/4Class-CVEP/P1/P1_burst40.set`
- `crates/cvep-decoder/data/MNE-4class-vep-data/records/8255618/files/4Class-CVEP/P1/P1_burst100.set`

What is available:

- EEG recordings in EEGLAB format
- per-trial annotations
- class labels recoverable from annotations

What is problematic:

- the annotation strings use tokenized burst patterns such as `0`, `1`, `2`,
  `20`, `21`
- these are not as directly self-describing as the Thielen codebooks or the
  Castillos m-sequence binary annotations
- the current repo loader collapses these into binary strings, but that does
  not cleanly establish the original stimulus timing semantics

What this supports safely:

- supervised methods like `eTRCA`

What this supports with caution:

- zero-training CCA variants
- UMM variants

Assessment:

- medium readiness for supervised benchmarking
- low-medium readiness for exact stimulus-driven zero-training reproduction
- not recommended as the first target for new zero-training decoder work

## Readiness Matrix

| Dataset | `eTRCA` | Instantaneous CCA | Cumulative CCA | UMM | Cumulative UMM | Confidence |
|---|---|---|---|---|---|---|
| `Thielen2021` | Yes | Yes | Yes | Probably | Probably | High |
| `Thielen2015` | Yes | Yes | Yes | Probably | Probably | High |
| `CastillosCVEP40` | Yes | Yes | Yes | Probably | Probably | Medium-high |
| `CastillosCVEP100` | Yes | Yes | Yes | Probably | Probably | Medium-high |
| `CastillosBurstVEP40` | Yes | Caution | Caution | Caution | Caution | Low-medium |
| `CastillosBurstVEP100` | Yes | Caution | Caution | Caution | Caution | Low-medium |

Meaning of the confidence column:

- `High`: the downloaded files contain explicit or reliably reconstructable
  stimulus structure and labels
- `Medium-high`: the files are usable, but the stimulus representation is less
  canonical or less cleanly packaged
- `Low-medium`: the files are usable for supervised methods, but the exact
  stimulus semantics are not trustworthy enough for first-line zero-training
  validation

## Best-Fit Evaluation Order

If the goal is to evaluate zero-training alternatives for eventual MCU
deployment, use this order.

### 1. Thielen2021

Why first:

- clearest stimulus/codebook support
- already central to the repo's CVEP workflow
- multi-target setting with explicit labels and codebook
- best fit for both CCA-family methods and any UMM variant

What to try here first:

1. instantaneous zero-training CCA
2. cumulative zero-training CCA
3. UMM
4. cumulative UMM

### 2. Thielen2015

Why second:

- also high-quality and explicit in terms of codes and labels
- good cross-check that the method is not overfit to one dataset format
- useful because it has different channel count and target structure

What to use it for:

- cross-dataset validation of the same implementation choices made on
  `Thielen2021`
- stress test for complexity and memory scaling because the channel count is
  larger

### 3. CastillosCVEP40

Why third:

- smaller 4-class problem
- m-sequence annotations are directly recoverable
- useful as a simpler external dataset after the Thielen family

Why before `CastillosCVEP100`:

- the repo currently treats both similarly from the recovered codebook point of
  view
- starting with one m-sequence variant is enough to validate pipeline
  portability before duplicating effort

### 4. CastillosCVEP100

Why fourth:

- same general advantages as `CastillosCVEP40`
- useful for checking robustness across recording variants in the same dataset

### 5. CastillosBurstVEP40 / CastillosBurstVEP100

Why last:

- least certain stimulus reconstruction
- highest risk of spending time debugging dataset semantics rather than decoder
  behavior
- acceptable only after the method is already working on better-supported data

## Recommended First Test Matrix

If you want a minimal first pass with high information value:

1. implement the algorithm on `Thielen2021`
2. validate the same implementation on `Thielen2015`
3. port the unchanged implementation to `CastillosCVEP40`

That gives:

- one primary benchmark dataset
- one strong cross-check dataset
- one external portability check

If all three work, then move on to:

4. `CastillosCVEP100`
5. the burst variants

## Practical Recommendation By Algorithm

### Instantaneous Zero-Training CCA

Best first dataset:

- `Thielen2021`

Second dataset:

- `Thielen2015`

Reason:

- these datasets provide the cleanest explicit codebooks and labels

### Cumulative Zero-Training CCA

Best first dataset:

- `Thielen2021`

Second dataset:

- `Thielen2015`

Third dataset:

- `CastillosCVEP40`

Reason:

- cumulative methods are sensitive to early error propagation
- you want the clearest available stimulus model before testing online update
  behavior

### UMM / Cumulative UMM

Best first dataset:

- `Thielen2021`

Second dataset:

- `CastillosCVEP40`

Third dataset:

- `Thielen2015`

Reason:

- UMM depends more on trial segmentation and target/nontarget assignment logic
  than on an explicit long codebook alone
- `Thielen2021` still gives the cleanest overall structure
- the smaller 4-class Castillos m-sequence data may be a useful simplification
  before scaling to the more complex Thielen2015 setup

## Bottom Line

The repository has enough downloaded data to implement and benchmark all of the
decoder families you are considering, with this practical interpretation:

- `eTRCA`: fully supported
- zero-training CCA: fully supported on the Thielen datasets and likely on the
  Castillos m-sequence datasets
- UMM: likely supported on the Thielen datasets and Castillos m-sequence
  datasets, assuming a reasonable implementation choice

For first evaluation work, prefer:

1. `Thielen2021`
2. `Thielen2015`
3. `CastillosCVEP40`
4. `CastillosCVEP100`
5. `CastillosBurstVEP40`
6. `CastillosBurstVEP100`

## Repository References

- `python/cvep-bench/src/cvep_bench/benchmarks/pyntbci_vs_rust.py`
- `crates/cvep-decoder/data/download_manifest.json`
- `cvep_zero_training_decoder_options.md`
