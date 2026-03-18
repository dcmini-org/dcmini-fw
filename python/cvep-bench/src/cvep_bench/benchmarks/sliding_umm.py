from __future__ import annotations

import argparse
from pathlib import Path
from typing import Any, cast

import numpy as np
from rich.console import Console

from cvep_bench.algorithms.umm_features import (
    EpochScheduleName,
    LayoutName,
    build_umm_features,
    instantaneous_umm_predictions,
    make_structure,
)
from cvep_bench.benchmarks.pyntbci_vs_rust import DEFAULT_DATA_DIR
from cvep_bench.benchmarks.reporting import render_rich_table, write_json_payload
from cvep_bench.benchmarks.sliding_windows import (
    grouped_summary,
    render_sliding_html_report,
    rows_to_csv,
)
from cvep_bench.cli.arg_groups import (
    add_data_dir_arg,
    add_dataset_args,
    add_fold_args,
    add_output_args,
    add_target_fs_args,
    resolve_fold_indices,
)
from cvep_bench.datasets.loaders import load_subject, validate_target_fs
from cvep_bench.datasets.windowing import iter_sliding_windows
from cvep_bench.evaluation.splits import fold_slices


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    add_data_dir_arg(parser, DEFAULT_DATA_DIR)
    add_output_args(
        parser, output_dir=DEFAULT_DATA_DIR, stem="umm_sliding_window_results"
    )
    add_dataset_args(parser, default_datasets=["Thielen2021"])
    add_fold_args(parser)
    add_target_fs_args(parser, default=250, include_grid=False)
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
    rows: list[dict[str, Any]] = []
    accumulated_by_window: dict[float, np.ndarray] = {}
    for window in iter_sliding_windows(
        x_test.shape[2], data.fs, window_seconds_grid, step_seconds
    ):
        x_window = x_test[:, :, window.start_sample : window.end_sample]
        exported = build_umm_features(
            trials=x_window,
            stimulus=data.stimulus,
            fs=data.fs,
            presentation_rate=data.presentation_rate,
            epoch_seconds=epoch_seconds,
            layout=cast(LayoutName, layout),
            epoch_schedule=cast(EpochScheduleName, epoch_schedule),
            response_lag_seconds=lag_seconds,
            trial_demean=trial_demean,
            epoch_demean=epoch_demean,
            window_start_seconds=window.start_sample / data.fs,
        )
        structure = make_structure(exported)
        inst_predictions, inst_scores = instantaneous_umm_predictions(
            exported.features,
            exported.codebook,
            regularization=regularization,
            structure=structure,
        )
        accumulated = accumulated_by_window.setdefault(
            window.window_seconds, np.zeros_like(inst_scores, dtype=np.float64)
        )
        accumulated += inst_scores
        accumulated_predictions = np.argmax(accumulated, axis=1)
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
                    "window_seconds": window.window_seconds,
                    "step_seconds": step_seconds,
                    "window_start_seconds": window.start_sample / data.fs,
                    "window_end_seconds": window.end_sample / data.fs,
                    "window_start_sample": window.start_sample,
                    "window_end_sample": window.end_sample,
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
    fold_indices = resolve_fold_indices(args.folds, args.fold_index)
    for dataset in args.datasets:
        subjects = args.subjects or []
        if not subjects:
            from cvep_bench.datasets.loaders import subject_list_for_dataset

            subjects = subject_list_for_dataset(dataset)
        if args.max_subjects is not None:
            subjects = subjects[: args.max_subjects]
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
        "fold_index": fold_indices,
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
    write_json_payload(args.output_json, payload)
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
    render_rich_table(
        console,
        title="Sliding UMM summary",
        columns=[
            ("variant", "variant"),
            ("dataset", "dataset"),
            ("window", "window_seconds"),
            ("end", "window_end_seconds"),
            ("subjects", "subjects"),
            ("mean_acc", "mean_accuracy"),
        ],
        rows=summary,
        formatters={
            "window_seconds": lambda value: f"{value:.3f}",
            "window_end_seconds": lambda value: f"{value:.3f}",
            "mean_accuracy": lambda value: f"{value:.4f}",
        },
    )
