from __future__ import annotations

import argparse
import json
from pathlib import Path

import numpy as np
from rich.console import Console
from rich.table import Table

from cvep_bench.algorithms import cca_reference as cca
from cvep_bench.benchmarks.pyntbci_vs_rust import DEFAULT_DATA_DIR
from cvep_bench.datasets.loaders import load_subject, subject_list_for_dataset
from cvep_bench.datasets.models import PreprocessingOptions
from cvep_bench.datasets.windows import fold_slices, stimulus_to_sample_rate


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data-dir", type=Path, default=DEFAULT_DATA_DIR)
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
        default=DEFAULT_DATA_DIR / "search_best_125hz_cca_inprocess.json",
    )
    return parser.parse_args()


def subjects_for_args(args: argparse.Namespace) -> list[int]:
    return args.subjects or subject_list_for_dataset("Thielen2021")[: args.max_subjects]


def score_config(args: argparse.Namespace, config: dict) -> dict:
    subjects = subjects_for_args(args)
    per_window = {window: [] for window in args.window_seconds_grid}
    profile_scores: list[float] = []
    for subject in subjects:
        for window_seconds in args.window_seconds_grid:
            preprocessing = PreprocessingOptions(
                band_low=config["band_low"],
                band_high=config["band_high"],
                notch_hz=50.0,
                pretrial_buffer_seconds=0.5,
                drop_first_seconds=config["drop_first_seconds"],
            )
            data = load_subject(
                "Thielen2021",
                subject,
                args.data_dir,
                args.target_fs,
                trial_seconds=window_seconds,
                preprocessing=preprocessing,
                thielen2021_source="raw",
            )
            idx = fold_slices(data.x.shape[0], args.folds)[args.fold_index]
            x = data.x[idx]
            y = data.y[idx]
            stimulus_fs = stimulus_to_sample_rate(
                data.stimulus, presentation_rate=data.presentation_rate, fs=data.fs
            )
            if config["backend"] == "pyntbci":
                if config["algorithm"] == "instantaneous_cca":
                    result = cca.instantaneous_cca_predictions_pyntbci(
                        x,
                        stimulus_fs,
                        data.fs,
                        event=config["event"],
                        onset_event=config["onset_event"],
                        encoding_length=config["encoding_length"],
                    )
                else:
                    result = cca.cumulative_cca_predictions_pyntbci(
                        x,
                        stimulus_fs,
                        data.fs,
                        event=config["event"],
                        onset_event=config["onset_event"],
                        encoding_length=config["encoding_length"],
                    )
            else:
                encodings = cca.build_cca_encodings(
                    stimulus_fs,
                    data.fs,
                    x.shape[2],
                    event=config["event"],
                    onset_event=config["onset_event"],
                    encoding_length=config["encoding_length"],
                )
                if config["algorithm"] == "instantaneous_cca":
                    result = cca.instantaneous_cca_predictions_reference(
                        x, encodings, regularization=config["regularization"]
                    )
                else:
                    result = cca.cumulative_cca_predictions_reference(
                        x,
                        encodings,
                        regularization=config["regularization"],
                        min_margin=config.get("min_margin"),
                    )
            accuracy = float(np.mean(result.predictions == y))
            per_window[window_seconds].append(accuracy)
            profile_scores.append(accuracy)
    return {
        **config,
        "mean_accuracy": float(np.mean(profile_scores)),
        "per_window": {
            str(window): float(np.mean(values)) for window, values in per_window.items()
        },
    }


def candidate_configs() -> list[dict]:
    configs: list[dict] = []
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
