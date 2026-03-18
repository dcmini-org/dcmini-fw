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

from cvep_bench.algorithms.pyntbci_models import quantize_trials_to_adc
from cvep_bench.algorithms.umm_features import (
    build_umm_features,
    cumulative_umm_predictions,
    instantaneous_umm_predictions,
    make_structure,
)
from cvep_bench.benchmarks.pyntbci_vs_rust import DEFAULT_DATA_DIR
from cvep_bench.datasets.loaders import (
    load_subject,
    subject_list_for_dataset,
    validate_target_fs,
)
from cvep_bench.datasets.windows import decode_window_requests, fold_slices
from cvep_bench.runtime.cargo import build_rust_binary
from cvep_bench.runtime.fixtures import run_rust_fixture


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data-dir", type=Path, default=DEFAULT_DATA_DIR)
    parser.add_argument(
        "--output-json",
        type=Path,
        default=DEFAULT_DATA_DIR / "umm_vs_rust_results.json",
    )
    parser.add_argument(
        "--output-csv", type=Path, default=DEFAULT_DATA_DIR / "umm_vs_rust_results.csv"
    )
    parser.add_argument(
        "--output-html",
        type=Path,
        default=DEFAULT_DATA_DIR / "umm_vs_rust_results.html",
    )
    parser.add_argument(
        "--algorithms",
        nargs="+",
        choices=["instantaneous_umm", "cumulative_umm"],
        default=["instantaneous_umm", "cumulative_umm"],
    )
    parser.add_argument("--datasets", nargs="+", default=["Thielen2021"])
    parser.add_argument("--subjects", type=int, nargs="+", default=None)
    parser.add_argument("--max-subjects", type=int, default=None)
    parser.add_argument("--folds", type=int, default=5)
    parser.add_argument("--fold-index", type=int, nargs="+", default=None)
    parser.add_argument("--target-fs", type=int, default=250)
    parser.add_argument("--window-step-seconds", type=float, default=None)
    parser.add_argument("--window-seconds-grid", type=float, nargs="+", default=None)
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
    parser.add_argument("--adc-bits", type=int, default=24)
    parser.add_argument("--adc-headroom", type=float, default=0.95)
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
    x_train = data.x[train_idx]
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
            args.layout,
            args.epoch_schedule,
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
                confidence_model=args.confidence_model,
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
        rust = None
        if rust_binary is not None:
            with tempfile.TemporaryDirectory(prefix="cvep-umm-benchmark-") as tmpdir:
                fixture_path = Path(tmpdir) / "fixture.json"
                fixture_path.write_text(json.dumps(fixture), encoding="utf-8")
                rust = run_rust_fixture(fixture_path, rust_binary)
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
                "train_trials": int(x_train.shape[0]),
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


def flatten_results_csv(results: list[dict[str, Any]]) -> str:
    keys = [
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
    ]
    lines = [",".join(keys)]
    for row in results:
        lines.append(
            ",".join("" if row.get(key) is None else str(row[key]) for key in keys)
        )
    return "\n".join(lines) + "\n"


def grouped_summary_rows(results: list[dict[str, Any]]) -> list[dict[str, Any]]:
    grouped: dict[tuple[str, str, int, int], list[dict[str, Any]]] = {}
    for row in results:
        grouped.setdefault(
            (row["algorithm"], row["dataset"], row["target_fs"], row["window"]), []
        ).append(row)
    summaries = []
    for (algorithm, dataset, target_fs, window), rows in sorted(grouped.items()):
        summaries.append(
            {
                "algorithm": algorithm,
                "dataset": dataset,
                "target_fs": target_fs,
                "window": window,
                "window_seconds": rows[0]["window_seconds"],
                "requested_window_seconds": rows[0]["requested_window_seconds"],
                "subjects": len({row["subject"] for row in rows}),
                "mean_python_reference_accuracy": float(
                    np.mean([row["python_reference_accuracy"] for row in rows])
                ),
                "mean_rust_exact_accuracy": float(
                    np.mean(
                        [
                            row["rust_exact_accuracy"]
                            for row in rows
                            if row["rust_exact_accuracy"] is not None
                        ]
                    )
                )
                if any(row["rust_exact_accuracy"] is not None for row in rows)
                else None,
                "mean_rust_exact_match_rate": float(
                    np.mean(
                        [
                            row["rust_exact_match_rate"]
                            for row in rows
                            if row["rust_exact_match_rate"] is not None
                        ]
                    )
                )
                if any(row["rust_exact_match_rate"] is not None for row in rows)
                else None,
                "mean_rust_fixed_accuracy": float(
                    np.mean(
                        [
                            row["rust_fixed_accuracy"]
                            for row in rows
                            if row["rust_fixed_accuracy"] is not None
                        ]
                    )
                )
                if any(row["rust_fixed_accuracy"] is not None for row in rows)
                else None,
                "mean_rust_fixed_match_rate": float(
                    np.mean(
                        [
                            row["rust_fixed_match_rate"]
                            for row in rows
                            if row["rust_fixed_match_rate"] is not None
                        ]
                    )
                )
                if any(row["rust_fixed_match_rate"] is not None for row in rows)
                else None,
            }
        )
    return summaries


def fmt_metric(value: float | None) -> str:
    return "-" if value is None else f"{value:.4f}"


def render_html_report(
    output: Path, config: dict[str, Any], results: list[dict[str, Any]]
) -> None:
    summary = grouped_summary_rows(results)
    summary_rows = "\n".join(
        (
            "<tr>"
            f"<td>{html.escape(row['algorithm'])}</td><td>{html.escape(row['dataset'])}</td><td>{row['target_fs']}</td><td>{row['requested_window_seconds']:.3f}</td><td>{row['window_seconds']:.3f}</td><td>{row['subjects']}</td><td>{row['mean_python_reference_accuracy']:.4f}</td><td>{fmt_metric(row['mean_rust_exact_accuracy'])}</td><td>{fmt_metric(row['mean_rust_exact_match_rate'])}</td><td>{fmt_metric(row['mean_rust_fixed_accuracy'])}</td><td>{fmt_metric(row['mean_rust_fixed_match_rate'])}</td>"
            "</tr>"
        )
        for row in summary
    )
    output.write_text(
        f"<!doctype html><html lang='en'><body><pre>{html.escape(json.dumps(config, indent=2))}</pre><table><tbody>{summary_rows}</tbody></table></body></html>",
        encoding="utf-8",
    )


def main() -> None:
    args = parse_args()
    validate_target_fs(args.target_fs)
    rust_binary = None if args.skip_rust else build_rust_binary("umm_benchmark")
    console = Console()
    rows: list[dict[str, Any]] = []
    for dataset in args.datasets:
        subjects = args.subjects or subject_list_for_dataset(dataset)
        if args.max_subjects is not None:
            subjects = subjects[: args.max_subjects]
        fold_indices = args.fold_index or list(range(args.folds))
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
            "fold_index": args.fold_index,
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
    for output in [args.output_json, args.output_csv, args.output_html]:
        output.parent.mkdir(parents=True, exist_ok=True)
    args.output_json.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    args.output_csv.write_text(flatten_results_csv(rows), encoding="utf-8")
    render_html_report(args.output_html, payload["config"], rows)
    summary = grouped_summary_rows(rows)
    table = Table(title="UMM vs Rust benchmark summary")
    for col in [
        "algorithm",
        "dataset",
        "fs",
        "window",
        "subjects",
        "python_ref",
        "rust_exact",
        "exact_match",
        "rust_fixed",
        "fixed_match",
    ]:
        table.add_column(col)
    for row in summary:
        table.add_row(
            row["algorithm"],
            row["dataset"],
            str(row["target_fs"]),
            f"{row['requested_window_seconds']:.3f}",
            str(row["subjects"]),
            f"{row['mean_python_reference_accuracy']:.4f}",
            fmt_metric(row["mean_rust_exact_accuracy"]),
            fmt_metric(row["mean_rust_exact_match_rate"]),
            fmt_metric(row["mean_rust_fixed_accuracy"]),
            fmt_metric(row["mean_rust_fixed_match_rate"]),
        )
    console.print(table)
