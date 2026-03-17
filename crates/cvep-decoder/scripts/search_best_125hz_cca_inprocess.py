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
"""In-process 125 Hz zero-training CCA sweeps, including regularized references."""

from __future__ import annotations

import argparse
import importlib.util
import json
import sys
from pathlib import Path
from typing import Any

import numpy as np
from rich.console import Console
from rich.table import Table


WORKSPACE_ROOT = Path(__file__).resolve().parents[3]
SCRIPTS_ROOT = WORKSPACE_ROOT / "crates/cvep-decoder/scripts"


def load_module(path: Path, name: str) -> Any:
    sys.path.insert(0, str(path.parent))
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"Failed to load module from {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


BENCH = load_module(SCRIPTS_ROOT / "benchmark_pyntbci_vs_rust.py", "cca_search_bench")
CCA = load_module(SCRIPTS_ROOT / "cca_reference_utils.py", "cca_search_utils")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--data-dir", type=Path, default=WORKSPACE_ROOT / "crates/cvep-decoder/data"
    )
    parser.add_argument("--target-fs", type=int, default=125)
    parser.add_argument("--subjects", type=int, nargs="+", default=None)
    parser.add_argument("--max-subjects", type=int, default=8)
    parser.add_argument("--folds", type=int, default=5)
    parser.add_argument("--fold-index", type=int, default=0)
    parser.add_argument(
        "--window-seconds-grid", type=float, nargs="+", default=[1.05, 2.1, 4.2]
    )
    parser.add_argument(
        "--output-json",
        type=Path,
        default=WORKSPACE_ROOT
        / "crates/cvep-decoder/data/search_best_125hz_cca_inprocess.json",
    )
    return parser.parse_args()


def subjects_for_args(args: argparse.Namespace) -> list[int]:
    all_subjects = BENCH.subject_list_for_dataset("Thielen2021")
    if args.subjects is not None:
        return args.subjects
    return all_subjects[: args.max_subjects]


def score_config(args: argparse.Namespace, config: dict[str, Any]) -> dict[str, Any]:
    subjects = subjects_for_args(args)
    per_window: dict[float, list[float]] = {
        window: [] for window in args.window_seconds_grid
    }
    profile_scores: list[float] = []

    for subject in subjects:
        for window_seconds in args.window_seconds_grid:
            preprocessing = BENCH.PreprocessingOptions(
                band_low=config["band_low"],
                band_high=config["band_high"],
                notch_hz=50.0,
                pretrial_buffer_seconds=0.5,
                drop_first_seconds=config["drop_first_seconds"],
            )
            data = BENCH.load_subject(
                "Thielen2021",
                subject,
                args.data_dir,
                args.target_fs,
                trial_seconds=window_seconds,
                preprocessing=preprocessing,
                thielen2021_source="raw",
            )
            test_idx = BENCH.fold_slices(data.x.shape[0], args.folds)[args.fold_index]
            x = data.x[test_idx]
            y = data.y[test_idx]
            stimulus_fs = BENCH.stimulus_to_sample_rate(
                data.stimulus,
                presentation_rate=data.presentation_rate,
                fs=data.fs,
            )

            if config["backend"] == "pyntbci":
                if config["algorithm"] == "instantaneous_cca":
                    result = CCA.instantaneous_cca_predictions_pyntbci(
                        x,
                        stimulus_fs,
                        data.fs,
                        event=config["event"],
                        onset_event=config["onset_event"],
                        encoding_length=config["encoding_length"],
                    )
                else:
                    result = CCA.cumulative_cca_predictions_pyntbci(
                        x,
                        stimulus_fs,
                        data.fs,
                        event=config["event"],
                        onset_event=config["onset_event"],
                        encoding_length=config["encoding_length"],
                    )
            else:
                encodings = CCA.build_cca_encodings(
                    stimulus_fs,
                    data.fs,
                    x.shape[2],
                    event=config["event"],
                    onset_event=config["onset_event"],
                    encoding_length=config["encoding_length"],
                )
                if config["algorithm"] == "instantaneous_cca":
                    result = CCA.instantaneous_cca_predictions_reference(
                        x,
                        encodings,
                        regularization=config["regularization"],
                    )
                else:
                    result = CCA.cumulative_cca_predictions_reference(
                        x,
                        encodings,
                        regularization=config["regularization"],
                        min_margin=config.get("min_margin"),
                    )

            accuracy = float(np.mean(result.predictions == y))
            per_window[window_seconds].append(accuracy)
            profile_scores.append(accuracy)

    summary = {
        "mean_accuracy": float(np.mean(profile_scores)),
        "per_window": {
            str(window): float(np.mean(values)) for window, values in per_window.items()
        },
    }
    return {**config, **summary}


def candidate_configs() -> list[dict[str, Any]]:
    configs: list[dict[str, Any]] = []
    for algorithm in ["instantaneous_cca", "cumulative_cca"]:
        configs.append(
            {
                "name": f"{algorithm}_pyntbci_refe",
                "backend": "pyntbci",
                "algorithm": algorithm,
                "event": "refe",
                "onset_event": False,
                "encoding_length": 0.3,
                "band_low": 6.0,
                "band_high": 50.0,
                "drop_first_seconds": 0.0,
                "regularization": None,
            }
        )
        for regularization in [1.0e-3, 3.0e-3, 1.0e-2, 3.0e-2, 1.0e-1]:
            configs.append(
                {
                    "name": f"{algorithm}_reference_reg_{regularization:g}",
                    "backend": "reference",
                    "algorithm": algorithm,
                    "event": "refe",
                    "onset_event": False,
                    "encoding_length": 0.3,
                    "band_low": 6.0,
                    "band_high": 50.0,
                    "drop_first_seconds": 0.0,
                    "regularization": regularization,
                    "min_margin": 0.05 if algorithm == "cumulative_cca" else None,
                }
            )
    return configs


def main() -> None:
    args = parse_args()
    console = Console()
    results = []
    for config in candidate_configs():
        console.print(f"[cyan]search[/cyan] {config['name']}")
        try:
            results.append(score_config(args, config))
        except Exception as exc:  # noqa: BLE001
            results.append(
                {
                    **config,
                    "error": str(exc),
                    "mean_accuracy": float("nan"),
                    "per_window": {},
                }
            )
    valid = [row for row in results if not np.isnan(row["mean_accuracy"])]
    valid.sort(key=lambda row: (row["algorithm"], -row["mean_accuracy"]))

    payload = {
        "config": {
            "target_fs": args.target_fs,
            "subjects": subjects_for_args(args),
            "fold_index": args.fold_index,
            "window_seconds_grid": args.window_seconds_grid,
        },
        "results": results,
        "best": {
            algorithm: max(
                (row for row in valid if row["algorithm"] == algorithm),
                key=lambda row: row["mean_accuracy"],
            )
            for algorithm in ["instantaneous_cca", "cumulative_cca"]
        },
    }
    args.output_json.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")

    table = Table(title="In-process 125 Hz CCA Search")
    for column in ["Name", "Backend", "Alg", "Mean", "1.05", "2.1", "4.2"]:
        table.add_column(column)
    for row in valid:
        per_window = row["per_window"]
        table.add_row(
            row["name"],
            row["backend"],
            row["algorithm"],
            f"{row['mean_accuracy']:.4f}",
            f"{per_window.get('1.05', float('nan')):.4f}",
            f"{per_window.get('2.1', float('nan')):.4f}",
            f"{per_window.get('4.2', float('nan')):.4f}",
        )
    console.print(table)


if __name__ == "__main__":
    main()
