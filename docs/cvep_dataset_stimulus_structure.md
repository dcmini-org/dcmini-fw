# CVEP Dataset Stimulus Structure

This note summarizes the stimulus structure of the c-VEP datasets currently
used by `cvep_bench`, with emphasis on the timing and code semantics that matter
for zero-training decoders.

## Why this matters

All of the zero-training methods in this repository rely on the known stimulus
sequence.

The most important stimulus properties are:

- presentation rate
- number of classes / targets
- code length
- cycle duration
- trial duration
- whether the codebook is recovered directly from source files or provided in a
  packaged form

These details determine how much evidence exists in a `1 s` window, how cleanly
the EEG can be aligned to the stimulus stream, and how realistic a continuous
event-driven decoder is.

## Common setup across datasets

The supported datasets are code-modulated VEP datasets:

- each class is associated with a binary stimulus code,
- the decoder knows the codebook,
- the EEG is interpreted relative to the timing of these code bits.

That means the decoder is not just looking for a generic evoked response. It is
looking for a response that is phase-locked to a known binary stimulation
pattern.

## Thielen2015

Current loader reference:

- `python/cvep-bench/src/cvep_bench/datasets/loaders.py`

Stimulus properties:

- presentation rate: `120 Hz`
- subjects: `12`
- runs per subject: `3`
- benchmark trial duration: `4.2 s`
- classes: recovered from the subject-specific selected code subset/layout

The benchmark loader reconstructs one base code cycle and repeats it four times
per trial. In practice:

- one base cycle is `1.05 s`
- `1.05 s x 4 = 4.2 s`

Implication:

- this is a short-trial repeated-cycle c-VEP setup,
- useful for studying fast decoding with repeated code structure inside one
  trial.

## Thielen2021

Current loader reference:

- `python/cvep-bench/src/cvep_bench/datasets/loaders.py`

Stimulus properties:

- presentation rate: `60 Hz`
- subjects: `30`
- offline blocks per subject: `5`
- classes: `20`
- codebook source: `mgold_61_6521_flip_balanced_20.mat`

The key timing values are:

- code length: `126` stimulus frames
- cycle duration: `126 / 60 = 2.1 s`
- full offline trial duration: `31.5 s`
- full offline trial cycles: `15`

This makes the common evaluation windows especially meaningful:

- `1.05 s` = half a code cycle
- `2.1 s` = one full code cycle
- `4.2 s` = two full code cycles
- `5.25 s` = two and a half cycles
- `10.5 s` = five cycles
- `31.5 s` = fifteen cycles

Implications:

- a `~1 s` decoder on `Thielen2021` is trying to classify from only half a code
  cycle,
- that is one major reason zero-training methods are much weaker there than at
  longer windows,
- continuous synchronized decoding is attractive here because the system can
  remain phase-locked to the `2.1 s` code cycle across time.

### Raw vs packaged forms

`Thielen2021` exists in two forms inside our tooling:

- **raw reconstruction** from the source files
- **packaged** form from PyNTBCI (`.npz`)

The raw loader stores the canonical one-cycle `20 x 126` codebook at `60 Hz`
and then derives the fs-aligned stimulus representation used by the decoder.

This is important because previous zero-training CCA bugs came from mixing the
stimulus frame rate and the EEG sample rate incorrectly.

## Castillos datasets

Current loader reference:

- `python/cvep-bench/src/cvep_bench/datasets/loaders.py`

Supported paradigms:

- `CastillosBurstVEP40`
- `CastillosBurstVEP100`
- `CastillosCVEP40`
- `CastillosCVEP100`

Stimulus properties:

- presentation rate: `60 Hz`
- subjects: `12`
- classes: `4`
- benchmark trial duration: `2.2 s`

The codebook is reconstructed from the event annotations in the EEGLAB files.

Implications:

- these are shorter and smaller-class-count than `Thielen2021`,
- they are useful for method prototyping,
- but they are less demanding than the 20-class long-trial `Thielen2021` case.

## What matters most for our current research questions

For the current zero-training and continuous-state work, the most important
dataset is `Thielen2021` because it combines:

- `20` classes,
- long trials,
- a clearly defined `60 Hz` code stream,
- a known `2.1 s` code cycle,
- and a widely used packaged reference implementation.

This is the best dataset in the repo for testing whether a decoder can:

- stay synchronized to an ongoing stimulus stream,
- accumulate useful state over time,
- and eventually emit useful short-latency decisions without calibration.

## Deployment interpretation

The embedded deployment story should be understood in terms of stimulus timing,
not only trial length.

Because the embedded device will continuously receive the presentation events,
it can:

- stay aligned to the code stream all the time,
- score short fresh windows,
- but retain state over much longer periods.

That is why the most relevant question is not simply:

- "how good is a stateless `1 s` decoder?"

but rather:

- "how good is a synchronized decoder that emits after `~1 s` of fresh evidence
  while retaining state from the ongoing event stream?"

For that reason, the stimulus structure of `Thielen2021` strongly motivates the
continuous-state cumulative CCA prototype.
