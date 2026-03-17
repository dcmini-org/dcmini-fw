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
    cumulative_cca_predictions_pyntbci_confidence_gated,
    instantaneous_cca_predictions_pyntbci,
    quantize_trials_to_i32,
)


WORKSPACE_ROOT = Path(__file__).resolve().parents[3]
CRATE_ROOT = Path(__file__).resolve().parents[1]
RAW_BENCHMARK_SCRIPT = CRATE_ROOT / "scripts/benchmark_pyntbci_vs_rust.py"


def load_benchmark_module() -> Any:
    spec = importlib.util.spec_from_file_location(
        "cvep_raw_benchmark", RAW_BENCHMARK_SCRIPT
    )
    if spec is None or spec.loader is None:
        raise RuntimeError(
            f"Failed to load benchmark module from {RAW_BENCHMARK_SCRIPT}"
        )
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--profile",
        choices=[
            "legacy",
            "matched_embedded_125",
            "matched_diagnostic_125",
            "matched_onset_aware_125",
            "literature_oriented_125",
        ],
        default="legacy",
    )
    parser.add_argument("--data-dir", type=Path, default=CRATE_ROOT / "data")
    parser.add_argument(
        "--output-json", type=Path, default=CRATE_ROOT / "data/cca_vs_rust_results.json"
    )
    parser.add_argument(
        "--output-csv", type=Path, default=CRATE_ROOT / "data/cca_vs_rust_results.csv"
    )
    parser.add_argument(
        "--output-html", type=Path, default=CRATE_ROOT / "data/cca_vs_rust_results.html"
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
        "--onset-event",
        action=argparse.BooleanOptionalAction,
        default=None,
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
    parser.add_argument("--skip-rust", action="store_true")
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
    if not filtered:
        return None
    return float(np.mean(filtered))


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
        <thead><tr><th>Algorithm</th><th>Dataset</th><th>Profile</th><th>fs</th><th>Requested window</th><th>Actual window</th><th>Subjects</th><th>Python</th><th>Rust exact</th><th>Exact match</th><th>Rust fixed</th><th>Fixed match</th></tr></thead>
        <tbody>{summary_rows}</tbody>
      </table>
    </div>
    <div class="card">
      <h2>Details</h2>
      <table>
        <thead><tr><th>Algorithm</th><th>Dataset</th><th>Profile</th><th>Subject</th><th>Fold</th><th>fs</th><th>Requested window</th><th>Actual window</th><th>Features</th><th>Python</th><th>Rust exact</th><th>Exact match</th><th>Rust fixed</th><th>Fixed match</th></tr></thead>
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
    rust_binary: Path | None,
    fold_idx: int,
    folds: int,
    window_requests_seconds: list[float],
    args: argparse.Namespace,
) -> list[dict[str, Any]]:
    fold_parts = benchmark.fold_slices(data.x.shape[0], folds)
    test_idx = fold_parts[fold_idx]
    train_idx = np.concatenate(
        [part for idx, part in enumerate(fold_parts) if idx != fold_idx]
    )
    x_test = data.x[test_idx]
    y_test = data.y[test_idx]
    stimulus_fs = benchmark.stimulus_to_sample_rate(
        data.stimulus,
        presentation_rate=data.presentation_rate,
        fs=data.fs,
    )

    rows: list[dict[str, Any]] = []
    for requested_window_seconds in window_requests_seconds:
        x_test_window, _stimulus_window, window_info = (
            benchmark.slice_windowed_trials_and_stimulus(
                x_test,
                stimulus_fs,
                data.fs,
                data.fs,
                requested_window_seconds,
                args.drop_first_seconds,
            )
        )
        window_samples = int(window_info["effective_window_samples"])
        actual_window_seconds = float(window_info["effective_window_seconds"])
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
        elif algorithm == "cumulative_cca":
            if args.cumulative_update_mode == "naive":
                reference = cumulative_cca_predictions_pyntbci(
                    x_test_window,
                    stimulus_fs,
                    data.fs,
                    event=args.event,
                    onset_event=args.onset_event,
                    encoding_length=args.encoding_length,
                    start_sample=int(window_info["leading_trim_samples"]),
                )
            elif args.cumulative_update_mode == "confidence_gated":
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
            else:
                raise ValueError(
                    f"Unsupported cumulative_update_mode {args.cumulative_update_mode}"
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
        if args.skip_rust:
            rust = None
        else:
            assert rust_binary is not None
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
                "effective_window_seconds": actual_window_seconds,
                "leading_trim_seconds": float(window_info["leading_trim_seconds"]),
                "window": fixture["window"],
                "feature_count": fixture["features"],
                "train_trials": int(train_idx.shape[0]),
                "test_trials": int(x_test.shape[0]),
                "python_reference_accuracy": float(
                    np.mean(reference.predictions == y_test)
                ),
                "pyntbci_accuracy": float(np.mean(reference.predictions == y_test)),
                "rust_exact_accuracy": (
                    None if rust is None else float(rust["rust_exact_accuracy"])
                ),
                "rust_exact_match_rate": (
                    None if rust is None else float(rust["rust_exact_match_rate"])
                ),
                "rust_fixed_accuracy": (
                    None if rust is None else float(rust["rust_fixed_accuracy"])
                ),
                "rust_fixed_match_rate": (
                    None if rust is None else float(rust["rust_fixed_match_rate"])
                ),
            }
        )
    return rows


def main() -> None:
    args = parse_args()
    benchmark = load_benchmark_module()
    profile = benchmark.resolve_benchmark_profile(args.profile)
    preprocessing = benchmark.resolve_preprocessing_options(
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
    args.event = benchmark.resolve_event(profile, args.event)
    args.onset_event = benchmark.resolve_onset_event(profile, args.onset_event)
    args.encoding_length = benchmark.resolve_encoding_length(
        profile, args.encoding_length
    )
    resolved_target_fs = benchmark.resolve_target_fs(profile, args.target_fs)
    target_fs_grid = args.target_fs_grid or [resolved_target_fs]
    fold_indices = args.fold_index or list(range(args.folds))
    rust_binary = None if args.skip_rust else build_rust_binary()
    console = Console()
    resolved_window_grid = args.window_seconds_grid

    results: list[dict[str, Any]] = []
    for dataset in args.datasets:
        subjects = args.subjects or benchmark.subject_list_for_dataset(dataset)
        if args.max_subjects is not None:
            subjects = subjects[: args.max_subjects]
        for target_fs in target_fs_grid:
            benchmark.validate_target_fs(target_fs)
            for subject in subjects:
                full_trial_seconds = benchmark.trial_seconds_for_dataset(dataset)
                resolved_window_grid = benchmark.resolve_window_grid(
                    profile,
                    dataset,
                    args.window_seconds_grid,
                    args.window_step_seconds,
                )
                window_requests_seconds = benchmark.decode_window_requests(
                    full_trial_seconds,
                    resolved_window_grid,
                    args.window_step_seconds,
                )
                data_cache: dict[tuple[str, float | None], Any] = {}
                for algorithm in args.algorithms:
                    grouped_windows = [window_requests_seconds]
                    use_direct_window_trials = (
                        benchmark.loader_trial_seconds_for_algorithm(
                            dataset,
                            algorithm,
                            requested_window_seconds=full_trial_seconds,
                        )
                        is not None
                    )
                    if use_direct_window_trials:
                        grouped_windows = [
                            [window] for window in window_requests_seconds
                        ]

                    for windows in grouped_windows:
                        load_seconds = (
                            benchmark.loader_trial_seconds_for_algorithm(
                                dataset,
                                algorithm,
                                requested_window_seconds=windows[0],
                            )
                            if use_direct_window_trials
                            else None
                        )
                        cache_key = (algorithm, load_seconds)
                        if cache_key not in data_cache:
                            data_cache[cache_key] = benchmark.load_subject(
                                dataset,
                                subject,
                                args.data_dir,
                                target_fs,
                                trial_seconds=load_seconds,
                                preprocessing=preprocessing,
                            )

                        data = data_cache[cache_key]
                        for fold_idx in fold_indices:
                            console.print(
                                f"[blue]cca[/blue] dataset={dataset} subject={subject} "
                                f"fold={fold_idx} target_fs={target_fs} algorithm={algorithm} "
                                f"windows={','.join(f'{value:.3f}' for value in windows)}"
                            )
                            results.extend(
                                benchmark_subject_fold_windows(
                                    algorithm,
                                    data,
                                    benchmark,
                                    rust_binary,
                                    fold_idx,
                                    args.folds,
                                    windows,
                                    args,
                                )
                            )

    config = {
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
        "preprocessing": {
            "band_low": preprocessing.band_low,
            "band_high": preprocessing.band_high,
            "notch_hz": preprocessing.notch_hz,
            "drop_first_seconds": preprocessing.drop_first_seconds,
        },
    }
    args.output_json.write_text(
        json.dumps({"config": config, "results": results}, indent=2) + "\n",
        encoding="utf-8",
    )
    args.output_csv.write_text(flatten_results_csv(results), encoding="utf-8")
    render_html_report(args.output_html, config, results)

    summary = grouped_summary_rows(results)
    table = Table(title="CCA Benchmark Summary")
    for column in [
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
        table.add_column(column)
    for row in summary:
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


if __name__ == "__main__":
    main()
