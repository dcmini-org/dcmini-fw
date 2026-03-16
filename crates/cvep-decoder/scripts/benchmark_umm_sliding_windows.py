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
"""Benchmark sliding-window UMM decoding on the downloaded c-VEP datasets."""

from __future__ import annotations

import argparse
import html
import importlib.util
import json
from pathlib import Path
import sys
from typing import Any

import numpy as np
from rich.console import Console
from rich.table import Table

from umm_feature_utils import (
    build_umm_features,
    instantaneous_umm_predictions,
    make_structure,
)


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
    parser.add_argument(
        "--output-json",
        type=Path,
        default=CRATE_ROOT / "data/umm_sliding_window_results.json",
    )
    parser.add_argument(
        "--output-csv",
        type=Path,
        default=CRATE_ROOT / "data/umm_sliding_window_results.csv",
    )
    parser.add_argument(
        "--output-html",
        type=Path,
        default=CRATE_ROOT / "data/umm_sliding_window_results.html",
    )
    parser.add_argument("--datasets", nargs="+", default=["Thielen2021"])
    parser.add_argument("--subjects", type=int, nargs="+", default=None)
    parser.add_argument("--max-subjects", type=int, default=None)
    parser.add_argument("--folds", type=int, default=5)
    parser.add_argument("--fold-index", type=int, nargs="+", default=None)
    parser.add_argument("--target-fs", type=int, default=250)
    parser.add_argument("--window-seconds-grid", type=float, nargs="+", default=[1.0, 4.0])
    parser.add_argument("--step-seconds", type=float, default=0.5)
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
    parser.add_argument("--regularization", type=float, default=1.0e-3)
    return parser.parse_args()


def rows_to_csv(rows: list[dict[str, Any]]) -> str:
    keys = [
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
        "epochs_per_window",
        "trials",
        "accuracy",
    ]
    lines = [",".join(keys)]
    for row in rows:
        lines.append(",".join("" if row.get(key) is None else str(row[key]) for key in keys))
    return "\n".join(lines) + "\n"


def grouped_summary(rows: list[dict[str, Any]]) -> list[dict[str, Any]]:
    grouped: dict[tuple[Any, ...], list[dict[str, Any]]] = {}
    for row in rows:
        key = (
            row["variant"],
            row["dataset"],
            row["window_seconds"],
            row["window_end_seconds"],
        )
        grouped.setdefault(key, []).append(row)

    out = []
    for key, members in sorted(grouped.items()):
        variant, dataset, window_seconds, window_end_seconds = key
        out.append(
            {
                "variant": variant,
                "dataset": dataset,
                "window_seconds": window_seconds,
                "window_end_seconds": window_end_seconds,
                "subjects": len({row["subject"] for row in members}),
                "mean_accuracy": float(np.mean([row["accuracy"] for row in members])),
            }
        )
    return out


def render_html_report(output: Path, config: dict[str, Any], rows: list[dict[str, Any]]) -> None:
    summary = grouped_summary(rows)
    summary_rows = "\n".join(
        (
            "<tr>"
            f"<td>{html.escape(row['variant'])}</td>"
            f"<td>{html.escape(row['dataset'])}</td>"
            f"<td>{row['window_seconds']:.3f}</td>"
            f"<td>{row['window_end_seconds']:.3f}</td>"
            f"<td>{row['subjects']}</td>"
            f"<td>{row['mean_accuracy']:.4f}</td>"
            "</tr>"
        )
        for row in summary
    )
    detail_rows = "\n".join(
        (
            "<tr>"
            f"<td>{html.escape(row['variant'])}</td>"
            f"<td>{html.escape(row['dataset'])}</td>"
            f"<td>{row['subject']}</td>"
            f"<td>{row['fold_index']}</td>"
            f"<td>{row['window_seconds']:.3f}</td>"
            f"<td>{row['window_start_seconds']:.3f}</td>"
            f"<td>{row['window_end_seconds']:.3f}</td>"
            f"<td>{row['epochs_per_window']}</td>"
            f"<td>{row['accuracy']:.4f}</td>"
            "</tr>"
        )
        for row in rows
    )
    document = f"""<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Sliding UMM Benchmark</title>
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
      <h1>Sliding UMM Benchmark</h1>
      <p>Sliding-window UMM decoding over continuous reconstructed trial data.</p>
      <pre>{html.escape(json.dumps(config, indent=2))}</pre>
    </div>
    <div class="card">
      <h2>Summary</h2>
      <table>
        <thead><tr><th>Variant</th><th>Dataset</th><th>Window</th><th>Window end</th><th>Subjects</th><th>Mean accuracy</th></tr></thead>
        <tbody>{summary_rows}</tbody>
      </table>
    </div>
    <div class="card">
      <h2>Details</h2>
      <table>
        <thead><tr><th>Variant</th><th>Dataset</th><th>Subject</th><th>Fold</th><th>Window</th><th>Start</th><th>End</th><th>Epochs</th><th>Accuracy</th></tr></thead>
        <tbody>{detail_rows}</tbody>
      </table>
    </div>
  </main>
</body>
</html>
"""
    output.write_text(document, encoding="utf-8")


def sliding_window_starts(
    trial_samples: int,
    window_samples: int,
    step_samples: int,
) -> np.ndarray:
    if window_samples > trial_samples:
        return np.asarray([], dtype=np.int64)
    last_start = trial_samples - window_samples
    starts = np.arange(0, last_start + 1, step_samples, dtype=np.int64)
    if starts[-1] != last_start:
        starts = np.concatenate((starts, np.asarray([last_start], dtype=np.int64)))
    return starts


def benchmark_subject_fold(
    data: Any,
    benchmark: Any,
    fold_idx: int,
    folds: int,
    window_seconds_grid: list[float],
    step_seconds: float,
    epoch_seconds: float,
    epoch_schedule: str,
    lag_seconds: float,
    layout: str,
    trial_demean: bool,
    epoch_demean: bool,
    regularization: float,
) -> list[dict[str, Any]]:
    fold_parts = benchmark.fold_slices(data.x.shape[0], folds)
    test_idx = fold_parts[fold_idx]
    x_test = data.x[test_idx]
    y_test = data.y[test_idx]
    trial_samples = x_test.shape[2]
    step_samples = benchmark.seconds_to_samples(step_seconds, data.fs)

    rows: list[dict[str, Any]] = []
    for window_seconds in window_seconds_grid:
        window_samples = benchmark.seconds_to_samples(window_seconds, data.fs)
        starts = sliding_window_starts(trial_samples, window_samples, step_samples)
        accumulated_scores: np.ndarray | None = None

        for start in starts:
            stop = start + window_samples
            x_window = x_test[:, :, start:stop]
            exported = build_umm_features(
                trials=x_window,
                stimulus=data.stimulus,
                fs=data.fs,
                presentation_rate=data.presentation_rate,
                epoch_seconds=epoch_seconds,
                layout=layout,
                epoch_schedule=epoch_schedule,
                response_lag_seconds=lag_seconds,
                trial_demean=trial_demean,
                epoch_demean=epoch_demean,
                window_start_seconds=start / data.fs,
            )
            structure = make_structure(exported)

            inst_predictions, inst_scores = instantaneous_umm_predictions(
                exported.features,
                exported.codebook,
                regularization=regularization,
                structure=structure,
            )
            if accumulated_scores is None:
                accumulated_scores = inst_scores.astype(np.float64, copy=True)
            else:
                accumulated_scores += inst_scores
            accumulated_predictions = np.argmax(accumulated_scores, axis=1)

            for variant, predictions in (
                ("instantaneous_window", inst_predictions),
                ("instantaneous_accumulated", accumulated_predictions),
            ):
                rows.append(
                    {
                        "variant": variant,
                        "dataset": data.dataset,
                        "subject": data.subject,
                        "fold_index": fold_idx,
                        "target_fs": data.fs,
                        "window_seconds": window_seconds,
                        "step_seconds": step_seconds,
                        "window_start_seconds": start / data.fs,
                        "window_end_seconds": stop / data.fs,
                        "window_start_sample": int(start),
                        "window_end_sample": int(stop),
                        "feature_count": int(exported.features.shape[1]),
                        "epochs_per_window": int(exported.features.shape[2]),
                        "trials": int(exported.features.shape[0]),
                        "accuracy": float(np.mean(predictions == y_test)),
                    }
                )

    return rows


def main() -> None:
    args = parse_args()
    benchmark = load_benchmark_module()
    benchmark.validate_target_fs(args.target_fs)
    console = Console()

    rows: list[dict[str, Any]] = []
    for dataset in args.datasets:
        subjects = args.subjects or benchmark.subject_list_for_dataset(dataset)
        if args.max_subjects is not None:
            subjects = subjects[: args.max_subjects]
        fold_indices = args.fold_index or list(range(args.folds))
        for subject in subjects:
            data = benchmark.load_subject(dataset, subject, args.data_dir, args.target_fs)
            for fold_idx in fold_indices:
                rows.extend(
                    benchmark_subject_fold(
                        data=data,
                        benchmark=benchmark,
                        fold_idx=fold_idx,
                        folds=args.folds,
                        window_seconds_grid=args.window_seconds_grid,
                        step_seconds=args.step_seconds,
                        epoch_seconds=args.epoch_seconds,
                        epoch_schedule=args.epoch_schedule,
                        lag_seconds=args.lag_seconds,
                        layout=args.layout,
                        trial_demean=args.trial_demean,
                        epoch_demean=args.epoch_demean,
                        regularization=args.regularization,
                    )
                )
                console.print(
                    f"[blue]umm-sliding[/blue] dataset={dataset} subject={subject} "
                    f"fold={fold_idx}"
                )

    payload = {
        "config": {
            "datasets": args.datasets,
            "subjects": args.subjects,
            "max_subjects": args.max_subjects,
            "folds": args.folds,
            "fold_index": args.fold_index,
            "target_fs": args.target_fs,
            "window_seconds_grid": args.window_seconds_grid,
            "step_seconds": args.step_seconds,
            "epoch_seconds": args.epoch_seconds,
            "epoch_schedule": args.epoch_schedule,
            "lag_seconds": args.lag_seconds,
            "layout": args.layout,
            "trial_demean": args.trial_demean,
            "epoch_demean": args.epoch_demean,
            "regularization": args.regularization,
            "note": (
                "The `instantaneous_accumulated` variant accumulates class scores "
                "across overlapping windows within a trial. A truly stateful cumulative "
                "UMM-over-windows variant is not included here because shifted windows "
                "carry shifted codebook slices, so the current helper would need a "
                "window-dependent label model to do that correctly."
            ),
        },
        "results": rows,
    }
    args.output_json.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    args.output_csv.write_text(rows_to_csv(rows), encoding="utf-8")
    render_html_report(args.output_html, payload["config"], rows)

    summary = grouped_summary(rows)
    table = Table(title="Sliding UMM summary")
    table.add_column("variant")
    table.add_column("dataset")
    table.add_column("window")
    table.add_column("end")
    table.add_column("subjects")
    table.add_column("mean_acc")
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


if __name__ == "__main__":
    main()
