from __future__ import annotations

import argparse
from pathlib import Path
from typing import Any

import numpy as np
from rich.console import Console

from cvep_bench.algorithms.pyntbci_models import build_etrca_bank, fit_etrca
from cvep_bench.benchmarks.pyntbci_vs_rust import DEFAULT_DATA_DIR
from cvep_bench.benchmarks.reporting import (
    build_group_summary,
    render_rich_table,
    render_tabular_html,
    rows_to_csv,
    write_json_payload,
)
from cvep_bench.cli.arg_groups import (
    add_data_dir_arg,
    add_dataset_args,
    add_fold_args,
    add_output_args,
    add_preprocessing_override_args,
    add_profile_arg,
    add_target_fs_args,
    add_window_args,
    resolve_fold_indices,
)
from cvep_bench.datasets.loaders import (
    effective_etrca_cycle_size,
    load_subject,
    subject_list_for_dataset,
    trial_seconds_for_dataset,
    validate_target_fs,
)
from cvep_bench.datasets.profiles import (
    benchmark_profile_names,
    resolve_benchmark_profile,
    resolve_preprocessing_options,
)
from cvep_bench.datasets.windows import seconds_to_samples
from cvep_bench.evaluation.splits import fold_slices
from cvep_bench.export.predictors import exact_etrca_predict


MODE_WITHIN_SUBJECT = "within_subject"
MODE_CROSS_SUBJECT = "cross_subject"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    add_profile_arg(
        parser, choices=benchmark_profile_names(), default="matched_embedded_125"
    )
    add_data_dir_arg(parser, DEFAULT_DATA_DIR)
    add_output_args(parser, output_dir=DEFAULT_DATA_DIR, stem="etrca_training_curve")
    add_dataset_args(parser, default_datasets=["Thielen2021"])
    add_fold_args(parser)
    add_target_fs_args(parser, default=None, include_grid=False)
    add_window_args(parser, default_grid=[1.05, 2.1, 4.2], include_step=False)
    parser.add_argument(
        "--training-window-seconds-grid",
        type=float,
        nargs="+",
        default=[2.1, 4.2, 31.5],
    )
    add_preprocessing_override_args(parser)
    parser.add_argument(
        "--modes",
        nargs="+",
        choices=[MODE_WITHIN_SUBJECT, MODE_CROSS_SUBJECT],
        default=[MODE_WITHIN_SUBJECT, MODE_CROSS_SUBJECT],
    )
    parser.add_argument(
        "--per-class-trials-grid", type=int, nargs="+", default=[1, 2, 3, 4]
    )
    parser.add_argument("--cross-subject-source-limit", type=int, default=None)
    return parser.parse_args()


def select_first_n_per_class(
    x: np.ndarray, y: np.ndarray, per_class_trials: int
) -> tuple[np.ndarray, np.ndarray]:
    selected = []
    for class_label in np.unique(y):
        class_indices = np.flatnonzero(y == class_label)
        if class_indices.shape[0] < per_class_trials:
            raise ValueError(
                f"requested {per_class_trials} training trials for class {class_label}, only found {class_indices.shape[0]}"
            )
        selected.extend(class_indices[:per_class_trials].tolist())
    selected = np.asarray(sorted(selected), dtype=np.int64)
    return x[selected], y[selected]


def load_windowed_subject(
    dataset: str,
    subject: int,
    data_dir: Path,
    target_fs: int,
    window_seconds: float,
    preprocessing: Any,
) -> Any:
    return load_subject(
        dataset,
        subject,
        data_dir,
        target_fs,
        trial_seconds=window_seconds,
        preprocessing=preprocessing,
    )


def load_full_subject(
    dataset: str,
    subject: int,
    data_dir: Path,
    target_fs: int,
    preprocessing: Any,
) -> Any:
    return load_subject(
        dataset,
        subject,
        data_dir,
        target_fs,
        trial_seconds=trial_seconds_for_dataset(dataset),
        preprocessing=preprocessing,
    )


def within_subject_rows(
    *,
    dataset: str,
    subject: int,
    data: Any,
    training_window_seconds_grid: list[float],
    window_seconds: float,
    folds: int,
    fold_indices: list[int],
    per_class_trials_grid: list[int],
) -> list[dict[str, Any]]:
    rows = []
    fold_parts = fold_slices(data.x.shape[0], folds)
    for fold_idx in fold_indices:
        test_idx = fold_parts[fold_idx]
        train_idx = np.concatenate(
            [part for idx, part in enumerate(fold_parts) if idx != fold_idx]
        )
        x_train_full = data.x[train_idx]
        y_train_full = data.y[train_idx]
        window_samples = min(
            seconds_to_samples(window_seconds, data.fs), data.x.shape[2]
        )
        x_test = data.x[test_idx][:, :, :window_samples]
        y_test = data.y[test_idx]
        cycle_seconds = effective_etrca_cycle_size(data.cycle_size, data.fs)
        if cycle_seconds is None:
            raise ValueError("eTRCA requires cycle-aligned training windows")
        cycle_samples = int(round(cycle_seconds * data.fs))
        class_labels = np.unique(data.y)
        for training_window_seconds in training_window_seconds_grid:
            training_window_samples = min(
                seconds_to_samples(training_window_seconds, data.fs),
                data.x.shape[2],
            )
            if training_window_samples % cycle_samples != 0:
                continue
            x_train_cropped = x_train_full[:, :, :training_window_samples]
            actual_training_window_seconds = training_window_samples / data.fs
            for per_class_trials in per_class_trials_grid:
                try:
                    x_train, y_train = select_first_n_per_class(
                        x_train_cropped, y_train_full, per_class_trials
                    )
                except ValueError:
                    continue
                model = fit_etrca(x_train, y_train, data.fs, cycle_seconds)
                spatial_filters, templates = build_etrca_bank(
                    model, window_samples, class_labels
                )
                prediction = exact_etrca_predict(
                    x_test, spatial_filters, templates, class_labels
                )
                rows.append(
                    {
                        "mode": MODE_WITHIN_SUBJECT,
                        "dataset": dataset,
                        "subject": subject,
                        "fold_index": fold_idx,
                        "target_fs": data.fs,
                        "requested_window_seconds": window_seconds,
                        "training_window_seconds": actual_training_window_seconds,
                        "per_class_trials": per_class_trials,
                        "train_trials": int(x_train.shape[0]),
                        "test_trials": int(x_test.shape[0]),
                        "accuracy": float(np.mean(prediction == y_test)),
                    }
                )
    return rows


def cross_subject_rows(
    *,
    dataset: str,
    target_subject: int,
    all_subject_data: dict[int, Any],
    training_window_seconds_grid: list[float],
    window_seconds: float,
    per_class_trials_grid: list[int],
    cross_subject_source_limit: int | None,
) -> list[dict[str, Any]]:
    rows = []
    target_data = all_subject_data[target_subject]
    source_subjects = [
        subject for subject in sorted(all_subject_data) if subject != target_subject
    ]
    if cross_subject_source_limit is not None:
        source_subjects = source_subjects[:cross_subject_source_limit]
    window_samples = min(
        seconds_to_samples(window_seconds, target_data.fs), target_data.x.shape[2]
    )
    x_test = target_data.x[:, :, :window_samples]
    y_test = target_data.y
    cycle_seconds = effective_etrca_cycle_size(target_data.cycle_size, target_data.fs)
    if cycle_seconds is None:
        raise ValueError("eTRCA requires cycle-aligned training windows")
    cycle_samples = int(round(cycle_seconds * target_data.fs))
    class_labels = np.unique(target_data.y)
    for training_window_seconds in training_window_seconds_grid:
        training_window_samples = min(
            seconds_to_samples(training_window_seconds, target_data.fs),
            target_data.x.shape[2],
        )
        if training_window_samples % cycle_samples != 0:
            continue
        actual_training_window_seconds = training_window_samples / target_data.fs
        for per_class_trials in per_class_trials_grid:
            source_trials = []
            source_labels = []
            used_subjects = 0
            for subject in source_subjects:
                source_data = all_subject_data[subject]
                try:
                    x_subj, y_subj = select_first_n_per_class(
                        source_data.x[:, :, :training_window_samples],
                        source_data.y,
                        per_class_trials,
                    )
                except ValueError:
                    continue
                source_trials.append(x_subj)
                source_labels.append(y_subj)
                used_subjects += 1
            if not source_trials:
                continue
            x_train = np.concatenate(source_trials, axis=0)
            y_train = np.concatenate(source_labels, axis=0)
            model = fit_etrca(x_train, y_train, target_data.fs, cycle_seconds)
            spatial_filters, templates = build_etrca_bank(
                model, window_samples, class_labels
            )
            prediction = exact_etrca_predict(
                x_test, spatial_filters, templates, class_labels
            )
            rows.append(
                {
                    "mode": MODE_CROSS_SUBJECT,
                    "dataset": dataset,
                    "subject": target_subject,
                    "fold_index": -1,
                    "target_fs": target_data.fs,
                    "requested_window_seconds": window_seconds,
                    "training_window_seconds": actual_training_window_seconds,
                    "per_class_trials": per_class_trials,
                    "source_subjects": used_subjects,
                    "train_trials": int(x_train.shape[0]),
                    "test_trials": int(x_test.shape[0]),
                    "accuracy": float(np.mean(prediction == y_test)),
                }
            )
    return rows


def grouped_summary(rows: list[dict[str, Any]]) -> list[dict[str, Any]]:
    return build_group_summary(
        rows,
        key_fields=[
            "mode",
            "dataset",
            "target_fs",
            "requested_window_seconds",
            "training_window_seconds",
            "per_class_trials",
        ],
        metric_fields=["accuracy"],
    )


def main() -> None:
    args = parse_args()
    console = Console()
    profile = resolve_benchmark_profile(args.profile)
    target_fs = args.target_fs if args.target_fs is not None else profile.target_fs
    validate_target_fs(target_fs)
    preprocessing = resolve_preprocessing_options(
        profile,
        band_low=args.band_low,
        band_high=args.band_high,
        notch_hz=args.notch_hz,
        drop_first_seconds=args.drop_first_seconds,
    )
    fold_indices = resolve_fold_indices(args.folds, args.fold_index)
    rows: list[dict[str, Any]] = []
    for dataset in args.datasets:
        subjects = args.subjects or subject_list_for_dataset(dataset)
        if args.max_subjects is not None:
            subjects = subjects[: args.max_subjects]
        for window_seconds in args.window_seconds_grid:
            subject_data = {
                subject: load_full_subject(
                    dataset,
                    subject,
                    args.data_dir,
                    target_fs,
                    preprocessing,
                )
                for subject in subjects
            }
            for subject in subjects:
                if MODE_WITHIN_SUBJECT in args.modes:
                    rows.extend(
                        within_subject_rows(
                            dataset=dataset,
                            subject=subject,
                            data=subject_data[subject],
                            training_window_seconds_grid=args.training_window_seconds_grid,
                            window_seconds=window_seconds,
                            folds=args.folds,
                            fold_indices=fold_indices,
                            per_class_trials_grid=args.per_class_trials_grid,
                        )
                    )
                if MODE_CROSS_SUBJECT in args.modes:
                    rows.extend(
                        cross_subject_rows(
                            dataset=dataset,
                            target_subject=subject,
                            all_subject_data=subject_data,
                            training_window_seconds_grid=args.training_window_seconds_grid,
                            window_seconds=window_seconds,
                            per_class_trials_grid=args.per_class_trials_grid,
                            cross_subject_source_limit=args.cross_subject_source_limit,
                        )
                    )
                console.print(
                    f"[blue]etrca-training[/blue] dataset={dataset} subject={subject} window={window_seconds:.3f}s fs={target_fs}"
                )
    payload = {
        "config": {
            "profile": profile.name,
            "datasets": args.datasets,
            "subjects": args.subjects,
            "max_subjects": args.max_subjects,
            "folds": args.folds,
            "fold_index": fold_indices,
            "target_fs": target_fs,
            "window_seconds_grid": args.window_seconds_grid,
            "training_window_seconds_grid": args.training_window_seconds_grid,
            "modes": args.modes,
            "per_class_trials_grid": args.per_class_trials_grid,
            "cross_subject_source_limit": args.cross_subject_source_limit,
        },
        "results": rows,
    }
    write_json_payload(args.output_json, payload)
    args.output_csv.write_text(
        rows_to_csv(
            rows,
            [
                "mode",
                "dataset",
                "subject",
                "fold_index",
                "target_fs",
                "requested_window_seconds",
                "training_window_seconds",
                "per_class_trials",
                "source_subjects",
                "train_trials",
                "test_trials",
                "accuracy",
            ],
        ),
        encoding="utf-8",
    )
    summary_rows = grouped_summary(rows)
    render_tabular_html(
        args.output_html,
        title="eTRCA Training Curve",
        subtitle="How many supervised eTRCA training trials are needed, and does cross-subject transfer work?",
        config=payload["config"],
        summary_columns=[
            ("Mode", "mode"),
            ("Dataset", "dataset"),
            ("fs", "target_fs"),
            ("Window", "requested_window_seconds"),
            ("Train Window", "training_window_seconds"),
            ("Trials/Class", "per_class_trials"),
            ("Subjects", "subjects"),
            ("Mean Accuracy", "mean_accuracy"),
        ],
        summary_rows=summary_rows,
    )
    render_rich_table(
        console,
        title="eTRCA Training Curve",
        columns=[
            ("Mode", "mode"),
            ("Dataset", "dataset"),
            ("fs", "target_fs"),
            ("Window", "requested_window_seconds"),
            ("Train Window", "training_window_seconds"),
            ("Trials/Class", "per_class_trials"),
            ("Subjects", "subjects"),
            ("Mean Accuracy", "mean_accuracy"),
        ],
        rows=summary_rows,
        formatters={
            "requested_window_seconds": lambda value: f"{value:.3f}",
            "training_window_seconds": lambda value: f"{value:.3f}",
            "mean_accuracy": lambda value: f"{value:.4f}",
        },
    )
