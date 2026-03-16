#!/usr/bin/env -S uv run --script
# /// script
# dependencies = [
#   "h5py>=3.16.0",
#   "mne>=1.11.0",
#   "numpy>=2.2.6",
#   "pyntbci>=1.8.3",
#   "rich>=14.3.3",
#   "scipy>=1.15.3",
# ]
# ///
"""Benchmark zero-training CCA against the Rust CCA runtime."""

from __future__ import annotations

import argparse
import html
import importlib.util
import json
from pathlib import Path
import time
import subprocess
import sys
import tempfile
from typing import Any

import numpy as np
from rich.console import Console
from rich.table import Table

from cca_reference_utils import (
    build_cca_encodings,
    cumulative_cca_predictions_pyntbci,
    instantaneous_cca_predictions_pyntbci,
    quantize_trials_to_i32,
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
    parser.add_argument("--output-json", type=Path, default=CRATE_ROOT / "data/cca_vs_rust_results.json")
    parser.add_argument("--output-csv", type=Path, default=CRATE_ROOT / "data/cca_vs_rust_results.csv")
    parser.add_argument("--output-html", type=Path, default=CRATE_ROOT / "data/cca_vs_rust_results.html")
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
    parser.add_argument("--target-fs", type=int, default=250)
    parser.add_argument("--target-fs-grid", type=int, nargs="+", default=None)
    parser.add_argument("--window-step-seconds", type=float, default=None)
    parser.add_argument("--window-seconds-grid", type=float, nargs="+", default=None)
    parser.add_argument("--encoding-length", type=float, default=0.3)
    parser.add_argument("--event", type=str, default="refe")
    parser.add_argument("--onset-event", action="store_true")
    parser.add_argument("--regularization", type=float, default=1.0e-3)
    parser.add_argument("--adc-bits", type=int, default=24)
    parser.add_argument("--adc-headroom", type=float, default=0.95)
    return parser.parse_args()


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
            "cca_benchmark",
        ],
        check=True,
        cwd=WORKSPACE_ROOT,
    )
    return WORKSPACE_ROOT / "target" / "debug" / "cca_benchmark"


def run_rust_fixture(fixture_path: Path, rust_binary: Path) -> dict[str, Any]:
    attempts = 0
    while True:
        attempts += 1
        result = subprocess.run(
            [str(rust_binary), str(fixture_path)],
            check=False,
            capture_output=True,
            text=True,
        )
        if result.returncode == 0:
            return json.loads(result.stdout)
        if attempts >= 2:
            raise RuntimeError(
                f"cca_benchmark failed for {fixture_path} with code {result.returncode}\n"
                f"stdout:\n{result.stdout}\n\nstderr:\n{result.stderr}"
            )
        time.sleep(0.1)


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
        lines.append(",".join("" if row.get(key) is None else str(row[key]) for key in keys))
    return "\n".join(lines) + "\n"


def grouped_summary_rows(results: list[dict[str, Any]]) -> list[dict[str, Any]]:
    grouped: dict[tuple[str, str, int, int], list[dict[str, Any]]] = {}
    for row in results:
        grouped.setdefault(
            (row["algorithm"], row["dataset"], row["target_fs"], row["window"]),
            [],
        ).append(row)

    out = []
    for (algorithm, dataset, target_fs, window), rows in sorted(grouped.items()):
        out.append(
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
    return out


def render_html_report(output: Path, config: dict[str, Any], results: list[dict[str, Any]]) -> None:
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
  <title>CCA Benchmark</title>
  <style>
    :root {{
      --bg: #f6f2ea;
      --panel: #fffdf8;
      --ink: #1d2935;
      --muted: #5b6875;
      --line: #d8cfbf;
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
      <h1>CCA Benchmark</h1>
      <p>Zero-training CCA parity against the Rust runtime.</p>
      <pre>{html.escape(json.dumps(config, indent=2))}</pre>
    </div>
    <div class="card">
      <h2>Summary</h2>
      <table>
        <thead><tr><th>Algorithm</th><th>Dataset</th><th>fs</th><th>Requested window</th><th>Actual window</th><th>Subjects</th><th>Python</th><th>Rust exact</th><th>Exact match</th><th>Rust fixed</th><th>Fixed match</th></tr></thead>
        <tbody>{summary_rows}</tbody>
      </table>
    </div>
    <div class="card">
      <h2>Details</h2>
      <table>
        <thead><tr><th>Algorithm</th><th>Dataset</th><th>Subject</th><th>Fold</th><th>fs</th><th>Requested window</th><th>Actual window</th><th>Features</th><th>Python</th><th>Rust exact</th><th>Exact match</th><th>Rust fixed</th><th>Fixed match</th></tr></thead>
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
    args: argparse.Namespace,
) -> list[dict[str, Any]]:
    fold_parts = benchmark.fold_slices(data.x.shape[0], folds)
    test_idx = fold_parts[fold_idx]
    train_idx = np.concatenate([part for idx, part in enumerate(fold_parts) if idx != fold_idx])
    x_test = data.x[test_idx]
    y_test = data.y[test_idx]

    rows: list[dict[str, Any]] = []
    for requested_window_seconds in window_requests_seconds:
        requested_samples = benchmark.seconds_to_samples(requested_window_seconds, data.fs)
        window_samples = min(requested_samples, data.x.shape[2])
        actual_window_seconds = window_samples / data.fs
        x_test_window = x_test[:, :, :window_samples]
        encodings = build_cca_encodings(
            data.stimulus,
            data.fs,
            window_samples,
            event=args.event,
            onset_event=args.onset_event,
            encoding_length=args.encoding_length,
        )
        if algorithm == "instantaneous_cca":
            reference = instantaneous_cca_predictions_pyntbci(
                x_test_window,
                data.stimulus,
                data.fs,
                event=args.event,
                onset_event=args.onset_event,
                encoding_length=args.encoding_length,
            )
        elif algorithm == "cumulative_cca":
            reference = cumulative_cca_predictions_pyntbci(
                x_test_window,
                data.stimulus,
                data.fs,
                event=args.event,
                onset_event=args.onset_event,
                encoding_length=args.encoding_length,
            )
        else:
            raise ValueError(f"Unsupported algorithm {algorithm}")

        quantized_trials, _ = quantize_trials_to_i32(
            x_test_window,
            signed_bits=args.adc_bits,
            headroom=args.adc_headroom,
        )
        fixture = {
            "algorithm": algorithm,
            "dataset": data.dataset,
            "subject": data.subject,
            "classes": int(encodings.shape[0]),
            "channels": int(x_test_window.shape[1]),
            "features": int(encodings.shape[1]),
            "window": int(window_samples),
            "regularization": float(args.regularization),
            "encodings": encodings.astype(np.float32).tolist(),
            "benchmark_predictions": reference.predictions.astype(np.int64).tolist(),
            "benchmark_labels": y_test.astype(np.int64).tolist(),
            "trials_f32": x_test_window.astype(np.float32).tolist(),
            "trials_i32": quantized_trials.tolist(),
        }
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
                "target_fs": data.fs,
                "train_window_seconds": data.trial_seconds,
                "requested_window_seconds": requested_window_seconds,
                "window_seconds": actual_window_seconds,
                "window": fixture["window"],
                "feature_count": fixture["features"],
                "train_trials": int(train_idx.shape[0]),
                "test_trials": int(x_test.shape[0]),
                "python_reference_accuracy": float(np.mean(reference.predictions == y_test)),
                "pyntbci_accuracy": float(np.mean(reference.predictions == y_test)),
                "rust_exact_accuracy": float(rust["rust_exact_accuracy"]),
                "rust_exact_match_rate": float(rust["rust_exact_match_rate"]),
                "rust_fixed_accuracy": float(rust["rust_fixed_accuracy"]),
                "rust_fixed_match_rate": float(rust["rust_fixed_match_rate"]),
            }
        )
    return rows


def main() -> None:
    args = parse_args()
    benchmark = load_benchmark_module()
    target_fs_grid = args.target_fs_grid or [args.target_fs]
    fold_indices = args.fold_index or list(range(args.folds))
    rust_binary = build_rust_binary()
    console = Console()

    results: list[dict[str, Any]] = []
    for dataset in args.datasets:
        subjects = args.subjects or benchmark.subject_list_for_dataset(dataset)
        if args.max_subjects is not None:
            subjects = subjects[: args.max_subjects]
        for target_fs in target_fs_grid:
            benchmark.validate_target_fs(target_fs)
            for subject in subjects:
                data = benchmark.load_subject(dataset, subject, args.data_dir, target_fs)
                window_requests_seconds = benchmark.decode_window_requests(
                    data.trial_seconds,
                    args.window_seconds_grid,
                    args.window_step_seconds,
                )
                for fold_idx in fold_indices:
                    for algorithm in args.algorithms:
                        console.print(
                            f"[blue]cca[/blue] dataset={dataset} subject={subject} "
                            f"fold={fold_idx} target_fs={target_fs} algorithm={algorithm}"
                        )
                        results.extend(
                            benchmark_subject_fold_windows(
                                algorithm,
                                data,
                                benchmark,
                                rust_binary,
                                fold_idx,
                                args.folds,
                                window_requests_seconds,
                                args,
                            )
                        )

    config = {
        "datasets": args.datasets,
        "subjects": args.subjects,
        "max_subjects": args.max_subjects,
        "folds": args.folds,
        "fold_index": fold_indices,
        "target_fs_grid": target_fs_grid,
        "window_step_seconds": args.window_step_seconds,
        "window_seconds_grid": args.window_seconds_grid,
        "algorithms": args.algorithms,
        "event": args.event,
        "onset_event": args.onset_event,
        "encoding_length": args.encoding_length,
        "regularization": args.regularization,
    }
    args.output_json.write_text(
        json.dumps({"config": config, "results": results}, indent=2) + "\n",
        encoding="utf-8",
    )
    args.output_csv.write_text(flatten_results_csv(results), encoding="utf-8")
    render_html_report(args.output_html, config, results)

    summary = grouped_summary_rows(results)
    table = Table(title="CCA Benchmark Summary")
    for column in ["Algorithm", "Dataset", "fs", "Window", "Subjects", "Python", "Rust exact", "Exact match", "Rust fixed", "Fixed match"]:
        table.add_column(column)
    for row in summary:
        table.add_row(
            row["algorithm"],
            row["dataset"],
            str(row["target_fs"]),
            f"{row['window_seconds']:.3f}",
            str(row["subjects"]),
            f"{row['mean_python_reference_accuracy']:.4f}",
            f"{row['mean_rust_exact_accuracy']:.4f}",
            f"{row['mean_rust_exact_match_rate']:.4f}",
            f"{row['mean_rust_fixed_accuracy']:.4f}",
            f"{row['mean_rust_fixed_match_rate']:.4f}",
        )
    console.print(table)


if __name__ == "__main__":
    main()
