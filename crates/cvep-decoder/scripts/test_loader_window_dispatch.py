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
"""Regression checks for Thielen2021 direct-window loader dispatch."""

from __future__ import annotations

import importlib.util
import sys
import tempfile
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
    "test_benchmark_pyntbci_vs_rust",
)
CCA_BENCHMARK = load_module(
    SCRIPTS_ROOT / "benchmark_cca_vs_rust.py",
    "test_benchmark_cca_vs_rust",
)


def fake_subject_data(
    dataset: str, subject: int, trial_seconds: float
) -> types.SimpleNamespace:
    samples = max(1, int(round(trial_seconds * 10.0)))
    return types.SimpleNamespace(
        dataset=dataset,
        subject=subject,
        x=np.zeros((4, 8, samples), dtype=np.float64),
        y=np.zeros(4, dtype=np.int64),
        fs=250,
        stimulus=np.zeros((20, samples), dtype=np.float64),
        cycle_size=None,
        trial_seconds=trial_seconds,
        presentation_rate=60,
    )


class LoaderWindowDispatchTest(unittest.TestCase):
    def test_raw_benchmark_uses_per_window_thielen2021_rcca_loader(self) -> None:
        calls: list[tuple[str, int, int, float | None]] = []

        def fake_load_subject(
            dataset: str,
            subject: int,
            data_dir: Path,
            target_fs: int,
            trial_seconds: float | None = None,
            preprocessing=None,
            thielen2021_source: str = "raw",
        ):
            del data_dir
            del preprocessing
            del thielen2021_source
            calls.append((dataset, subject, target_fs, trial_seconds))
            return fake_subject_data(dataset, subject, trial_seconds or 31.5)

        with tempfile.TemporaryDirectory() as tmp_dir:
            tmp = Path(tmp_dir)
            argv = [
                "benchmark_pyntbci_vs_rust.py",
                "--datasets",
                "Thielen2021",
                "--subjects",
                "1",
                "--algorithms",
                "rcca",
                "etrca",
                "--target-fs",
                "250",
                "--folds",
                "2",
                "--fold-index",
                "0",
                "--window-seconds-grid",
                "4.2",
                "8.4",
                "--output-json",
                str(tmp / "out.json"),
                "--output-csv",
                str(tmp / "out.csv"),
                "--output-html",
                str(tmp / "out.html"),
            ]
            with (
                mock.patch.object(sys, "argv", argv),
                mock.patch.object(
                    RAW_BENCHMARK, "build_rust_binary", return_value=tmp / "fake"
                ),
                mock.patch.object(
                    RAW_BENCHMARK, "load_subject", side_effect=fake_load_subject
                ),
                mock.patch.object(
                    RAW_BENCHMARK, "benchmark_subject_fold_windows", return_value=[]
                ),
                mock.patch.object(RAW_BENCHMARK, "render_html_report"),
                mock.patch.object(RAW_BENCHMARK, "render_summary"),
            ):
                RAW_BENCHMARK.main()

        self.assertEqual(
            calls,
            [
                ("Thielen2021", 1, 250, 4.2),
                ("Thielen2021", 1, 250, 8.4),
                ("Thielen2021", 1, 250, None),
            ],
        )

    def test_cca_benchmark_uses_per_window_thielen2021_direct_loader(self) -> None:
        calls: list[tuple[str, int, int, float | None]] = []

        def fake_load_subject(
            dataset: str,
            subject: int,
            data_dir: Path,
            target_fs: int,
            trial_seconds: float | None = None,
            preprocessing=None,
            thielen2021_source: str = "raw",
        ):
            del data_dir
            del preprocessing
            del thielen2021_source
            calls.append((dataset, subject, target_fs, trial_seconds))
            return fake_subject_data(dataset, subject, trial_seconds or 31.5)

        fake_benchmark = types.SimpleNamespace(
            subject_list_for_dataset=lambda dataset: [1],
            validate_target_fs=lambda target_fs: None,
            trial_seconds_for_dataset=lambda dataset: 31.5,
            resolve_benchmark_profile=RAW_BENCHMARK.resolve_benchmark_profile,
            resolve_preprocessing_options=RAW_BENCHMARK.resolve_preprocessing_options,
            resolve_event=RAW_BENCHMARK.resolve_event,
            resolve_onset_event=RAW_BENCHMARK.resolve_onset_event,
            resolve_encoding_length=RAW_BENCHMARK.resolve_encoding_length,
            resolve_target_fs=RAW_BENCHMARK.resolve_target_fs,
            resolve_window_grid=RAW_BENCHMARK.resolve_window_grid,
            decode_window_requests=lambda full_trial_seconds, explicit, step_seconds: [
                4.2,
                8.4,
            ],
            loader_trial_seconds_for_algorithm=RAW_BENCHMARK.loader_trial_seconds_for_algorithm,
            load_subject=fake_load_subject,
        )

        with tempfile.TemporaryDirectory() as tmp_dir:
            tmp = Path(tmp_dir)
            argv = [
                "benchmark_cca_vs_rust.py",
                "--datasets",
                "Thielen2021",
                "--subjects",
                "1",
                "--algorithms",
                "instantaneous_cca",
                "cumulative_cca",
                "--target-fs",
                "250",
                "--folds",
                "2",
                "--fold-index",
                "0",
                "--window-seconds-grid",
                "4.2",
                "8.4",
                "--output-json",
                str(tmp / "out.json"),
                "--output-csv",
                str(tmp / "out.csv"),
                "--output-html",
                str(tmp / "out.html"),
            ]
            with (
                mock.patch.object(sys, "argv", argv),
                mock.patch.object(
                    CCA_BENCHMARK, "load_benchmark_module", return_value=fake_benchmark
                ),
                mock.patch.object(
                    CCA_BENCHMARK, "build_rust_binary", return_value=tmp / "fake"
                ),
                mock.patch.object(
                    CCA_BENCHMARK, "benchmark_subject_fold_windows", return_value=[]
                ),
                mock.patch.object(CCA_BENCHMARK, "render_html_report"),
            ):
                CCA_BENCHMARK.main()

        self.assertEqual(
            calls,
            [
                ("Thielen2021", 1, 250, 4.2),
                ("Thielen2021", 1, 250, 8.4),
                ("Thielen2021", 1, 250, 4.2),
                ("Thielen2021", 1, 250, 8.4),
            ],
        )


if __name__ == "__main__":
    unittest.main()
