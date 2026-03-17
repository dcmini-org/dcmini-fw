#!/usr/bin/env -S uv run --script
# /// script
# dependencies = [
#   "h5py>=3.16.0",
#   "mne>=1.11.0",
#   "numpy>=2.2.6",
#   "pyntbci>=1.8.3",
#   "rich>=14.3.3",
#   "scipy>=1.15.3",
# ]
# ///
"""Regression check for packaged-vs-raw Thielen2021 zero-training CCA parity."""

from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path


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


BENCHMARK = load_module(
    SCRIPTS_ROOT / "benchmark_pyntbci_vs_rust.py",
    "test_parity_raw_benchmark",
)
CCA_BENCHMARK = load_module(
    SCRIPTS_ROOT / "benchmark_cca_vs_rust.py",
    "test_parity_cca_benchmark",
)


class Thielen2021CcaParityTest(unittest.TestCase):
    def _run_source(self, source: str) -> dict[str, float]:
        benchmark = BENCHMARK
        data = benchmark.load_subject(
            "Thielen2021",
            1,
            WORKSPACE_ROOT / "crates/cvep-decoder/data",
            240,
            trial_seconds=4.2,
            preprocessing=benchmark.default_preprocessing_options(),
            thielen2021_source=source,
        )
        fake_args = type(
            "Args",
            (),
            {
                "event": "refe",
                "onset_event": False,
                "encoding_length": 0.3,
                "adc_bits": 24,
                "adc_headroom": 0.95,
                "regularization": 1.0e-3,
                "skip_rust": True,
                "profile": "matched_embedded_125",
                "band_low": 1.0,
                "band_high": 65.0,
                "notch_hz": 50.0,
                "drop_first_seconds": 0.0,
                "cumulative_update_mode": "naive",
                "cumulative_min_margin": 0.05,
            },
        )()
        rows = CCA_BENCHMARK.benchmark_subject_fold_windows(
            "instantaneous_cca",
            data,
            benchmark,
            rust_binary=None,
            fold_idx=0,
            folds=5,
            window_requests_seconds=[4.2],
            args=fake_args,
        )
        rows += CCA_BENCHMARK.benchmark_subject_fold_windows(
            "cumulative_cca",
            data,
            benchmark,
            rust_binary=None,
            fold_idx=0,
            folds=5,
            window_requests_seconds=[4.2],
            args=fake_args,
        )
        return {row["algorithm"]: row["python_reference_accuracy"] for row in rows}

    def test_packaged_and_raw_4p2s_240hz_stay_close(self) -> None:
        packaged = self._run_source("packaged")
        raw = self._run_source("raw")
        self.assertGreaterEqual(packaged["instantaneous_cca"], 0.85)
        self.assertGreaterEqual(packaged["cumulative_cca"], 0.95)
        self.assertLessEqual(
            abs(packaged["instantaneous_cca"] - raw["instantaneous_cca"]),
            0.10,
        )
        self.assertLessEqual(
            abs(packaged["cumulative_cca"] - raw["cumulative_cca"]),
            0.10,
        )


if __name__ == "__main__":
    unittest.main()
