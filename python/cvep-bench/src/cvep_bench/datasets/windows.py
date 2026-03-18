from __future__ import annotations

import math

import numpy as np

from cvep_bench.benchmarks.load_planning import loader_trial_seconds_for_algorithm
from cvep_bench.evaluation.splits import fold_slices


def round_half_up(value: float) -> int:
    return int(math.floor(value + 0.5))


def seconds_to_samples(seconds: float, fs: int) -> int:
    return max(1, round_half_up(seconds * fs))


def decode_window_requests(
    full_trial_seconds: float,
    explicit: list[float] | None,
    step_seconds: float | None,
) -> list[float]:
    if explicit is not None:
        values = sorted({float(value) for value in explicit})
    elif step_seconds is not None:
        if step_seconds <= 0.0:
            raise ValueError(
                f"window_step_seconds must be positive, got {step_seconds}"
            )
        values = [
            round(idx * step_seconds, 6)
            for idx in range(1, int(math.floor(full_trial_seconds / step_seconds)) + 1)
        ]
    else:
        values = [full_trial_seconds]
    filtered = [value for value in values if 0.0 < value <= full_trial_seconds]
    if not filtered:
        raise ValueError("No valid window lengths remain after filtering")
    if explicit is not None:
        return sorted(filtered)
    if not any(
        math.isclose(value, full_trial_seconds, abs_tol=1e-9) for value in filtered
    ):
        filtered.append(full_trial_seconds)
    return sorted(filtered)


def stimulus_to_sample_rate(
    stimulus: np.ndarray, presentation_rate: int, fs: int
) -> np.ndarray:
    if presentation_rate <= 0:
        raise ValueError(f"presentation_rate must be positive, got {presentation_rate}")
    duration_seconds = stimulus.shape[1] / presentation_rate
    total_samples = seconds_to_samples(duration_seconds, fs)
    sample_positions = np.floor(
        np.arange(total_samples) * presentation_rate / fs
    ).astype(np.int64)
    sample_positions = np.clip(sample_positions, 0, stimulus.shape[1] - 1)
    return np.asarray(stimulus[:, sample_positions], dtype=np.float64)


def slice_windowed_trials_and_stimulus(
    x: np.ndarray,
    stimulus: np.ndarray,
    fs: int,
    presentation_rate: int,
    requested_window_seconds: float,
    drop_first_seconds: float,
) -> tuple[np.ndarray, np.ndarray, dict[str, float | int]]:
    nominal_window_samples = min(
        seconds_to_samples(requested_window_seconds, fs), x.shape[2]
    )
    nominal_window_seconds = nominal_window_samples / fs
    nominal_stimulus_samples = min(
        seconds_to_samples(requested_window_seconds, presentation_rate),
        stimulus.shape[1],
    )
    x_window = x[:, :, :nominal_window_samples]
    stimulus_window = stimulus[:, :nominal_stimulus_samples]
    trim_seconds = max(0.0, drop_first_seconds)
    if trim_seconds > 0.0:
        trim_samples = min(
            seconds_to_samples(trim_seconds, fs), max(0, nominal_window_samples - 1)
        )
        trim_stimulus_samples = min(
            seconds_to_samples(trim_seconds, presentation_rate),
            max(0, nominal_stimulus_samples - 1),
        )
    else:
        trim_samples = 0
        trim_stimulus_samples = 0
    x_effective = x_window[:, :, trim_samples:]
    stimulus_effective = stimulus_window[:, trim_stimulus_samples:]
    return (
        x_effective,
        stimulus_effective,
        {
            "nominal_window_samples": nominal_window_samples,
            "nominal_window_seconds": nominal_window_seconds,
            "effective_window_samples": x_effective.shape[2],
            "effective_window_seconds": x_effective.shape[2] / fs,
            "leading_trim_seconds": trim_seconds,
            "leading_trim_samples": trim_samples,
            "nominal_stimulus_samples": nominal_stimulus_samples,
            "effective_stimulus_samples": stimulus_effective.shape[1],
        },
    )
