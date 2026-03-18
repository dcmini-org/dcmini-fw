from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any

import numpy as np
from rich.console import Console
from rich.table import Table

from cvep_bench.algorithms.umm_features import (
    build_umm_features,
    instantaneous_umm_predictions,
    make_structure,
)
from cvep_bench.benchmarks.pyntbci_vs_rust import DEFAULT_DATA_DIR
from cvep_bench.benchmarks.sliding_windows import (
    grouped_summary,
    render_sliding_html_report,
    rows_to_csv,
    sliding_window_starts,
)
from cvep_bench.datasets.loaders import (
    load_subject,
    subject_list_for_dataset,
    validate_target_fs,
)
from cvep_bench.datasets.windows import fold_slices, seconds_to_samples


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data-dir", type=Path, default=DEFAULT_DATA_DIR)
    parser.add_argument(
        "--output-json",
        type=Path,
        default=DEFAULT_DATA_DIR / "umm_sliding_window_results.json",
    )
    parser.add_argument(
        "--output-csv",
        type=Path,
        default=DEFAULT_DATA_DIR / "umm_sliding_window_results.csv",
    )
    parser.add_argument(
        "--output-html",
        type=Path,
        default=DEFAULT_DATA_DIR / "umm_sliding_window_results.html",
    )
    parser.add_argument("--datasets", nargs="+", default=["Thielen2021"])
    parser.add_argument("--subjects", type=int, nargs="+", default=None)
    parser.add_argument("--max-subjects", type=int, default=None)
    parser.add_argument("--folds", type=int, default=5)
    parser.add_argument("--fold-index", type=int, nargs="+", default=None)
    parser.add_argument("--target-fs", type=int, default=250)
    parser.add_argument(
        "--window-seconds-grid", type=float, nargs="+", default=[1.0, 4.0]
    )
    parser.add_argument("--step-seconds", type=float, default=0.5)
    parser.add_argument("--epoch-seconds", type=float, default=0.3)
    parser.add_argument(
        "--epoch-schedule",
        choices=["rounded_stride", "fractional_onset"],
        default="fractional_onset",
    )
    parser.add_argument("--lag-seconds", type=float, default=0.05)
    parser.add_argument(
        "--layout", choices=["channel_prime", "time_prime"], default="channel_prime"
    )
    parser.add_argument("--trial-demean", action="store_true")
    parser.add_argument("--epoch-demean", action="store_true")
    parser.add_argument("--regularization", type=float, default=1.0e-3)
    return parser.parse_args()


def benchmark_subject_fold(
    data: Any,
    fold_idx: int,
    folds: int,
    window_seconds_grid: list[float],
    step_seconds: float,
    epoch_seconds: float,
    epoch_schedule: str,
    lag_seconds: float,
    layout: str,
    trial_demean: bool,
    epoch_demean: bool,
    regularization: float,
) -> list[dict[str, Any]]:
    fold_parts = fold_slices(data.x.shape[0], folds)
    test_idx = fold_parts[fold_idx]
    x_test = data.x[test_idx]
    y_test = data.y[test_idx]
    trial_samples = x_test.shape[2]
    step_samples = seconds_to_samples(step_seconds, data.fs)
    rows: list[dict[str, Any]] = []
    for window_seconds in window_seconds_grid:
        window_samples = seconds_to_samples(window_seconds, data.fs)
        starts = sliding_window_starts(trial_samples, window_samples, step_samples)
        accumulated_scores: np.ndarray | None = None
        for start in starts:
            stop = start + window_samples
            x_window = x_test[:, :, start:stop]
            exported = build_umm_features(
                trials=x_window,
                stimulus=data.stimulus,
                fs=data.fs,
                presentation_rate=data.presentation_rate,
                epoch_seconds=epoch_seconds,
                layout=layout,
                epoch_schedule=epoch_schedule,
                response_lag_seconds=lag_seconds,
                trial_demean=trial_demean,
                epoch_demean=epoch_demean,
                window_start_seconds=start / data.fs,
            )
            structure = make_structure(exported)
            inst_predictions, inst_scores = instantaneous_umm_predictions(
                exported.features,
                exported.codebook,
                regularization=regularization,
                structure=structure,
            )
            accumulated_scores = (
                inst_scores.astype(np.float64, copy=True)
                if accumulated_scores is None
                else accumulated_scores + inst_scores
            )
            accumulated_predictions = np.argmax(accumulated_scores, axis=1)
            for variant, predictions in (
                ("instantaneous_window", inst_predictions),
                ("instantaneous_accumulated", accumulated_predictions),
            ):
                rows.append(
                    {
                        "variant": variant,
                        "dataset": data.dataset,
                        "subject": data.subject,
                        "fold_index": fold_idx,
                        "target_fs": data.fs,
                        "window_seconds": window_seconds,
                        "step_seconds": step_seconds,
                        "window_start_seconds": start / data.fs,
                        "window_end_seconds": stop / data.fs,
                        "window_start_sample": int(start),
                        "window_end_sample": int(stop),
                        "feature_count": int(exported.features.shape[1]),
                        "epochs_per_window": int(exported.features.shape[2]),
                        "trials": int(exported.features.shape[0]),
                        "accuracy": float(np.mean(predictions == y_test)),
                    }
                )
    return rows


def main() -> None:
    args = parse_args()
    validate_target_fs(args.target_fs)
    console = Console()
    rows: list[dict[str, Any]] = []
    for dataset in args.datasets:
        subjects = args.subjects or subject_list_for_dataset(dataset)
        if args.max_subjects is not None:
            subjects = subjects[: args.max_subjects]
        fold_indices = args.fold_index or list(range(args.folds))
        for subject in subjects:
            data = load_subject(dataset, subject, args.data_dir, args.target_fs)
            for fold_idx in fold_indices:
                rows.extend(
                    benchmark_subject_fold(
                        data,
                        fold_idx,
                        args.folds,
                        args.window_seconds_grid,
                        args.step_seconds,
                        args.epoch_seconds,
                        args.epoch_schedule,
                        args.lag_seconds,
                        args.layout,
                        args.trial_demean,
                        args.epoch_demean,
                        args.regularization,
                    )
                )
                console.print(
                    f"[blue]umm-sliding[/blue] dataset={dataset} subject={subject} fold={fold_idx}"
                )
    config = {
        "datasets": args.datasets,
        "subjects": args.subjects,
        "max_subjects": args.max_subjects,
        "folds": args.folds,
        "fold_index": args.fold_index,
        "target_fs": args.target_fs,
        "window_seconds_grid": args.window_seconds_grid,
        "step_seconds": args.step_seconds,
        "epoch_seconds": args.epoch_seconds,
        "epoch_schedule": args.epoch_schedule,
        "lag_seconds": args.lag_seconds,
        "layout": args.layout,
        "trial_demean": args.trial_demean,
        "epoch_demean": args.epoch_demean,
        "regularization": args.regularization,
        "note": "The `instantaneous_accumulated` variant accumulates class scores across overlapping windows within a trial. A truly stateful cumulative UMM-over-windows variant is not included here because shifted windows carry shifted codebook slices, so the current helper would need a window-dependent label model to do that correctly.",
    }
    payload = {"config": config, "results": rows}
    args.output_json.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    args.output_csv.write_text(
        rows_to_csv(
            rows,
            [
                "variant",
                "dataset",
                "subject",
                "fold_index",
                "target_fs",
                "window_seconds",
                "step_seconds",
                "window_start_seconds",
                "window_end_seconds",
                "window_start_sample",
                "window_end_sample",
                "feature_count",
                "epochs_per_window",
                "trials",
                "accuracy",
            ],
        ),
        encoding="utf-8",
    )
    summary = grouped_summary(rows)
    render_sliding_html_report(
        args.output_html,
        title="Sliding UMM Benchmark",
        subtitle="Sliding-window UMM decoding over continuous reconstructed trial data.",
        config=config,
        rows=rows,
        summary=summary,
        summary_columns=[
            ("Variant", "variant"),
            ("Dataset", "dataset"),
            ("Window", "window_seconds"),
            ("Window End", "window_end_seconds"),
            ("Subjects", "subjects"),
            ("Mean Accuracy", "mean_accuracy"),
        ],
        detail_columns=[
            ("Variant", "variant"),
            ("Dataset", "dataset"),
            ("Subject", "subject"),
            ("Fold", "fold_index"),
            ("Window", "window_seconds"),
            ("Start", "window_start_seconds"),
            ("End", "window_end_seconds"),
            ("Epochs", "epochs_per_window"),
            ("Accuracy", "accuracy"),
        ],
    )
    table = Table(title="Sliding UMM summary")
    for col in ["variant", "dataset", "window", "end", "subjects", "mean_acc"]:
        table.add_column(col)
    for row in summary:
        table.add_row(
            row["variant"],
            row["dataset"],
            f"{row['window_seconds']:.3f}",
            f"{row['window_end_seconds']:.3f}",
            str(row["subjects"]),
            f"{row['mean_accuracy']:.4f}",
        )
    console.print(table)
