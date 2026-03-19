# Continuous-State Zero-Training Prototype Plan

## Goal

Prototype the deployment story where the decoder:

- receives a continuous stream of stimulus-presentation events,
- keeps synchronized state across time,
- emits decisions from short fresh windows,
- but does **not** reset to a stateless decoder at each nominal trial boundary.

This is the most realistic way to evaluate cumulative zero-training CCA on the
embedded target.

## Why this experiment matters

The current offline fixed-window benchmarks answer:

- how well does the decoder classify after seeing the first `W` seconds of a
  trial?

They do **not** answer the embedded question:

- how well can the decoder classify after seeing `W` seconds of *new* EEG while
  retaining synchronized state built from the preceding event stream?

The distinction matters most for cumulative zero-training CCA.

## Current evidence from the package

The cleaned comparison tools suggest:

- packaged-vs-raw waveform parity is strong at `250 Hz` and weaker at `125 Hz`,
- zero-training CCA is now plausibly implemented,
- cumulative CCA is already strong by `2.1-4.2 s`,
- UMM still looks much weaker and is not the best candidate for the first
  continuous-state prototype.

So the recommended first prototype target is:

- **cumulative zero-training CCA**

## Prototype question

Can cumulative zero-training CCA do better than the current fixed-window
benchmark at short effective latencies if we let it:

- process overlapping windows,
- keep its running state between updates,
- and only emit when confidence crosses a threshold or after a fixed maximum
  dwell time?

## First implementation target

Add a new experiment module to `cvep_bench`:

- `python/cvep-bench/src/cvep_bench/benchmarks/continuous_state_cca.py`

and a CLI wrapper:

- `python/cvep-bench/src/cvep_bench/cli/benchmark_continuous_state_cca.py`

## Proposed experiment semantics

### Input assumptions

- Use `Thielen2021` first.
- Start with `250 Hz` as the fidelity-oriented reference condition.
- Then repeat at `125 Hz` as the deployment-constrained condition.
- Use the existing direct-window raw loader path.

### Core loop

For each subject and fold:

1. Load each full trial.
2. Maintain a running cumulative CCA decoder state across the trial.
3. Step through the trial in short increments, for example:
   - update interval: `0.25 s`
   - scoring window: `1.0 s`
4. At each step:
   - score the current trailing window,
   - update accumulated evidence,
   - optionally update cumulative state,
   - check a stopping rule.
5. Emit either:
   - earliest confident decision, or
   - forced decision at a max dwell time.

### Candidate stopping rules

Start simple:

1. **Fixed dwell only**
- no early stop
- score at `1.0, 1.25, 1.5, ...`

2. **Margin threshold**
- stop when best-minus-runner-up score exceeds threshold

3. **Score ratio threshold**
- stop when winner / runner-up exceeds threshold

4. **Consecutive agreement**
- stop when the winner is stable for `N` consecutive updates

The first version should implement at least:

- fixed dwell
- margin threshold

## State variants to compare

The prototype should compare at least these modes:

1. **Stateless instantaneous**
- current trailing window only
- no carryover state

2. **Within-trial accumulated scores**
- accumulate scores across overlapping windows
- no cross-trial decoder update

3. **Cross-trial cumulative CCA**
- existing cumulative update logic across trials
- but decisions emitted from short sliding windows

4. **Hybrid continuous cumulative**
- accumulate within trial
- and update decoder state across trials

This is the most realistic embedded mode.

## Metrics to record

For each subject/fold and condition:

- accuracy
- mean decision time
- median decision time
- fraction of trials that stop early
- decision-time histogram
- confidence-at-stop statistics
- comparison to fixed-window baseline at:
  - `1.05 s`
  - `2.1 s`
  - `4.2 s`

## Result schema

The output rows should include:

- `dataset`
- `subject`
- `fold_index`
- `target_fs`
- `update_seconds`
- `window_seconds`
- `max_dwell_seconds`
- `mode`
- `stop_rule`
- `stop_threshold`
- `decision_seconds`
- `stopped_early`
- `predicted_class`
- `true_class`
- `correct`

and grouped summaries should report:

- mean accuracy
- mean/median decision time
- early-stop rate

## Best place to build on existing code

Use these current modules as the foundation:

- `python/cvep-bench/src/cvep_bench/benchmarks/sliding_cca.py`
- `python/cvep-bench/src/cvep_bench/algorithms/cca_reference.py`
- `python/cvep-bench/src/cvep_bench/datasets/windowing.py`
- `python/cvep-bench/src/cvep_bench/benchmarks/reporting.py`
- `python/cvep-bench/src/cvep_bench/cli/arg_groups.py`

The first version should avoid Rust parity entirely.

## Recommended first experiment matrix

### Phase 1

- dataset: `Thielen2021`
- subjects: `1-8`
- fs: `250`
- update interval: `0.25 s`
- trailing window: `1.0 s`
- max dwell: `4.2 s`
- modes:
  - stateless instantaneous
  - within-trial accumulated
  - cumulative across trials
- stop rules:
  - fixed dwell
  - margin threshold

### Phase 2

Repeat Phase 1 at `125 Hz`.

### Phase 3

Scale promising settings to all subjects.

## Success criteria

This prototype is useful if it shows any of the following:

- cumulative CCA reaches meaningfully better than the fixed-window `1.05 s`
  baseline while still emitting close to `~1-2 s`
- score accumulation within a trial gives a useful latency/accuracy tradeoff
- `250 Hz` and `125 Hz` separate clearly enough to justify keeping both a
  research-fidelity and deployment-constrained benchmark track

## Recommended interpretation path

If the prototype works:

- cumulative zero-training CCA becomes a serious continuous-state decoder path

If it does not:

- the repository should treat zero-training CCA as a `2-4 s` method rather than
  a `~1 s` method,
- and keep `rCCA/eTRCA` as the practical short-latency decoders.
