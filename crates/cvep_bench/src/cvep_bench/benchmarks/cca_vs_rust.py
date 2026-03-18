from __future__ import annotations

import argparse
import html
import json
import tempfile
from pathlib import Path
from typing import Any

import numpy as np
from rich.console import Console
from rich.table import Table

from cvep_bench.algorithms.cca_reference import (
    build_cca_encodings,
    cumulative_cca_predictions_pyntbci,
    cumulative_cca_predictions_pyntbci_confidence_gated,
    instantaneous_cca_predictions_pyntbci,
    quantize_trials_to_i32,
)
from cvep_bench.benchmarks.pyntbci_vs_rust import DEFAULT_DATA_DIR
from cvep_bench.datasets.loaders import (
    load_subject,
    subject_list_for_dataset,
    trial_seconds_for_dataset,
)
from cvep_bench.datasets.profiles import (
    benchmark_profile_names,
    resolve_benchmark_profile,
    resolve_encoding_length,
    resolve_event,
    resolve_onset_event,
    resolve_preprocessing_options,
    resolve_target_fs,
    resolve_window_grid,
)
from cvep_bench.datasets.windows import (
    decode_window_requests,
    fold_slices,
    loader_trial_seconds_for_algorithm,
    slice_windowed_trials_and_stimulus,
    stimulus_to_sample_rate,
)
from cvep_bench.runtime.cargo import build_rust_binary
from cvep_bench.runtime.fixtures import run_rust_fixture


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--profile", choices=benchmark_profile_names(), default="legacy"
    )
    parser.add_argument("--data-dir", type=Path, default=DEFAULT_DATA_DIR)
    parser.add_argument(
        "--output-json",
        type=Path,
        default=DEFAULT_DATA_DIR / "cca_vs_rust_results.json",
    )
    parser.add_argument(
        "--output-csv", type=Path, default=DEFAULT_DATA_DIR / "cca_vs_rust_results.csv"
    )
    parser.add_argument(
        "--output-html",
        type=Path,
        default=DEFAULT_DATA_DIR / "cca_vs_rust_results.html",
    )
    parser.add_argument(
        "--algorithms",
        nargs="+",
        choices=["instantaneous_cca", "cumulative_cca"],
        default=["instantaneous_cca", "cumulative_cca"],
    )
    parser.add_argument("--datasets", nargs="+", default=["Thielen2021"])
    parser.add_argument("--subjects", type=int, nargs="+", default=None)
    parser.add_argument("--max-subjects", type=int, default=None)
    parser.add_argument("--folds", type=int, default=5)
    parser.add_argument("--fold-index", type=int, nargs="+", default=None)
    parser.add_argument("--target-fs", type=int, default=None)
    parser.add_argument("--target-fs-grid", type=int, nargs="+", default=None)
    parser.add_argument("--window-step-seconds", type=float, default=None)
    parser.add_argument("--window-seconds-grid", type=float, nargs="+", default=None)
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
    parser.add_argument("--adc-bits", type=int, default=24)
    parser.add_argument("--adc-headroom", type=float, default=0.95)
    parser.add_argument("--band-low", type=float, default=None)
    parser.add_argument("--band-high", type=float, default=None)
    parser.add_argument("--notch-hz", type=float, default=None)
    parser.add_argument("--drop-first-seconds", type=float, default=None)
    parser.add_argument(
        "--thielen2021-source", choices=["raw", "packaged"], default="raw"
    )
    parser.add_argument("--skip-rust", action="store_true")
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
        rust = None
        if rust_binary is not None:
            with tempfile.TemporaryDirectory(prefix="cvep-cca-benchmark-") as tmp_dir:
                fixture_path = Path(tmp_dir) / "fixture.json"
                fixture_path.write_text(json.dumps(fixture), encoding="utf-8")
                rust = run_rust_fixture(fixture_path, rust_binary)
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


def flatten_results_csv(results: list[dict[str, Any]]) -> str:
    keys = [
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
    ]
    lines = [",".join(keys)]
    for row in results:
        lines.append(
            ",".join("" if row.get(key) is None else str(row[key]) for key in keys)
        )
    return "\n".join(lines) + "\n"


def mean_or_none(values: list[float | None]) -> float | None:
    filtered = [value for value in values if value is not None]
    return None if not filtered else float(np.mean(filtered))


def fmt_metric(value: float | None) -> str:
    return "-" if value is None else f"{value:.4f}"


def grouped_summary_rows(results: list[dict[str, Any]]) -> list[dict[str, Any]]:
    grouped: dict[tuple[str, str, str, int, int], list[dict[str, Any]]] = {}
    for row in results:
        grouped.setdefault(
            (
                row["algorithm"],
                row["dataset"],
                row.get("profile", "legacy"),
                row["target_fs"],
                row["window"],
            ),
            [],
        ).append(row)
    out = []
    for (algorithm, dataset, profile, target_fs, window), rows in sorted(
        grouped.items()
    ):
        out.append(
            {
                "algorithm": algorithm,
                "dataset": dataset,
                "profile": profile,
                "target_fs": target_fs,
                "window": window,
                "window_seconds": rows[0]["window_seconds"],
                "effective_window_seconds": rows[0].get("effective_window_seconds"),
                "requested_window_seconds": rows[0]["requested_window_seconds"],
                "subjects": len({row["subject"] for row in rows}),
                "mean_python_reference_accuracy": float(
                    np.mean([row["python_reference_accuracy"] for row in rows])
                ),
                "mean_rust_exact_accuracy": mean_or_none(
                    [row.get("rust_exact_accuracy") for row in rows]
                ),
                "mean_rust_exact_match_rate": mean_or_none(
                    [row.get("rust_exact_match_rate") for row in rows]
                ),
                "mean_rust_fixed_accuracy": mean_or_none(
                    [row.get("rust_fixed_accuracy") for row in rows]
                ),
                "mean_rust_fixed_match_rate": mean_or_none(
                    [row.get("rust_fixed_match_rate") for row in rows]
                ),
            }
        )
    return out


def render_summary(console: Console, results: list[dict[str, Any]]) -> None:
    table = Table(title="CCA Benchmark Summary")
    for col in [
        "Algorithm",
        "Dataset",
        "Profile",
        "fs",
        "Window",
        "Subjects",
        "Python",
        "Rust exact",
        "Exact match",
        "Rust fixed",
        "Fixed match",
    ]:
        table.add_column(col)
    for row in grouped_summary_rows(results):
        table.add_row(
            row["algorithm"],
            row["dataset"],
            row["profile"],
            str(row["target_fs"]),
            f"{row['effective_window_seconds']:.3f}",
            str(row["subjects"]),
            f"{row['mean_python_reference_accuracy']:.4f}",
            fmt_metric(row["mean_rust_exact_accuracy"]),
            fmt_metric(row["mean_rust_exact_match_rate"]),
            fmt_metric(row["mean_rust_fixed_accuracy"]),
            fmt_metric(row["mean_rust_fixed_match_rate"]),
        )
    console.print(table)


def render_html_report(
    output: Path, config: dict[str, Any], results: list[dict[str, Any]]
) -> None:
    summary = grouped_summary_rows(results)
    summary_rows = "\n".join(
        (
            "<tr>"
            f"<td>{html.escape(row['algorithm'])}</td>"
            f"<td>{html.escape(row['dataset'])}</td>"
            f"<td>{html.escape(row['profile'])}</td>"
            f"<td>{row['target_fs']}</td>"
            f"<td>{row['requested_window_seconds']:.3f}</td>"
            f"<td>{row['effective_window_seconds']:.3f}</td>"
            f"<td>{row['subjects']}</td>"
            f"<td>{row['mean_python_reference_accuracy']:.4f}</td>"
            f"<td>{fmt_metric(row['mean_rust_exact_accuracy'])}</td>"
            f"<td>{fmt_metric(row['mean_rust_exact_match_rate'])}</td>"
            f"<td>{fmt_metric(row['mean_rust_fixed_accuracy'])}</td>"
            f"<td>{fmt_metric(row['mean_rust_fixed_match_rate'])}</td>"
            "</tr>"
        )
        for row in summary
    )
    detail_rows = "\n".join(
        (
            "<tr>"
            f"<td>{html.escape(row['algorithm'])}</td>"
            f"<td>{html.escape(row['dataset'])}</td>"
            f"<td>{html.escape(row['profile'])}</td>"
            f"<td>{row['subject']}</td>"
            f"<td>{row['fold_index']}</td>"
            f"<td>{row['target_fs']}</td>"
            f"<td>{row['requested_window_seconds']:.3f}</td>"
            f"<td>{row['effective_window_seconds']:.3f}</td>"
            f"<td>{row['feature_count']}</td>"
            f"<td>{row['python_reference_accuracy']:.4f}</td>"
            f"<td>{fmt_metric(row['rust_exact_accuracy'])}</td>"
            f"<td>{fmt_metric(row['rust_exact_match_rate'])}</td>"
            f"<td>{fmt_metric(row['rust_fixed_accuracy'])}</td>"
            f"<td>{fmt_metric(row['rust_fixed_match_rate'])}</td>"
            "</tr>"
        )
        for row in results
    )
    output.write_text(
        f"<!doctype html><html lang='en'><head><meta charset='utf-8'><meta name='viewport' content='width=device-width, initial-scale=1'><title>CCA Benchmark</title></head><body><pre>{html.escape(json.dumps(config, indent=2))}</pre><table><thead><tr><th>Algorithm</th><th>Dataset</th><th>Profile</th><th>fs</th><th>Requested window</th><th>Actual window</th><th>Subjects</th><th>Python</th><th>Rust exact</th><th>Exact match</th><th>Rust fixed</th><th>Fixed match</th></tr></thead><tbody>{summary_rows}</tbody></table><table><thead><tr><th>Algorithm</th><th>Dataset</th><th>Profile</th><th>Subject</th><th>Fold</th><th>fs</th><th>Requested window</th><th>Actual window</th><th>Features</th><th>Python</th><th>Rust exact</th><th>Exact match</th><th>Rust fixed</th><th>Fixed match</th></tr></thead><tbody>{detail_rows}</tbody></table></body></html>",
        encoding="utf-8",
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
    results: list[dict[str, Any]] = []
    resolved_window_grid = args.window_seconds_grid
    for dataset in args.datasets:
        subjects = args.subjects or subject_list_for_dataset(dataset)
        if args.max_subjects is not None:
            subjects = subjects[: args.max_subjects]
        for subject in subjects:
            for target_fs in target_fs_grid:
                full_trial_seconds = trial_seconds_for_dataset(dataset)
                resolved_window_grid = resolve_window_grid(
                    profile, dataset, args.window_seconds_grid, args.window_step_seconds
                )
                window_requests_seconds = decode_window_requests(
                    full_trial_seconds, resolved_window_grid, args.window_step_seconds
                )
                data_cache: dict[tuple[str, float | None], Any] = {}
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
                        cache_key = (algorithm, load_seconds)
                        if cache_key not in data_cache:
                            data_cache[cache_key] = load_subject(
                                dataset,
                                subject,
                                args.data_dir,
                                target_fs,
                                trial_seconds=load_seconds,
                                preprocessing=preprocessing,
                                thielen2021_source=args.thielen2021_source,
                            )
                        data = data_cache[cache_key]
                        fold_indices = args.fold_index or list(range(args.folds))
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
            "fold_index": args.fold_index or list(range(args.folds)),
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
    for output in [args.output_json, args.output_csv, args.output_html]:
        output.parent.mkdir(parents=True, exist_ok=True)
    args.output_json.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    args.output_csv.write_text(flatten_results_csv(results), encoding="utf-8")
    render_html_report(args.output_html, payload["config"], results)
    render_summary(console, results)
