from __future__ import annotations

import numpy as np

from cvep_bench.algorithms.cca_reference import build_cca_encodings
from cvep_bench.datasets.windows import stimulus_to_sample_rate


def test_stimulus_expansion_matches_expected_240hz_frame_repeats() -> None:
    stimulus = np.asarray([[1.0, 0.0, 1.0]], dtype=np.float64)
    expanded = stimulus_to_sample_rate(stimulus, presentation_rate=60, fs=240)
    expected = np.asarray([[1.0] * 4 + [0.0] * 4 + [1.0] * 4], dtype=np.float64)
    np.testing.assert_array_equal(expanded, expected)


def test_build_cca_encodings_respects_start_sample_offset() -> None:
    stimulus = np.asarray(
        [[1.0, 0.0, 1.0] * 80, [0.0, 1.0, 0.0] * 80], dtype=np.float64
    )
    enc0 = build_cca_encodings(
        stimulus,
        fs=240,
        window_samples=24,
        event="refe",
        onset_event=False,
        encoding_length=0.3,
        start_sample=0,
    )
    enc4 = build_cca_encodings(
        stimulus,
        fs=240,
        window_samples=24,
        event="refe",
        onset_event=False,
        encoding_length=0.3,
        start_sample=4,
    )
    assert enc0.shape == enc4.shape == (2, enc0.shape[1], 24)
    assert not np.array_equal(enc0, enc4)
