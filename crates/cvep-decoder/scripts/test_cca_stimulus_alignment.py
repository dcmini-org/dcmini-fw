#!/usr/bin/env -S uv run --script
# /// script
# dependencies = [
#   "h5py>=3.16.0",
#   "mne>=1.11.0",
#   "numpy>=2.2.6",
#   "scipy>=1.15.3",
#   "rich>=14.3.3",
# ]
# ///
"""Regression checks for fs-aligned CCA stimulus handling."""

from __future__ import annotations

import importlib.util
import sys
import types
import unittest
from pathlib import Path
from unittest import mock

import numpy as np


WORKSPACE_ROOT = Path(__file__).resolve().parents[3]
SCRIPTS_ROOT = WORKSPACE_ROOT / "crates/cvep-decoder/scripts"


def load_module(path: Path, name: str):
    sys.path.insert(0, str(path.parent))
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"Failed to load module from {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


RAW_BENCHMARK = load_module(
    SCRIPTS_ROOT / "benchmark_pyntbci_vs_rust.py",
    "test_alignment_raw_benchmark",
)
CCA_BENCHMARK = load_module(
    SCRIPTS_ROOT / "benchmark_cca_vs_rust.py",
    "test_alignment_cca_benchmark",
)


class StimulusAlignmentTest(unittest.TestCase):
    def test_stimulus_to_sample_rate_repeats_frames_at_240hz(self) -> None:
        stimulus = np.asarray([[1.0, 0.0, 1.0]], dtype=np.float64)
        expanded = RAW_BENCHMARK.stimulus_to_sample_rate(
            stimulus,
            presentation_rate=60,
            fs=240,
        )
        expected = np.asarray([[1.0] * 4 + [0.0] * 4 + [1.0] * 4], dtype=np.float64)
        np.testing.assert_array_equal(expanded, expected)

    def test_cca_benchmark_uses_fs_aligned_stimulus(self) -> None:
        data = types.SimpleNamespace(
            x=np.zeros((4, 2, 8), dtype=np.float64),
            y=np.asarray([0, 1, 0, 1], dtype=np.int64),
            stimulus=np.asarray([[1.0, 0.0], [0.0, 1.0]], dtype=np.float64),
            fs=240,
            presentation_rate=60,
            dataset="Dummy",
            subject=1,
            trial_seconds=2.0,
        )
        benchmark = types.SimpleNamespace(
            fold_slices=lambda n_trials, folds: [
                np.asarray([0, 1]),
                np.asarray([2, 3]),
            ],
            slice_windowed_trials_and_stimulus=RAW_BENCHMARK.slice_windowed_trials_and_stimulus,
            stimulus_to_sample_rate=RAW_BENCHMARK.stimulus_to_sample_rate,
        )
        args = types.SimpleNamespace(
            event="refe",
            onset_event=False,
            encoding_length=0.3,
            adc_bits=24,
            adc_headroom=0.95,
            regularization=1.0e-3,
            skip_rust=True,
            profile="matched_embedded_125",
            band_low=6.0,
            band_high=50.0,
            notch_hz=50.0,
            drop_first_seconds=0.0,
            cumulative_update_mode="naive",
            cumulative_min_margin=0.05,
        )
        captured_shapes: list[tuple[tuple[int, ...], int]] = []

        def fake_build_encodings(stimulus, fs, window_samples, **kwargs):
            del fs, kwargs
            captured_shapes.append((stimulus.shape, window_samples))
            return np.zeros((stimulus.shape[0], 1, window_samples), dtype=np.float64)

        fake_reference = types.SimpleNamespace(
            predictions=np.asarray([0, 1], dtype=np.int64),
            scores=np.zeros((2, 2), dtype=np.float64),
        )

        with (
            mock.patch.object(
                CCA_BENCHMARK, "build_cca_encodings", side_effect=fake_build_encodings
            ),
            mock.patch.object(
                CCA_BENCHMARK,
                "instantaneous_cca_predictions_pyntbci",
                return_value=fake_reference,
            ),
        ):
            rows = CCA_BENCHMARK.benchmark_subject_fold_windows(
                "instantaneous_cca",
                data,
                benchmark,
                rust_binary=None,
                fold_idx=0,
                folds=2,
                window_requests_seconds=[1.0],
                args=args,
            )

        self.assertEqual(rows[0]["window"], 8)
        self.assertEqual(captured_shapes, [((2, 8), 8)])


if __name__ == "__main__":
    unittest.main()
