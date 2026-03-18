from __future__ import annotations

import numpy as np

from cvep_bench.datasets.windows import (
    decode_window_requests,
    slice_windowed_trials_and_stimulus,
    stimulus_to_sample_rate,
)


def test_decode_window_requests_keeps_explicit_values_only() -> None:
    assert decode_window_requests(31.5, [4.2], None) == [4.2]


def test_stimulus_to_sample_rate_expands_frames() -> None:
    stimulus = np.asarray([[1.0, 0.0, 1.0]], dtype=np.float64)
    expanded = stimulus_to_sample_rate(stimulus, presentation_rate=60, fs=240)
    expected = np.asarray([[1.0] * 4 + [0.0] * 4 + [1.0] * 4], dtype=np.float64)
    np.testing.assert_array_equal(expanded, expected)


def test_slice_windowed_trials_applies_leading_trim() -> None:
    x = np.zeros((2, 3, 10), dtype=np.float64)
    stimulus = np.zeros((4, 10), dtype=np.float64)
    sliced_x, sliced_stimulus, info = slice_windowed_trials_and_stimulus(
        x,
        stimulus,
        fs=10,
        presentation_rate=10,
        requested_window_seconds=1.0,
        drop_first_seconds=0.2,
    )
    assert sliced_x.shape == (2, 3, 8)
    assert sliced_stimulus.shape == (4, 8)
    assert info["leading_trim_samples"] == 2
