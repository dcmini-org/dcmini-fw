from __future__ import annotations

from dataclasses import dataclass

import numpy as np

from cvep_bench.datasets.windows import (
    decode_window_requests,
    round_half_up,
    seconds_to_samples,
    slice_windowed_trials_and_stimulus,
    stimulus_to_sample_rate,
)


@dataclass(frozen=True)
class WindowSlice:
    start_sample: int
    end_sample: int
    window_samples: int
    window_seconds: float


def window_seconds_to_samples(
    requested_seconds: float,
    fs: int,
    trial_samples: int,
    *,
    clamp: bool = True,
) -> int:
    samples = seconds_to_samples(requested_seconds, fs)
    return min(samples, trial_samples) if clamp else samples


def sliding_window_starts(
    trial_samples: int,
    window_samples: int,
    step_samples: int,
) -> np.ndarray:
    if window_samples > trial_samples:
        return np.asarray([], dtype=np.int64)
    last_start = trial_samples - window_samples
    starts = np.arange(0, last_start + 1, step_samples, dtype=np.int64)
    if starts.size == 0 or starts[-1] != last_start:
        starts = np.concatenate((starts, np.asarray([last_start], dtype=np.int64)))
    return starts


def iter_sliding_windows(
    trial_samples: int,
    fs: int,
    requested_windows: list[float],
    step_seconds: float,
) -> list[WindowSlice]:
    out: list[WindowSlice] = []
    step_samples = seconds_to_samples(step_seconds, fs)
    for requested_seconds in requested_windows:
        window_samples = window_seconds_to_samples(requested_seconds, fs, trial_samples)
        for start in sliding_window_starts(trial_samples, window_samples, step_samples):
            out.append(
                WindowSlice(
                    start_sample=int(start),
                    end_sample=int(start + window_samples),
                    window_samples=int(window_samples),
                    window_seconds=window_samples / fs,
                )
            )
    return out
