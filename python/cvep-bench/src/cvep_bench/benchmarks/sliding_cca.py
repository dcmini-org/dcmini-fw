from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any

import numpy as np
from rich.console import Console

from cvep_bench.algorithms.cca_reference import (
    build_cca_encodings,
    class_scores_from_encodings,
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
from cvep_bench.datasets.loaders import load_subject
from cvep_bench.datasets.windowing import iter_sliding_windows, stimulus_to_sample_rate
from cvep_bench.evaluation.splits import fold_slices


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    add_data_dir_arg(parser, DEFAULT_DATA_DIR)
    add_output_args(
        parser, output_dir=DEFAULT_DATA_DIR, stem="cca_sliding_window_results"
    )
    add_dataset_args(parser, default_datasets=["Thielen2021"])
    add_fold_args(parser)
    add_target_fs_args(parser, default=250, include_grid=False)
    parser.add_argument(
        "--window-seconds-grid", type=float, nargs="+", default=[1.0, 4.0]
    )
    parser.add_argument("--step-seconds", type=float, default=0.5)
    parser.add_argument("--encoding-length", type=float, default=0.3)
    parser.add_argument("--event", type=str, default="refe")
    parser.add_argument("--onset-event", action="store_true")
    parser.add_argument("--regularization", type=float, default=1.0e-3)
    return parser.parse_args()


def benchmark_subject_fold(
    data: Any,
    fold_idx: int,
    folds: int,
    window_seconds_grid: list[float],
    step_seconds: float,
    encoding_length: float,
    event: str,
    onset_event: bool,
    regularization: float,
) -> list[dict[str, Any]]:
    fold_parts = fold_slices(data.x.shape[0], folds)
    test_idx = fold_parts[fold_idx]
    x_test = data.x[test_idx]
    y_test = data.y[test_idx]
    stimulus_fs = stimulus_to_sample_rate(
        data.stimulus, data.presentation_rate, data.fs
    )
    rows: list[dict[str, Any]] = []
    accumulated_by_window: dict[float, np.ndarray] = {}
    for window in iter_sliding_windows(
        x_test.shape[2], data.fs, window_seconds_grid, step_seconds
    ):
        encodings = build_cca_encodings(
            stimulus_fs,
            data.fs,
            window.window_samples,
            event=event,
            onset_event=onset_event,
            encoding_length=encoding_length,
            start_sample=window.start_sample,
        )
        scores = np.stack(
            [
                class_scores_from_encodings(
                    x_test[trial_idx, :, window.start_sample : window.end_sample],
                    encodings,
                    regularization,
                )
                for trial_idx in range(x_test.shape[0])
            ],
            axis=0,
        )
        predictions = scores.argmax(axis=1)
        accumulated = accumulated_by_window.setdefault(
            window.window_seconds, np.zeros_like(scores)
        )
        accumulated += scores
        accumulated_predictions = accumulated.argmax(axis=1)
        for variant, preds in (
            ("instantaneous_window", predictions),
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
                    "feature_count": int(encodings.shape[1]),
                    "trials": int(x_test.shape[0]),
                    "accuracy": float(np.mean(preds == y_test)),
                }
            )
    return rows


def main() -> None:
    args = parse_args()
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
                        args.encoding_length,
                        args.event,
                        args.onset_event,
                        args.regularization,
                    )
                )
                console.print(
                    f"[blue]cca-sliding[/blue] dataset={dataset} subject={subject} fold={fold_idx} target_fs={args.target_fs}"
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
        "event": args.event,
        "onset_event": args.onset_event,
        "encoding_length": args.encoding_length,
        "regularization": args.regularization,
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
                "trials",
                "accuracy",
            ],
        ),
        encoding="utf-8",
    )
    summary = grouped_summary(rows)
    render_sliding_html_report(
        args.output_html,
        title="Sliding CCA Benchmark",
        subtitle="Sliding-window zero-training CCA over continuous reconstructed trial data.",
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
            ("Features", "feature_count"),
            ("Accuracy", "accuracy"),
        ],
    )
    render_rich_table(
        console,
        title="Sliding CCA Summary",
        columns=[
            ("Variant", "variant"),
            ("Dataset", "dataset"),
            ("Window", "window_seconds"),
            ("Window End", "window_end_seconds"),
            ("Subjects", "subjects"),
            ("Mean Accuracy", "mean_accuracy"),
        ],
        rows=summary,
        formatters={
            "window_seconds": lambda value: f"{value:.3f}",
            "window_end_seconds": lambda value: f"{value:.3f}",
            "mean_accuracy": lambda value: f"{value:.4f}",
        },
    )
