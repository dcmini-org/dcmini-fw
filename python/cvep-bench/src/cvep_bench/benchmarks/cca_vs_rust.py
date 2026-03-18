from __future__ import annotations

import argparse
from pathlib import Path
from typing import Any

import numpy as np
from rich.console import Console

from cvep_bench.algorithms.cca_reference import (
    build_cca_encodings,
    cumulative_cca_predictions_pyntbci,
    cumulative_cca_predictions_pyntbci_confidence_gated,
    instantaneous_cca_predictions_pyntbci,
    quantize_trials_to_i32,
)
from cvep_bench.benchmarks.load_planning import loader_trial_seconds_for_algorithm
from cvep_bench.benchmarks.orchestration import (
    BenchmarkDataCache,
    ensure_output_dirs,
    resolve_subjects,
    resolve_window_requests_for_dataset,
)
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
    add_preprocessing_override_args,
    add_profile_arg,
    add_rust_args,
    add_target_fs_args,
    add_window_args,
    resolve_fold_indices,
)
from cvep_bench.datasets.loaders import load_subject, trial_seconds_for_dataset
from cvep_bench.datasets.profiles import (
    benchmark_profile_names,
    resolve_benchmark_profile,
    resolve_encoding_length,
    resolve_event,
    resolve_onset_event,
    resolve_preprocessing_options,
    resolve_target_fs,
)
from cvep_bench.datasets.windowing import (
    slice_windowed_trials_and_stimulus,
    stimulus_to_sample_rate,
)
from cvep_bench.evaluation.splits import fold_slices
from cvep_bench.runtime.binaries import WORKSPACE_ROOT, build_rust_binary
from cvep_bench.runtime.json_fixtures import temporary_fixture_path
from cvep_bench.runtime.runner import maybe_run_fixture_payload


DEFAULT_DATA_DIR = WORKSPACE_ROOT / "crates" / "cvep-decoder" / "data"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    add_profile_arg(parser, choices=benchmark_profile_names())
    add_data_dir_arg(parser, DEFAULT_DATA_DIR)
    add_output_args(parser, output_dir=DEFAULT_DATA_DIR, stem="cca_vs_rust_results")
    parser.add_argument(
        "--algorithms",
        nargs="+",
        choices=["instantaneous_cca", "cumulative_cca"],
        default=["instantaneous_cca", "cumulative_cca"],
    )
    add_dataset_args(parser, default_datasets=["Thielen2021"])
    add_fold_args(parser)
    add_target_fs_args(parser, default=None, include_grid=True)
    add_window_args(parser, default_grid=None, include_step=True)
    parser.add_argument("--encoding-length", type=float, default=None)
    parser.add_argument("--event", type=str, default=None)
    parser.add_argument(
        "--onset-event", action=argparse.BooleanOptionalAction, default=None
    )
    parser.add_argument("--regularization", type=float, default=1.0e-3)
    parser.add_argument(
        "--cumulative-update-mode",
        choices=["naive", "confidence_gated"],
        default="naive",
    )
    parser.add_argument("--cumulative-min-margin", type=float, default=0.05)
    add_adc_args(parser)
    add_preprocessing_override_args(parser)
    parser.add_argument(
        "--thielen2021-source", choices=["raw", "packaged"], default="raw"
    )
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
    stimulus_fs = stimulus_to_sample_rate(
        data.stimulus, presentation_rate=data.presentation_rate, fs=data.fs
    )
    rows: list[dict[str, Any]] = []
    for requested_window_seconds in window_requests_seconds:
        x_test_window, _stimulus_window, window_info = (
            slice_windowed_trials_and_stimulus(
                x_test,
                stimulus_fs,
                data.fs,
                data.fs,
                requested_window_seconds,
                args.drop_first_seconds,
            )
        )
        window_samples = int(window_info["effective_window_samples"])
        encodings = build_cca_encodings(
            stimulus_fs,
            data.fs,
            window_samples,
            event=args.event,
            onset_event=args.onset_event,
            encoding_length=args.encoding_length,
            start_sample=int(window_info["leading_trim_samples"]),
        )
        if algorithm == "instantaneous_cca":
            reference = instantaneous_cca_predictions_pyntbci(
                x_test_window,
                stimulus_fs,
                data.fs,
                event=args.event,
                onset_event=args.onset_event,
                encoding_length=args.encoding_length,
                start_sample=int(window_info["leading_trim_samples"]),
            )
        elif args.cumulative_update_mode == "naive":
            reference = cumulative_cca_predictions_pyntbci(
                x_test_window,
                stimulus_fs,
                data.fs,
                event=args.event,
                onset_event=args.onset_event,
                encoding_length=args.encoding_length,
                start_sample=int(window_info["leading_trim_samples"]),
            )
        else:
            reference = cumulative_cca_predictions_pyntbci_confidence_gated(
                x_test_window,
                stimulus_fs,
                data.fs,
                event=args.event,
                onset_event=args.onset_event,
                encoding_length=args.encoding_length,
                min_margin=args.cumulative_min_margin,
                start_sample=int(window_info["leading_trim_samples"]),
            )
        quantized_trials, _scale = quantize_trials_to_i32(
            x_test_window, signed_bits=args.adc_bits, headroom=args.adc_headroom
        )
        fixture = {
            "algorithm": algorithm,
            "dataset": data.dataset,
            "subject": data.subject,
            "classes": int(data.stimulus.shape[0]),
            "channels": int(data.x.shape[1]),
            "features": int(encodings.shape[1]),
            "window": int(window_samples),
            "encodings": encodings.astype(np.float32).tolist(),
            "benchmark_predictions": reference.predictions.astype(np.int64).tolist(),
            "benchmark_labels": y_test.astype(np.int64).tolist(),
            "trials_i32": quantized_trials.tolist(),
            "regularization": args.regularization,
        }
        with temporary_fixture_path(prefix="cvep-cca-benchmark-") as fixture_path:
            rust = maybe_run_fixture_payload(
                rust_binary, fixture, fixture_path=fixture_path
            )
        rows.append(
            {
                "algorithm": algorithm,
                "dataset": data.dataset,
                "subject": data.subject,
                "fold_index": fold_idx,
                "folds": folds,
                "classes": fixture["classes"],
                "channels": fixture["channels"],
                "profile": args.profile,
                "target_fs": data.fs,
                "band_low": args.band_low,
                "band_high": args.band_high,
                "notch_hz": args.notch_hz,
                "train_window_seconds": data.trial_seconds,
                "requested_window_seconds": requested_window_seconds,
                "window_seconds": float(window_info["nominal_window_seconds"]),
                "effective_window_seconds": float(
                    window_info["effective_window_seconds"]
                ),
                "leading_trim_seconds": float(window_info["leading_trim_seconds"]),
                "window": fixture["window"],
                "feature_count": fixture["features"],
                "train_trials": int(train_idx.shape[0]),
                "test_trials": int(x_test.shape[0]),
                "python_reference_accuracy": float(
                    np.mean(reference.predictions == y_test)
                ),
                "pyntbci_accuracy": float(np.mean(reference.predictions == y_test)),
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
            }
        )
    return rows


def grouped_summary_rows(results: list[dict[str, Any]]) -> list[dict[str, Any]]:
    return build_group_summary(
        results,
        key_fields=[
            "algorithm",
            "dataset",
            "profile",
            "target_fs",
            "window",
            "window_seconds",
            "effective_window_seconds",
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


def render_summary(console: Console, results: list[dict[str, Any]]) -> None:
    render_rich_table(
        console,
        title="CCA Benchmark Summary",
        columns=[
            ("Algorithm", "algorithm"),
            ("Dataset", "dataset"),
            ("Profile", "profile"),
            ("fs", "target_fs"),
            ("Window", "effective_window_seconds"),
            ("Subjects", "subjects"),
            ("Python", "mean_python_reference_accuracy"),
            ("Rust exact", "mean_rust_exact_accuracy"),
            ("Exact match", "mean_rust_exact_match_rate"),
            ("Rust fixed", "mean_rust_fixed_accuracy"),
            ("Fixed match", "mean_rust_fixed_match_rate"),
        ],
        rows=grouped_summary_rows(results),
        formatters={
            "effective_window_seconds": lambda value: f"{value:.3f}",
            "mean_python_reference_accuracy": lambda value: f"{value:.4f}",
            "mean_rust_exact_accuracy": fmt_metric,
            "mean_rust_exact_match_rate": fmt_metric,
            "mean_rust_fixed_accuracy": fmt_metric,
            "mean_rust_fixed_match_rate": fmt_metric,
        },
    )


def main() -> None:
    args = parse_args()
    profile = resolve_benchmark_profile(args.profile)
    preprocessing = resolve_preprocessing_options(
        profile,
        band_low=args.band_low,
        band_high=args.band_high,
        notch_hz=args.notch_hz,
        drop_first_seconds=args.drop_first_seconds,
    )
    args.band_low = preprocessing.band_low
    args.band_high = preprocessing.band_high
    args.notch_hz = preprocessing.notch_hz
    args.drop_first_seconds = preprocessing.drop_first_seconds
    args.event = resolve_event(profile, args.event)
    args.onset_event = resolve_onset_event(profile, args.onset_event)
    args.encoding_length = resolve_encoding_length(profile, args.encoding_length)
    target_fs_grid = args.target_fs_grid or [resolve_target_fs(profile, args.target_fs)]
    rust_binary = None if args.skip_rust else build_rust_binary("cca_benchmark")
    console = Console()
    ensure_output_dirs([args.output_json, args.output_csv, args.output_html])
    fold_indices = resolve_fold_indices(args.folds, args.fold_index)
    results: list[dict[str, Any]] = []
    resolved_window_grid = args.window_seconds_grid
    cache = BenchmarkDataCache(args.data_dir)
    for dataset in args.datasets:
        subjects = resolve_subjects(dataset, args.subjects, args.max_subjects)
        for subject in subjects:
            for target_fs in target_fs_grid:
                full_trial_seconds = trial_seconds_for_dataset(dataset)
                window_requests_seconds, resolved_window_grid = (
                    resolve_window_requests_for_dataset(
                        dataset,
                        profile,
                        args.window_seconds_grid,
                        args.window_step_seconds,
                        full_trial_seconds,
                    )
                )
                for algorithm in args.algorithms:
                    grouped_windows = [window_requests_seconds]
                    if (
                        loader_trial_seconds_for_algorithm(
                            dataset,
                            algorithm,
                            requested_window_seconds=full_trial_seconds,
                        )
                        is not None
                    ):
                        grouped_windows = [
                            [window] for window in window_requests_seconds
                        ]
                    for windows in grouped_windows:
                        load_seconds = (
                            loader_trial_seconds_for_algorithm(
                                dataset, algorithm, requested_window_seconds=windows[0]
                            )
                            if len(windows) == 1 and len(window_requests_seconds) != 1
                            else None
                            if loader_trial_seconds_for_algorithm(
                                dataset,
                                algorithm,
                                requested_window_seconds=full_trial_seconds,
                            )
                            is None
                            else windows[0]
                        )
                        data = cache.get(
                            dataset,
                            subject,
                            target_fs,
                            trial_seconds=load_seconds,
                            preprocessing=preprocessing,
                            thielen2021_source=args.thielen2021_source,
                        )
                        for fold_idx in fold_indices:
                            results.extend(
                                benchmark_subject_fold_windows(
                                    algorithm,
                                    data,
                                    rust_binary,
                                    fold_idx,
                                    args.folds,
                                    windows,
                                    args,
                                )
                            )
    payload = {
        "config": {
            "datasets": args.datasets,
            "profile": profile.name,
            "profile_description": profile.description,
            "subjects": args.subjects,
            "max_subjects": args.max_subjects,
            "folds": args.folds,
            "fold_index": fold_indices,
            "target_fs_grid": target_fs_grid,
            "window_step_seconds": args.window_step_seconds,
            "window_seconds_grid": resolved_window_grid,
            "algorithms": args.algorithms,
            "event": args.event,
            "onset_event": args.onset_event,
            "encoding_length": args.encoding_length,
            "regularization": args.regularization,
            "cumulative_update_mode": args.cumulative_update_mode,
            "cumulative_min_margin": args.cumulative_min_margin,
            "skip_rust": args.skip_rust,
            "thielen2021_source": args.thielen2021_source,
            "preprocessing": {
                "band_low": preprocessing.band_low,
                "band_high": preprocessing.band_high,
                "notch_hz": preprocessing.notch_hz,
                "drop_first_seconds": preprocessing.drop_first_seconds,
            },
        },
        "results": results,
    }
    write_json_payload(args.output_json, payload)
    args.output_csv.write_text(
        rows_to_csv(
            results,
            [
                "algorithm",
                "dataset",
                "subject",
                "fold_index",
                "folds",
                "classes",
                "channels",
                "profile",
                "target_fs",
                "band_low",
                "band_high",
                "notch_hz",
                "train_window_seconds",
                "requested_window_seconds",
                "window_seconds",
                "effective_window_seconds",
                "leading_trim_seconds",
                "window",
                "feature_count",
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
        title="CCA Benchmark",
        subtitle="Zero-training CCA parity against Rust.",
        config=payload["config"],
        summary_columns=[
            ("Algorithm", "algorithm"),
            ("Dataset", "dataset"),
            ("Profile", "profile"),
            ("fs", "target_fs"),
            ("Requested window", "requested_window_seconds"),
            ("Actual window", "effective_window_seconds"),
            ("Subjects", "subjects"),
            ("Python", "mean_python_reference_accuracy"),
            ("Rust exact", "mean_rust_exact_accuracy"),
            ("Exact match", "mean_rust_exact_match_rate"),
            ("Rust fixed", "mean_rust_fixed_accuracy"),
            ("Fixed match", "mean_rust_fixed_match_rate"),
        ],
        summary_rows=grouped_summary_rows(results),
        detail_columns=[
            ("Algorithm", "algorithm"),
            ("Dataset", "dataset"),
            ("Profile", "profile"),
            ("Subject", "subject"),
            ("Fold", "fold_index"),
            ("fs", "target_fs"),
            ("Requested window", "requested_window_seconds"),
            ("Actual window", "effective_window_seconds"),
            ("Features", "feature_count"),
            ("Python", "python_reference_accuracy"),
            ("Rust exact", "rust_exact_accuracy"),
            ("Exact match", "rust_exact_match_rate"),
            ("Rust fixed", "rust_fixed_accuracy"),
            ("Fixed match", "rust_fixed_match_rate"),
        ],
        detail_rows=results,
    )
    render_summary(console, results)
