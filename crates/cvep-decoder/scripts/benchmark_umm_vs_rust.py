#!/usr/bin/env -S uv run --script
# /// script
# dependencies = [
#   "h5py>=3.16.0",
#   "mne>=1.11.0",
#   "numpy>=2.2.6",
#   "rich>=14.3.3",
#   "scipy>=1.15.3",
# ]
# ///
"""Benchmark corrected UMM decoding against the Rust UMM runtime."""

from __future__ import annotations

import argparse
import html
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
from typing import Any

import numpy as np
from rich.console import Console
from rich.table import Table

from umm_feature_utils import (
    build_umm_features,
    cumulative_umm_predictions,
    instantaneous_umm_predictions,
    make_structure,
)


WORKSPACE_ROOT = Path(__file__).resolve().parents[3]
CRATE_ROOT = Path(__file__).resolve().parents[1]
RAW_BENCHMARK_SCRIPT = CRATE_ROOT / "scripts/benchmark_pyntbci_vs_rust.py"


def load_benchmark_module() -> Any:
    spec = importlib.util.spec_from_file_location("cvep_raw_benchmark", RAW_BENCHMARK_SCRIPT)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"Failed to load benchmark module from {RAW_BENCHMARK_SCRIPT}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data-dir", type=Path, default=CRATE_ROOT / "data")
    parser.add_argument("--output-json", type=Path, default=CRATE_ROOT / "data/umm_vs_rust_results.json")
    parser.add_argument("--output-csv", type=Path, default=CRATE_ROOT / "data/umm_vs_rust_results.csv")
    parser.add_argument("--output-html", type=Path, default=CRATE_ROOT / "data/umm_vs_rust_results.html")
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
        "--layout",
        choices=["channel_prime", "time_prime"],
        default="channel_prime",
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
    return parser.parse_args()


def quantize_tensor_to_i32(
    x: np.ndarray,
    signed_bits: int,
    headroom: float,
) -> tuple[np.ndarray, float]:
    adc_peak = float((1 << (signed_bits - 1)) - 1)
    data_peak = float(np.max(np.abs(x)))
    scale = 1.0 if data_peak == 0.0 else (adc_peak * headroom) / data_peak
    quantized = np.rint(x * scale).clip(-adc_peak - 1.0, adc_peak).astype(np.int32)
    return quantized, scale


def build_rust_binary() -> Path:
    subprocess.run(
        [
            "cargo",
            "build",
            "--quiet",
            "-p",
            "cvep-decoder",
            "--features",
            "host-tools",
            "--bin",
            "umm_benchmark",
        ],
        check=True,
        cwd=WORKSPACE_ROOT,
    )
    return WORKSPACE_ROOT / "target" / "debug" / "umm_benchmark"


def run_rust_fixture(fixture_path: Path, rust_binary: Path) -> dict[str, Any]:
    result = subprocess.run(
        [str(rust_binary), str(fixture_path)],
        check=True,
        capture_output=True,
        text=True,
    )
    return json.loads(result.stdout)


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
        fields = []
        for key in keys:
            value = row.get(key)
            fields.append("" if value is None else str(value))
        lines.append(",".join(fields))
    return "\n".join(lines) + "\n"


def grouped_summary_rows(results: list[dict[str, Any]]) -> list[dict[str, Any]]:
    grouped: dict[tuple[str, str, int, int], list[dict[str, Any]]] = {}
    for row in results:
        grouped.setdefault(
            (row["algorithm"], row["dataset"], row["target_fs"], row["window"]),
            [],
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
                    np.mean([row["rust_exact_accuracy"] for row in rows])
                ),
                "mean_rust_exact_match_rate": float(
                    np.mean([row["rust_exact_match_rate"] for row in rows])
                ),
                "mean_rust_fixed_accuracy": float(
                    np.mean([row["rust_fixed_accuracy"] for row in rows])
                ),
                "mean_rust_fixed_match_rate": float(
                    np.mean([row["rust_fixed_match_rate"] for row in rows])
                ),
            }
        )
    return summaries


def render_html_report(
    output: Path,
    config: dict[str, Any],
    results: list[dict[str, Any]],
) -> None:
    summary = grouped_summary_rows(results)
    summary_rows = "\n".join(
        (
            "<tr>"
            f"<td>{html.escape(row['algorithm'])}</td>"
            f"<td>{html.escape(row['dataset'])}</td>"
            f"<td>{row['target_fs']}</td>"
            f"<td>{row['requested_window_seconds']:.3f}</td>"
            f"<td>{row['window_seconds']:.3f}</td>"
            f"<td>{row['subjects']}</td>"
            f"<td>{row['mean_python_reference_accuracy']:.4f}</td>"
            f"<td>{row['mean_rust_exact_accuracy']:.4f}</td>"
            f"<td>{row['mean_rust_exact_match_rate']:.4f}</td>"
            f"<td>{row['mean_rust_fixed_accuracy']:.4f}</td>"
            f"<td>{row['mean_rust_fixed_match_rate']:.4f}</td>"
            "</tr>"
        )
        for row in summary
    )
    detail_rows = "\n".join(
        (
            "<tr>"
            f"<td>{html.escape(row['algorithm'])}</td>"
            f"<td>{html.escape(row['dataset'])}</td>"
            f"<td>{row['subject']}</td>"
            f"<td>{row['fold_index']}</td>"
            f"<td>{row['target_fs']}</td>"
            f"<td>{row['requested_window_seconds']:.3f}</td>"
            f"<td>{row['window_seconds']:.3f}</td>"
            f"<td>{row['feature_count']}</td>"
            f"<td>{row['epochs_per_trial']}</td>"
            f"<td>{row['python_reference_accuracy']:.4f}</td>"
            f"<td>{row['rust_exact_accuracy']:.4f}</td>"
            f"<td>{row['rust_exact_match_rate']:.4f}</td>"
            f"<td>{row['rust_fixed_accuracy']:.4f}</td>"
            f"<td>{row['rust_fixed_match_rate']:.4f}</td>"
            "</tr>"
        )
        for row in results
    )
    document = f"""<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>UMM vs Rust Benchmark</title>
  <style>
    :root {{
      --bg: #f6f2ea;
      --panel: #fffdf8;
      --ink: #1d2935;
      --muted: #5b6875;
      --line: #d8cfbf;
      --accent: #0d9488;
    }}
    body {{ margin: 0; background: var(--bg); color: var(--ink); font-family: Georgia, serif; }}
    main {{ max-width: 1200px; margin: 0 auto; padding: 28px 18px 40px; }}
    .card {{ background: var(--panel); border: 1px solid var(--line); border-radius: 16px; padding: 18px; margin-bottom: 18px; }}
    table {{ width: 100%; border-collapse: collapse; font-size: 0.95rem; }}
    th, td {{ padding: 10px 8px; border-bottom: 1px solid var(--line); text-align: left; }}
    th {{ color: var(--muted); text-transform: uppercase; letter-spacing: 0.06em; font-size: 0.75rem; }}
    pre {{ overflow-x: auto; background: #f6f2ea; padding: 12px; border-radius: 12px; }}
  </style>
</head>
<body>
  <main>
    <div class="card">
      <h1>UMM vs Rust Benchmark</h1>
      <p>Python UMM reference against the Rust UMM runtime using corrected stimulus-locked feature timing.</p>
      <pre>{html.escape(json.dumps(config, indent=2))}</pre>
    </div>
    <div class="card">
      <h2>Summary</h2>
      <table>
        <thead><tr><th>Algorithm</th><th>Dataset</th><th>FS</th><th>Requested window</th><th>Window</th><th>Subjects</th><th>Python ref</th><th>Rust exact</th><th>Exact match</th><th>Rust fixed</th><th>Fixed match</th></tr></thead>
        <tbody>{summary_rows}</tbody>
      </table>
    </div>
    <div class="card">
      <h2>Details</h2>
      <table>
        <thead><tr><th>Algorithm</th><th>Dataset</th><th>Subject</th><th>Fold</th><th>FS</th><th>Requested window</th><th>Window</th><th>Features</th><th>Epochs</th><th>Python ref</th><th>Rust exact</th><th>Exact match</th><th>Rust fixed</th><th>Fixed match</th></tr></thead>
        <tbody>{detail_rows}</tbody>
      </table>
    </div>
  </main>
</body>
</html>
"""
    output.write_text(document, encoding="utf-8")


def benchmark_subject_fold_windows(
    algorithm: str,
    data: Any,
    benchmark: Any,
    rust_binary: Path,
    fold_idx: int,
    folds: int,
    window_requests_seconds: list[float],
    epoch_seconds: float,
    epoch_schedule: str,
    lag_seconds: float,
    layout: str,
    trial_demean: bool,
    epoch_demean: bool,
    regularization: float,
    confidence_model: str,
    adc_bits: int,
    adc_headroom: float,
) -> list[dict[str, Any]]:
    fold_parts = benchmark.fold_slices(data.x.shape[0], folds)
    test_idx = fold_parts[fold_idx]
    train_idx = np.concatenate(
        [part for idx, part in enumerate(fold_parts) if idx != fold_idx]
    )
    x_test = data.x[test_idx]
    y_test = data.y[test_idx]
    x_train = data.x[train_idx]

    rows: list[dict[str, Any]] = []
    for requested_window_seconds in window_requests_seconds:
        requested_samples = benchmark.seconds_to_samples(
            requested_window_seconds,
            data.fs,
        )
        window_samples = min(requested_samples, data.x.shape[2])
        actual_window_seconds = window_samples / data.fs
        x_test_window = x_test[:, :, :window_samples]
        exported = build_umm_features(
            trials=x_test_window,
            stimulus=data.stimulus,
            fs=data.fs,
            presentation_rate=data.presentation_rate,
            epoch_seconds=epoch_seconds,
            layout=layout,
            epoch_schedule=epoch_schedule,
            response_lag_seconds=lag_seconds,
            trial_demean=trial_demean,
            epoch_demean=epoch_demean,
        )
        structure = make_structure(exported)

        if algorithm == "instantaneous_umm":
            benchmark_predictions, _ = instantaneous_umm_predictions(
                exported.features,
                exported.codebook,
                regularization=regularization,
                structure=structure,
            )
        elif algorithm == "cumulative_umm":
            benchmark_predictions, _, _ = cumulative_umm_predictions(
                exported.features,
                exported.codebook,
                regularization=regularization,
                structure=structure,
                confidence_model=confidence_model,
            )
        else:
            raise ValueError(f"Unsupported UMM algorithm {algorithm}")

        features_i32, adc_scale = quantize_tensor_to_i32(
            exported.features,
            signed_bits=adc_bits,
            headroom=adc_headroom,
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
            "regularization": regularization,
            "confidence_model": confidence_model if algorithm == "cumulative_umm" else None,
            "codebook": exported.codebook.astype(np.uint8).tolist(),
            "benchmark_predictions": benchmark_predictions.astype(np.int64).tolist(),
            "benchmark_labels": y_test.astype(np.int64).tolist(),
            "features_f32": exported.features.astype(np.float32).tolist(),
            "features_i32": features_i32.tolist(),
        }

        with tempfile.TemporaryDirectory(prefix="cvep-umm-benchmark-") as tmp_dir:
            fixture_path = Path(tmp_dir) / "fixture.json"
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
                "window_seconds": actual_window_seconds,
                "window": int(window_samples),
                "feature_count": fixture["feature_count"],
                "epochs_per_trial": fixture["epochs_per_trial"],
                "epoch_seconds": epoch_seconds,
                "epoch_schedule": epoch_schedule,
                "lag_seconds": lag_seconds,
                "layout": layout,
                "trial_demean": trial_demean,
                "epoch_demean": epoch_demean,
                "train_trials": int(x_train.shape[0]),
                "test_trials": int(x_test.shape[0]),
                "python_reference_accuracy": python_reference_accuracy,
                "pyntbci_accuracy": python_reference_accuracy,
                "rust_exact_accuracy": float(rust["rust_exact_accuracy"]),
                "rust_exact_match_rate": float(rust["rust_exact_match_rate"]),
                "rust_fixed_accuracy": float(rust["rust_fixed_accuracy"]),
                "rust_fixed_match_rate": float(rust["rust_fixed_match_rate"]),
                "quantization_scale": adc_scale,
            }
        )
    return rows


def main() -> None:
    args = parse_args()
    benchmark = load_benchmark_module()
    benchmark.validate_target_fs(args.target_fs)
    rust_binary = build_rust_binary()
    console = Console()

    rows: list[dict[str, Any]] = []
    for dataset in args.datasets:
        subjects = args.subjects or benchmark.subject_list_for_dataset(dataset)
        if args.max_subjects is not None:
            subjects = subjects[: args.max_subjects]
        fold_indices = args.fold_index or list(range(args.folds))
        for subject in subjects:
            data = benchmark.load_subject(dataset, subject, args.data_dir, args.target_fs)
            window_requests = benchmark.decode_window_requests(
                data.trial_seconds,
                args.window_seconds_grid,
                args.window_step_seconds,
            )
            for algorithm in args.algorithms:
                for fold_idx in fold_indices:
                    subject_rows = benchmark_subject_fold_windows(
                        algorithm=algorithm,
                        data=data,
                        benchmark=benchmark,
                        rust_binary=rust_binary,
                        fold_idx=fold_idx,
                        folds=args.folds,
                        window_requests_seconds=window_requests,
                        epoch_seconds=args.epoch_seconds,
                        epoch_schedule=args.epoch_schedule,
                        lag_seconds=args.lag_seconds,
                        layout=args.layout,
                        trial_demean=args.trial_demean,
                        epoch_demean=args.epoch_demean,
                        regularization=args.regularization,
                        confidence_model=args.confidence_model,
                        adc_bits=args.adc_bits,
                        adc_headroom=args.adc_headroom,
                    )
                    rows.extend(subject_rows)
                    console.print(
                        f"[blue]umm-vs-rust[/blue] dataset={dataset} subject={subject} "
                        f"algorithm={algorithm} fold={fold_idx}"
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
            "source_note": (
                "The column `pyntbci_accuracy` is kept only for compatibility with the "
                "existing benchmark CSV shape. For UMM it is the Python reference "
                "accuracy from the host-side implementation, not a PyntBCI score."
            ),
        },
        "results": rows,
    }
    args.output_json.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    args.output_csv.write_text(flatten_results_csv(rows), encoding="utf-8")
    render_html_report(args.output_html, payload["config"], rows)

    summary = grouped_summary_rows(rows)
    table = Table(title="UMM vs Rust benchmark summary")
    table.add_column("algorithm")
    table.add_column("dataset")
    table.add_column("fs")
    table.add_column("window")
    table.add_column("subjects")
    table.add_column("python_ref")
    table.add_column("rust_exact")
    table.add_column("exact_match")
    table.add_column("rust_fixed")
    table.add_column("fixed_match")
    for row in summary:
        table.add_row(
            row["algorithm"],
            row["dataset"],
            str(row["target_fs"]),
            f"{row['requested_window_seconds']:.3f}",
            str(row["subjects"]),
            f"{row['mean_python_reference_accuracy']:.4f}",
            f"{row['mean_rust_exact_accuracy']:.4f}",
            f"{row['mean_rust_exact_match_rate']:.4f}",
            f"{row['mean_rust_fixed_accuracy']:.4f}",
            f"{row['mean_rust_fixed_match_rate']:.4f}",
        )
    Console().print(table)


if __name__ == "__main__":
    main()
