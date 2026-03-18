from __future__ import annotations

import argparse
from pathlib import Path
from typing import Any, cast

import numpy as np
from rich.console import Console

from cvep_bench.algorithms.pyntbci_models import quantize_trials_to_adc
from cvep_bench.algorithms.umm_features import (
    ConfidenceModelName,
    EpochScheduleName,
    LayoutName,
    build_umm_features,
    cumulative_umm_predictions,
    instantaneous_umm_predictions,
    make_structure,
)
from cvep_bench.benchmarks.orchestration import ensure_output_dirs, resolve_subjects
from cvep_bench.benchmarks.reporting import (
    build_group_summary,
    render_rich_table,
    render_tabular_html,
    rows_to_csv,
    write_json_payload,
)
from cvep_bench.cli.arg_groups import (
    add_adc_args,
    add_data_dir_arg,
    add_dataset_args,
    add_fold_args,
    add_output_args,
    add_rust_args,
    add_target_fs_args,
    add_window_args,
    resolve_fold_indices,
)
from cvep_bench.datasets.loaders import load_subject, validate_target_fs
from cvep_bench.datasets.windows import decode_window_requests, seconds_to_samples
from cvep_bench.evaluation.splits import fold_slices
from cvep_bench.runtime.binaries import WORKSPACE_ROOT, build_rust_binary
from cvep_bench.runtime.json_fixtures import temporary_fixture_path
from cvep_bench.runtime.runner import maybe_run_fixture_payload


DEFAULT_DATA_DIR = WORKSPACE_ROOT / "crates" / "cvep-decoder" / "data"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    add_data_dir_arg(parser, DEFAULT_DATA_DIR)
    add_output_args(parser, output_dir=DEFAULT_DATA_DIR, stem="umm_vs_rust_results")
    parser.add_argument(
        "--algorithms",
        nargs="+",
        choices=["instantaneous_umm", "cumulative_umm"],
        default=["instantaneous_umm", "cumulative_umm"],
    )
    add_dataset_args(parser, default_datasets=["Thielen2021"])
    add_fold_args(parser)
    add_target_fs_args(parser, default=250, include_grid=False)
    add_window_args(parser, default_grid=None, include_step=True)
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
    parser.add_argument(
        "--confidence-model",
        choices=["inferred_normalized_margin", "margin_over_winner"],
        default="inferred_normalized_margin",
    )
    parser.add_argument("--regularization", type=float, default=1.0e-3)
    add_adc_args(parser)
    add_rust_args(parser)
    return parser.parse_args()


def benchmark_subject_fold_windows(
    algorithm: str,
    data: Any,
    rust_binary: Path | None,
    fold_idx: int,
    folds: int,
    window_requests_seconds: list[float],
    args: argparse.Namespace,
) -> list[dict[str, Any]]:
    fold_parts = fold_slices(data.x.shape[0], folds)
    test_idx = fold_parts[fold_idx]
    train_idx = np.concatenate(
        [part for idx, part in enumerate(fold_parts) if idx != fold_idx]
    )
    x_test = data.x[test_idx]
    y_test = data.y[test_idx]
    rows: list[dict[str, Any]] = []
    for requested_window_seconds in window_requests_seconds:
        requested_samples = int(np.floor(requested_window_seconds * data.fs + 0.5))
        window_samples = min(requested_samples, data.x.shape[2])
        x_test_window = x_test[:, :, :window_samples]
        exported = build_umm_features(
            x_test_window,
            data.stimulus,
            data.fs,
            data.presentation_rate,
            args.epoch_seconds,
            cast(LayoutName, args.layout),
            cast(EpochScheduleName, args.epoch_schedule),
            args.lag_seconds,
            args.trial_demean,
            args.epoch_demean,
        )
        structure = make_structure(exported)
        if algorithm == "instantaneous_umm":
            benchmark_predictions, _scores = instantaneous_umm_predictions(
                exported.features,
                exported.codebook,
                regularization=args.regularization,
                structure=structure,
            )
        else:
            benchmark_predictions, _scores, _state = cumulative_umm_predictions(
                exported.features,
                exported.codebook,
                regularization=args.regularization,
                structure=structure,
                confidence_model=cast(ConfidenceModelName, args.confidence_model),
            )
        features_i32, adc_scale = quantize_trials_to_adc(
            exported.features, signed_bits=args.adc_bits, headroom=args.adc_headroom
        )
        fixture = {
            "algorithm": algorithm,
            "dataset": data.dataset,
            "subject": data.subject,
            "classes": int(exported.codebook.shape[0]),
            "feature_count": int(exported.features.shape[1]),
            "epochs_per_trial": int(exported.features.shape[2]),
            "channels": int(exported.n_channels),
            "timepoints": int(exported.n_timepoints),
            "layout": exported.layout,
            "regularization": args.regularization,
            "confidence_model": args.confidence_model
            if algorithm == "cumulative_umm"
            else None,
            "codebook": exported.codebook.astype(np.uint8).tolist(),
            "benchmark_predictions": benchmark_predictions.astype(np.int64).tolist(),
            "benchmark_labels": y_test.astype(np.int64).tolist(),
            "features_f32": exported.features.astype(np.float32).tolist(),
            "features_i32": features_i32.tolist(),
        }
        with temporary_fixture_path(prefix="cvep-umm-benchmark-") as fixture_path:
            rust = maybe_run_fixture_payload(
                rust_binary, fixture, fixture_path=fixture_path
            )
        python_reference_accuracy = float(np.mean(benchmark_predictions == y_test))
        rows.append(
            {
                "algorithm": algorithm,
                "dataset": data.dataset,
                "subject": data.subject,
                "fold_index": fold_idx,
                "folds": folds,
                "classes": fixture["classes"],
                "channels": int(exported.n_channels),
                "target_fs": data.fs,
                "train_window_seconds": data.trial_seconds,
                "requested_window_seconds": requested_window_seconds,
                "window_seconds": window_samples / data.fs,
                "window": int(window_samples),
                "feature_count": fixture["feature_count"],
                "epochs_per_trial": fixture["epochs_per_trial"],
                "epoch_seconds": args.epoch_seconds,
                "epoch_schedule": args.epoch_schedule,
                "lag_seconds": args.lag_seconds,
                "layout": args.layout,
                "trial_demean": args.trial_demean,
                "epoch_demean": args.epoch_demean,
                "train_trials": int(train_idx.shape[0]),
                "test_trials": int(x_test.shape[0]),
                "python_reference_accuracy": python_reference_accuracy,
                "pyntbci_accuracy": python_reference_accuracy,
                "rust_exact_accuracy": None
                if rust is None
                else float(rust["rust_exact_accuracy"]),
                "rust_exact_match_rate": None
                if rust is None
                else float(rust["rust_exact_match_rate"]),
                "rust_fixed_accuracy": None
                if rust is None
                else float(rust["rust_fixed_accuracy"]),
                "rust_fixed_match_rate": None
                if rust is None
                else float(rust["rust_fixed_match_rate"]),
                "quantization_scale": adc_scale,
            }
        )
    return rows


def grouped_summary_rows(results: list[dict[str, Any]]) -> list[dict[str, Any]]:
    return build_group_summary(
        results,
        key_fields=[
            "algorithm",
            "dataset",
            "target_fs",
            "window",
            "window_seconds",
            "requested_window_seconds",
        ],
        metric_fields=["python_reference_accuracy"],
        optional_metric_fields=(
            "rust_exact_accuracy",
            "rust_exact_match_rate",
            "rust_fixed_accuracy",
            "rust_fixed_match_rate",
        ),
    )


def fmt_metric(value: float | None) -> str:
    return "-" if value is None else f"{value:.4f}"


def render_summary(console: Console, rows: list[dict[str, Any]]) -> None:
    render_rich_table(
        console,
        title="UMM vs Rust benchmark summary",
        columns=[
            ("algorithm", "algorithm"),
            ("dataset", "dataset"),
            ("fs", "target_fs"),
            ("window", "requested_window_seconds"),
            ("subjects", "subjects"),
            ("python_ref", "mean_python_reference_accuracy"),
            ("rust_exact", "mean_rust_exact_accuracy"),
            ("exact_match", "mean_rust_exact_match_rate"),
            ("rust_fixed", "mean_rust_fixed_accuracy"),
            ("fixed_match", "mean_rust_fixed_match_rate"),
        ],
        rows=grouped_summary_rows(rows),
        formatters={
            "requested_window_seconds": lambda value: f"{value:.3f}",
            "mean_python_reference_accuracy": lambda value: f"{value:.4f}",
            "mean_rust_exact_accuracy": fmt_metric,
            "mean_rust_exact_match_rate": fmt_metric,
            "mean_rust_fixed_accuracy": fmt_metric,
            "mean_rust_fixed_match_rate": fmt_metric,
        },
    )


def main() -> None:
    args = parse_args()
    validate_target_fs(args.target_fs)
    rust_binary = None if args.skip_rust else build_rust_binary("umm_benchmark")
    console = Console()
    ensure_output_dirs([args.output_json, args.output_csv, args.output_html])
    rows: list[dict[str, Any]] = []
    fold_indices = resolve_fold_indices(args.folds, args.fold_index)
    for dataset in args.datasets:
        subjects = resolve_subjects(dataset, args.subjects, args.max_subjects)
        for subject in subjects:
            data = load_subject(dataset, subject, args.data_dir, args.target_fs)
            window_requests = decode_window_requests(
                data.trial_seconds, args.window_seconds_grid, args.window_step_seconds
            )
            for algorithm in args.algorithms:
                for fold_idx in fold_indices:
                    rows.extend(
                        benchmark_subject_fold_windows(
                            algorithm,
                            data,
                            rust_binary,
                            fold_idx,
                            args.folds,
                            window_requests,
                            args,
                        )
                    )
    payload = {
        "config": {
            "algorithms": args.algorithms,
            "datasets": args.datasets,
            "subjects": args.subjects,
            "max_subjects": args.max_subjects,
            "folds": args.folds,
            "fold_index": fold_indices,
            "target_fs": args.target_fs,
            "window_step_seconds": args.window_step_seconds,
            "window_seconds_grid": args.window_seconds_grid,
            "epoch_seconds": args.epoch_seconds,
            "epoch_schedule": args.epoch_schedule,
            "lag_seconds": args.lag_seconds,
            "layout": args.layout,
            "trial_demean": args.trial_demean,
            "epoch_demean": args.epoch_demean,
            "confidence_model": args.confidence_model,
            "regularization": args.regularization,
            "adc_bits": args.adc_bits,
            "adc_headroom": args.adc_headroom,
        },
        "results": rows,
    }
    write_json_payload(args.output_json, payload)
    args.output_csv.write_text(
        rows_to_csv(
            rows,
            [
                "algorithm",
                "dataset",
                "subject",
                "fold_index",
                "folds",
                "classes",
                "channels",
                "target_fs",
                "train_window_seconds",
                "requested_window_seconds",
                "window_seconds",
                "window",
                "feature_count",
                "epochs_per_trial",
                "epoch_seconds",
                "epoch_schedule",
                "lag_seconds",
                "layout",
                "trial_demean",
                "epoch_demean",
                "train_trials",
                "test_trials",
                "python_reference_accuracy",
                "pyntbci_accuracy",
                "rust_exact_accuracy",
                "rust_exact_match_rate",
                "rust_fixed_accuracy",
                "rust_fixed_match_rate",
            ],
        ),
        encoding="utf-8",
    )
    render_tabular_html(
        args.output_html,
        title="UMM vs Rust Benchmark",
        subtitle="UMM feature parity against Rust.",
        config=payload["config"],
        summary_columns=[
            ("Algorithm", "algorithm"),
            ("Dataset", "dataset"),
            ("fs", "target_fs"),
            ("Requested", "requested_window_seconds"),
            ("Actual", "window_seconds"),
            ("Subjects", "subjects"),
            ("Python", "mean_python_reference_accuracy"),
            ("Rust exact", "mean_rust_exact_accuracy"),
            ("Exact match", "mean_rust_exact_match_rate"),
            ("Rust fixed", "mean_rust_fixed_accuracy"),
            ("Fixed match", "mean_rust_fixed_match_rate"),
        ],
        summary_rows=grouped_summary_rows(rows),
        detail_columns=[
            ("Algorithm", "algorithm"),
            ("Dataset", "dataset"),
            ("Subject", "subject"),
            ("Fold", "fold_index"),
            ("fs", "target_fs"),
            ("Requested", "requested_window_seconds"),
            ("Actual", "window_seconds"),
            ("Features", "feature_count"),
            ("Epochs", "epochs_per_trial"),
            ("Python", "python_reference_accuracy"),
            ("Rust exact", "rust_exact_accuracy"),
            ("Exact match", "rust_exact_match_rate"),
            ("Rust fixed", "rust_fixed_accuracy"),
            ("Fixed match", "rust_fixed_match_rate"),
        ],
        detail_rows=rows,
    )
    render_summary(console, rows)
