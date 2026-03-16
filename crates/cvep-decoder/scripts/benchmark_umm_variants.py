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
"""Benchmark UMM variants on the downloaded c-VEP datasets."""

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
    cumulative_umm_predictions,
    instantaneous_umm_predictions,
    make_structure,
)


WORKSPACE_ROOT = Path(__file__).resolve().parents[3]
CRATE_ROOT = Path(__file__).resolve().parents[1]
RAW_BENCHMARK_SCRIPT = CRATE_ROOT / "scripts/benchmark_pyntbci_vs_rust.py"

DEFAULT_DATASETS = ["Thielen2021", "Thielen2015", "CastillosCVEP40", "CastillosCVEP100"]


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
    parser.add_argument("--output-json", type=Path, default=CRATE_ROOT / "data/umm_benchmark_results.json")
    parser.add_argument("--output-csv", type=Path, default=CRATE_ROOT / "data/umm_benchmark_results.csv")
    parser.add_argument("--output-html", type=Path, default=CRATE_ROOT / "data/umm_benchmark_results.html")
    parser.add_argument("--datasets", nargs="+", default=DEFAULT_DATASETS)
    parser.add_argument("--subjects", type=int, nargs="+", default=None)
    parser.add_argument("--max-subjects", type=int, default=None)
    parser.add_argument("--folds", type=int, default=5)
    parser.add_argument("--fold-index", type=int, nargs="+", default=None)
    parser.add_argument("--target-fs", type=int, default=250)
    parser.add_argument("--target-fs-grid", type=int, nargs="+", default=None)
    parser.add_argument("--window-step-seconds", type=float, default=None)
    parser.add_argument("--window-seconds-grid", type=float, nargs="+", default=None)
    parser.add_argument("--epoch-seconds-grid", type=float, nargs="+", default=[0.3])
    parser.add_argument(
        "--epoch-schedules",
        nargs="+",
        choices=["rounded_stride", "fractional_onset"],
        default=["fractional_onset"],
    )
    parser.add_argument("--lag-seconds-grid", type=float, nargs="+", default=[0.0])
    parser.add_argument(
        "--layouts",
        nargs="+",
        choices=["channel_prime", "time_prime"],
        default=["channel_prime", "time_prime"],
    )
    parser.add_argument(
        "--trial-demean-grid",
        nargs="+",
        choices=["false", "true"],
        default=["false"],
    )
    parser.add_argument(
        "--epoch-demean-grid",
        nargs="+",
        choices=["false", "true"],
        default=["false"],
    )
    parser.add_argument(
        "--confidence-models",
        nargs="+",
        choices=["inferred_normalized_margin", "margin_over_winner"],
        default=["inferred_normalized_margin", "margin_over_winner"],
    )
    parser.add_argument(
        "--variants",
        nargs="+",
        choices=["instantaneous_umm", "cumulative_umm"],
        default=["instantaneous_umm", "cumulative_umm"],
    )
    parser.add_argument("--regularization", type=float, default=1.0e-3)
    return parser.parse_args()


def rows_to_csv(rows: list[dict[str, Any]]) -> str:
    keys = [
        "variant",
        "dataset",
        "subject",
        "fold_index",
        "target_fs",
        "requested_window_seconds",
        "window_seconds",
        "epoch_seconds",
        "epoch_schedule",
        "lag_seconds",
        "layout",
        "trial_demean",
        "epoch_demean",
        "confidence_model",
        "classes",
        "channels",
        "feature_count",
        "epochs_per_trial",
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
            row["target_fs"],
            row["requested_window_seconds"],
            row["epoch_seconds"],
            row["epoch_schedule"],
            row["lag_seconds"],
            row["layout"],
            row["trial_demean"],
            row["epoch_demean"],
            row["confidence_model"],
        )
        grouped.setdefault(key, []).append(row)

    summary_rows = []
    for key, members in sorted(grouped.items()):
        (
            variant,
            dataset,
            target_fs,
            requested_window_seconds,
            epoch_seconds,
            epoch_schedule,
            lag_seconds,
            layout,
            trial_demean,
            epoch_demean,
            confidence_model,
        ) = key
        summary_rows.append(
            {
                "variant": variant,
                "dataset": dataset,
                "target_fs": target_fs,
                "requested_window_seconds": requested_window_seconds,
                "epoch_seconds": epoch_seconds,
                "epoch_schedule": epoch_schedule,
                "lag_seconds": lag_seconds,
                "layout": layout,
                "trial_demean": trial_demean,
                "epoch_demean": epoch_demean,
                "confidence_model": confidence_model,
                "subjects": len({row["subject"] for row in members}),
                "mean_accuracy": float(np.mean([row["accuracy"] for row in members])),
            }
        )
    return summary_rows


def render_html_report(output: Path, config: dict[str, Any], rows: list[dict[str, Any]]) -> None:
    summary = grouped_summary(rows)
    summary_rows = "\n".join(
        (
            "<tr>"
            f"<td>{html.escape(row['variant'])}</td>"
            f"<td>{html.escape(row['dataset'])}</td>"
            f"<td>{row['target_fs']}</td>"
            f"<td>{row['requested_window_seconds']:.3f}</td>"
            f"<td>{row['epoch_seconds']:.3f}</td>"
            f"<td>{html.escape(row['epoch_schedule'])}</td>"
            f"<td>{row['lag_seconds']:.3f}</td>"
            f"<td>{html.escape(row['layout'])}</td>"
            f"<td>{html.escape(str(row['trial_demean']))}</td>"
            f"<td>{html.escape(str(row['epoch_demean']))}</td>"
            f"<td>{html.escape(str(row['confidence_model']))}</td>"
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
            f"<td>{row['target_fs']}</td>"
            f"<td>{row['requested_window_seconds']:.3f}</td>"
            f"<td>{row['window_seconds']:.3f}</td>"
            f"<td>{row['epoch_seconds']:.3f}</td>"
            f"<td>{html.escape(row['epoch_schedule'])}</td>"
            f"<td>{row['lag_seconds']:.3f}</td>"
            f"<td>{html.escape(row['layout'])}</td>"
            f"<td>{html.escape(str(row['trial_demean']))}</td>"
            f"<td>{html.escape(str(row['epoch_demean']))}</td>"
            f"<td>{html.escape(str(row['confidence_model']))}</td>"
            f"<td>{row['classes']}</td>"
            f"<td>{row['channels']}</td>"
            f"<td>{row['feature_count']}</td>"
            f"<td>{row['epochs_per_trial']}</td>"
            f"<td>{row['trials']}</td>"
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
  <title>UMM Benchmark Report</title>
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
      <h1>UMM Benchmark Report</h1>
      <p>Offline sweep over UMM decoder variants, feature layouts, and confidence models.</p>
      <pre>{html.escape(json.dumps(config, indent=2))}</pre>
    </div>
    <div class="card">
      <h2>Summary</h2>
      <table>
        <thead><tr><th>Variant</th><th>Dataset</th><th>FS</th><th>Window</th><th>Epoch</th><th>Schedule</th><th>Lag</th><th>Layout</th><th>Trial demean</th><th>Epoch demean</th><th>Confidence</th><th>Subjects</th><th>Mean accuracy</th></tr></thead>
        <tbody>{summary_rows}</tbody>
      </table>
    </div>
    <div class="card">
      <h2>Details</h2>
      <table>
        <thead><tr><th>Variant</th><th>Dataset</th><th>Subject</th><th>Fold</th><th>FS</th><th>Requested window</th><th>Window</th><th>Epoch</th><th>Schedule</th><th>Lag</th><th>Layout</th><th>Trial demean</th><th>Epoch demean</th><th>Confidence</th><th>Classes</th><th>Channels</th><th>Features</th><th>Epochs/trial</th><th>Trials</th><th>Accuracy</th></tr></thead>
        <tbody>{detail_rows}</tbody>
      </table>
    </div>
  </main>
</body>
</html>
"""
    output.write_text(document, encoding="utf-8")


def main() -> None:
    args = parse_args()
    benchmark = load_benchmark_module()
    target_fs_grid = args.target_fs_grid or [args.target_fs]
    trial_demean_grid = [value == "true" for value in args.trial_demean_grid]
    epoch_demean_grid = [value == "true" for value in args.epoch_demean_grid]
    console = Console()

    rows: list[dict[str, Any]] = []
    for target_fs in target_fs_grid:
        benchmark.validate_target_fs(target_fs)
        for dataset in args.datasets:
            subjects = args.subjects or benchmark.subject_list_for_dataset(dataset)
            if args.max_subjects is not None:
                subjects = subjects[: args.max_subjects]
            fold_indices = args.fold_index or list(range(args.folds))
            for subject in subjects:
                data = benchmark.load_subject(dataset, subject, args.data_dir, target_fs)
                window_requests = benchmark.decode_window_requests(
                    data.trial_seconds,
                    args.window_seconds_grid,
                    args.window_step_seconds,
                )
                fold_parts = benchmark.fold_slices(data.x.shape[0], args.folds)

                for fold_idx in fold_indices:
                    test_idx = fold_parts[fold_idx]
                    x_test = data.x[test_idx]
                    y_test = data.y[test_idx]

                    for requested_window_seconds in window_requests:
                        window_samples = min(
                            benchmark.seconds_to_samples(requested_window_seconds, data.fs),
                            x_test.shape[2],
                        )
                        x_window = x_test[:, :, :window_samples]

                        for epoch_seconds in args.epoch_seconds_grid:
                            for epoch_schedule in args.epoch_schedules:
                                for lag_seconds in args.lag_seconds_grid:
                                    for layout in args.layouts:
                                        for trial_demean in trial_demean_grid:
                                            for epoch_demean in epoch_demean_grid:
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
                                                )
                                                structure = make_structure(exported)

                                                if "instantaneous_umm" in args.variants:
                                                    predictions, _scores = instantaneous_umm_predictions(
                                                        exported.features,
                                                        exported.codebook,
                                                        regularization=args.regularization,
                                                        structure=structure,
                                                    )
                                                    rows.append(
                                                        {
                                                            "variant": "instantaneous_umm",
                                                            "dataset": dataset,
                                                            "subject": subject,
                                                            "fold_index": fold_idx,
                                                            "target_fs": data.fs,
                                                            "requested_window_seconds": requested_window_seconds,
                                                            "window_seconds": window_samples / data.fs,
                                                            "epoch_seconds": epoch_seconds,
                                                            "epoch_schedule": epoch_schedule,
                                                            "lag_seconds": lag_seconds,
                                                            "layout": layout,
                                                            "trial_demean": trial_demean,
                                                            "epoch_demean": epoch_demean,
                                                            "confidence_model": None,
                                                            "classes": int(exported.codebook.shape[0]),
                                                            "channels": int(data.x.shape[1]),
                                                            "feature_count": int(exported.features.shape[1]),
                                                            "epochs_per_trial": exported.epochs_per_trial,
                                                            "trials": int(exported.features.shape[0]),
                                                            "accuracy": float(np.mean(predictions == y_test)),
                                                        }
                                                    )

                                                if "cumulative_umm" in args.variants:
                                                    for confidence_model in args.confidence_models:
                                                        predictions, _scores, _state = cumulative_umm_predictions(
                                                            exported.features,
                                                            exported.codebook,
                                                            regularization=args.regularization,
                                                            structure=structure,
                                                            confidence_model=confidence_model,
                                                        )
                                                        rows.append(
                                                            {
                                                                "variant": "cumulative_umm",
                                                                "dataset": dataset,
                                                                "subject": subject,
                                                                "fold_index": fold_idx,
                                                                "target_fs": data.fs,
                                                                "requested_window_seconds": requested_window_seconds,
                                                                "window_seconds": window_samples / data.fs,
                                                                "epoch_seconds": epoch_seconds,
                                                                "epoch_schedule": epoch_schedule,
                                                                "lag_seconds": lag_seconds,
                                                                "layout": layout,
                                                                "trial_demean": trial_demean,
                                                                "epoch_demean": epoch_demean,
                                                                "confidence_model": confidence_model,
                                                                "classes": int(exported.codebook.shape[0]),
                                                                "channels": int(data.x.shape[1]),
                                                                "feature_count": int(exported.features.shape[1]),
                                                                "epochs_per_trial": exported.epochs_per_trial,
                                                                "trials": int(exported.features.shape[0]),
                                                                "accuracy": float(np.mean(predictions == y_test)),
                                                            }
                                                        )

                                                console.print(
                                                    f"[blue]umm[/blue] dataset={dataset} subject={subject} "
                                                    f"fold={fold_idx} window={window_samples / data.fs:.3f}s "
                                                    f"epoch={epoch_seconds:.3f}s schedule={epoch_schedule} "
                                                    f"lag={lag_seconds:.3f}s layout={layout} "
                                                    f"trial_demean={trial_demean} epoch_demean={epoch_demean}"
                                                )

    payload = {
        "config": {
            "datasets": args.datasets,
            "subjects": args.subjects,
            "max_subjects": args.max_subjects,
            "folds": args.folds,
            "fold_index": args.fold_index,
            "target_fs_grid": target_fs_grid,
            "window_step_seconds": args.window_step_seconds,
            "window_seconds_grid": args.window_seconds_grid,
            "epoch_seconds_grid": args.epoch_seconds_grid,
            "epoch_schedules": args.epoch_schedules,
            "lag_seconds_grid": args.lag_seconds_grid,
            "layouts": args.layouts,
            "trial_demean_grid": trial_demean_grid,
            "epoch_demean_grid": epoch_demean_grid,
            "confidence_models": args.confidence_models,
            "variants": args.variants,
            "regularization": args.regularization,
            "source_note": (
                "Winner-vs-runner-up confidence dependence is source-backed, but the exact "
                "confidence transform remains an exposed benchmark choice because the "
                "accessible papers and public repos did not disclose one verified formula."
            ),
        },
        "results": rows,
    }
    args.output_json.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    args.output_csv.write_text(rows_to_csv(rows), encoding="utf-8")
    render_html_report(args.output_html, payload["config"], rows)

    summary = grouped_summary(rows)
    table = Table(title="UMM benchmark summary")
    table.add_column("variant")
    table.add_column("dataset")
    table.add_column("fs")
    table.add_column("window")
    table.add_column("epoch")
    table.add_column("schedule")
    table.add_column("lag")
    table.add_column("layout")
    table.add_column("trial_demean")
    table.add_column("epoch_demean")
    table.add_column("confidence")
    table.add_column("subjects")
    table.add_column("mean_acc")
    for row in summary:
        table.add_row(
            row["variant"],
            row["dataset"],
            str(row["target_fs"]),
            f"{row['requested_window_seconds']:.3f}",
            f"{row['epoch_seconds']:.3f}",
            row["epoch_schedule"],
            f"{row['lag_seconds']:.3f}",
            row["layout"],
            str(row["trial_demean"]),
            str(row["epoch_demean"]),
            str(row["confidence_model"]),
            str(row["subjects"]),
            f"{row['mean_accuracy']:.4f}",
        )
    Console().print(table)


if __name__ == "__main__":
    main()
