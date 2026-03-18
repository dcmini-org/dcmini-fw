from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any

import numpy as np
from rich.console import Console
from rich.table import Table

from cvep_bench.algorithms.cca_reference import (
    build_cca_encodings,
    class_scores_from_encodings,
)
from cvep_bench.benchmarks.pyntbci_vs_rust import DEFAULT_DATA_DIR
from cvep_bench.benchmarks.sliding_windows import (
    grouped_summary,
    render_sliding_html_report,
    rows_to_csv,
    sliding_window_starts,
)
from cvep_bench.datasets.loaders import load_subject, subject_list_for_dataset
from cvep_bench.datasets.windows import (
    fold_slices,
    seconds_to_samples,
    stimulus_to_sample_rate,
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data-dir", type=Path, default=DEFAULT_DATA_DIR)
    parser.add_argument(
        "--output-json",
        type=Path,
        default=DEFAULT_DATA_DIR / "cca_sliding_window_results.json",
    )
    parser.add_argument(
        "--output-csv",
        type=Path,
        default=DEFAULT_DATA_DIR / "cca_sliding_window_results.csv",
    )
    parser.add_argument(
        "--output-html",
        type=Path,
        default=DEFAULT_DATA_DIR / "cca_sliding_window_results.html",
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
    trial_samples = x_test.shape[2]
    step_samples = seconds_to_samples(step_seconds, data.fs)
    stimulus_fs = stimulus_to_sample_rate(
        data.stimulus, data.presentation_rate, data.fs
    )
    rows: list[dict[str, Any]] = []
    for window_seconds in window_seconds_grid:
        window_samples = min(seconds_to_samples(window_seconds, data.fs), trial_samples)
        starts = sliding_window_starts(trial_samples, window_samples, step_samples)
        accumulated_scores = np.zeros(
            (x_test.shape[0], stimulus_fs.shape[0]), dtype=np.float64
        )
        for start in starts:
            end = start + window_samples
            encodings = build_cca_encodings(
                stimulus_fs,
                data.fs,
                window_samples,
                event=event,
                onset_event=onset_event,
                encoding_length=encoding_length,
                start_sample=int(start),
            )
            scores = np.stack(
                [
                    class_scores_from_encodings(
                        x_test[trial_idx, :, start:end],
                        encodings,
                        regularization,
                    )
                    for trial_idx in range(x_test.shape[0])
                ],
                axis=0,
            )
            predictions = scores.argmax(axis=1)
            rows.append(
                {
                    "variant": "instantaneous_window",
                    "dataset": data.dataset,
                    "subject": data.subject,
                    "fold_index": fold_idx,
                    "target_fs": data.fs,
                    "window_seconds": window_samples / data.fs,
                    "step_seconds": step_seconds,
                    "window_start_seconds": start / data.fs,
                    "window_end_seconds": end / data.fs,
                    "window_start_sample": int(start),
                    "window_end_sample": int(end),
                    "feature_count": int(encodings.shape[1]),
                    "trials": int(x_test.shape[0]),
                    "accuracy": float(np.mean(predictions == y_test)),
                }
            )
            accumulated_scores += scores
            accumulated_predictions = accumulated_scores.argmax(axis=1)
            rows.append(
                {
                    "variant": "instantaneous_accumulated",
                    "dataset": data.dataset,
                    "subject": data.subject,
                    "fold_index": fold_idx,
                    "target_fs": data.fs,
                    "window_seconds": window_samples / data.fs,
                    "step_seconds": step_seconds,
                    "window_start_seconds": start / data.fs,
                    "window_end_seconds": end / data.fs,
                    "window_start_sample": int(start),
                    "window_end_sample": int(end),
                    "feature_count": int(encodings.shape[1]),
                    "trials": int(x_test.shape[0]),
                    "accuracy": float(np.mean(accumulated_predictions == y_test)),
                }
            )
    return rows


def main() -> None:
    args = parse_args()
    console = Console()
    rows: list[dict[str, Any]] = []
    fold_indices = args.fold_index or list(range(args.folds))
    for dataset in args.datasets:
        subjects = args.subjects or subject_list_for_dataset(dataset)
        if args.max_subjects is not None:
            subjects = subjects[: args.max_subjects]
        for subject in subjects:
            data = load_subject(dataset, subject, args.data_dir, args.target_fs)
            for fold_idx in fold_indices:
                console.print(
                    f"[blue]cca-sliding[/blue] dataset={dataset} subject={subject} fold={fold_idx} target_fs={args.target_fs}"
                )
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
    table = Table(title="Sliding CCA Summary")
    for column in [
        "Variant",
        "Dataset",
        "Window",
        "Window End",
        "Subjects",
        "Mean Accuracy",
    ]:
        table.add_column(column)
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
