#!/usr/bin/env python3
# /// script
# dependencies = [
#   "h5py",
#   "mne",
#   "numpy",
#   "pyntbci",
#   "rich",
#   "scipy",
# ]
# ///
"""Benchmark current reference preprocessing against causal SOS preprocessing."""

from __future__ import annotations

import argparse
import html
import importlib.util
import json
import os
import sys
import tempfile
from contextlib import contextmanager
from pathlib import Path
from typing import Any, Iterator

import numpy as np
from rich.console import Console
from rich.table import Table
from scipy import signal

os.environ.setdefault("MNE_DONTWRITE_HOME", "true")
os.environ.setdefault("MNE_HOME", str(Path(tempfile.gettempdir()) / "mne-home"))
os.environ.setdefault(
    "MPLCONFIGDIR",
    str(Path(tempfile.gettempdir()) / "matplotlib-cache"),
)

import mne


WORKSPACE_ROOT = Path(__file__).resolve().parents[3]
RAW_BENCHMARK_SCRIPT = (
    WORKSPACE_ROOT / "crates/cvep-decoder/scripts/benchmark_pyntbci_vs_rust.py"
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--data-dir",
        type=Path,
        default=WORKSPACE_ROOT / "crates/cvep-decoder/data",
        help="Local data root containing the downloaded datasets.",
    )
    parser.add_argument(
        "--output-json",
        type=Path,
        default=WORKSPACE_ROOT / "crates/cvep-decoder/data/benchmark_causal_vs_reference.json",
        help="Path for the raw benchmark results JSON.",
    )
    parser.add_argument(
        "--output-html",
        type=Path,
        default=WORKSPACE_ROOT / "crates/cvep-decoder/data/benchmark_causal_vs_reference.html",
        help="Path for the HTML report.",
    )
    parser.add_argument(
        "--algorithms",
        nargs="+",
        choices=["etrca", "rcca"],
        default=["etrca", "rcca"],
        help="Algorithms to benchmark.",
    )
    parser.add_argument(
        "--datasets",
        nargs="+",
        default=["Thielen2021", "CastillosCVEP100"],
        help="Datasets to benchmark.",
    )
    parser.add_argument(
        "--subjects",
        type=int,
        nargs="+",
        default=None,
        help="Optional explicit subject list to use for every dataset.",
    )
    parser.add_argument(
        "--max-subjects",
        type=int,
        default=None,
        help="Optional cap on subjects per dataset.",
    )
    parser.add_argument(
        "--folds",
        type=int,
        default=5,
        help="Number of chronological folds.",
    )
    parser.add_argument(
        "--fold-index",
        type=int,
        nargs="+",
        default=None,
        help="Optional specific fold indices. Defaults to all folds.",
    )
    parser.add_argument(
        "--target-fs",
        type=int,
        default=250,
        help="Resample all trials to this frequency before fitting.",
    )
    parser.add_argument(
        "--target-fs-grid",
        type=int,
        nargs="+",
        default=None,
        help="Optional list of target sample rates to sweep. If omitted, uses --target-fs.",
    )
    parser.add_argument(
        "--window-step-seconds",
        type=float,
        default=None,
        help="Optional decoding-window sweep step in seconds.",
    )
    parser.add_argument(
        "--window-seconds-grid",
        type=float,
        nargs="+",
        default=None,
        help="Optional explicit decoding-window lengths in seconds.",
    )
    parser.add_argument(
        "--adc-bits",
        type=int,
        default=24,
        help="Signed ADC bit depth used to map held-out trials into ADC codes.",
    )
    parser.add_argument(
        "--adc-headroom",
        type=float,
        default=0.95,
        help="Fraction of signed full scale to use when mapping held-out trials into ADC codes.",
    )
    parser.add_argument(
        "--encoding-length",
        type=float,
        default=0.3,
        help="Encoding length in seconds for rCCA.",
    )
    parser.add_argument(
        "--event",
        type=str,
        default="refe",
        help="Stimulus event string for rCCA.",
    )
    parser.add_argument(
        "--band-low",
        type=float,
        default=1.0,
        help="Causal band-pass lower cutoff in Hz.",
    )
    parser.add_argument(
        "--band-high",
        type=float,
        default=65.0,
        help="Causal band-pass upper cutoff in Hz.",
    )
    parser.add_argument(
        "--band-order",
        type=int,
        default=4,
        help="Causal Butterworth band-pass order.",
    )
    parser.add_argument(
        "--notch-q",
        type=float,
        default=30.0,
        help="Q factor for each causal notch section.",
    )
    return parser.parse_args()


def load_benchmark_module() -> Any:
    spec = importlib.util.spec_from_file_location(
        "cvep_raw_benchmark",
        RAW_BENCHMARK_SCRIPT,
    )
    if spec is None or spec.loader is None:
        raise RuntimeError(f"Failed to load benchmark module from {RAW_BENCHMARK_SCRIPT}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def design_causal_sos(
    fs: float,
    band_low: float,
    band_high: float,
    band_order: int,
    notch_q: float,
) -> np.ndarray:
    sos_parts = [
        signal.butter(
            band_order,
            [band_low, band_high],
            btype="bandpass",
            output="sos",
            fs=fs,
        )
    ]
    harmonic = 50.0
    while harmonic < fs / 2.0:
        b, a = signal.iirnotch(harmonic, notch_q, fs)
        sos_parts.append(signal.tf2sos(b, a))
        harmonic += 50.0
    return np.concatenate(sos_parts, axis=0).astype(np.float64)


def causal_epoch_and_resample_factory(
    benchmark: Any,
    band_low: float,
    band_high: float,
    band_order: int,
    notch_q: float,
):
    def epoch_and_resample(
        raw: mne.io.BaseRaw,
        events: np.ndarray,
        target_fs: int,
        tmin: float,
        tmax: float,
        event_id: dict[str, int] | None = None,
    ) -> np.ndarray:
        eeg_picks = mne.pick_types(raw.info, eeg=True, exclude=())
        continuous = raw.get_data(picks=eeg_picks).astype(np.float64, copy=False)
        sos = design_causal_sos(
            fs=float(raw.info["sfreq"]),
            band_low=band_low,
            band_high=band_high,
            band_order=band_order,
            notch_q=notch_q,
        )
        filtered_continuous = signal.sosfilt(sos, continuous, axis=1)
        raw._data[eeg_picks, :] = filtered_continuous
        epochs = mne.Epochs(
            raw,
            events=events,
            event_id=event_id,
            tmin=tmin - benchmark.PRETRIAL_BUFFER_SECONDS,
            tmax=tmax,
            baseline=None,
            picks="eeg",
            preload=True,
            verbose=False,
        )
        epochs.resample(sfreq=target_fs, verbose=False)
        return np.asarray(epochs.get_data(tmin=tmin, tmax=tmax), dtype=np.float64)

    return epoch_and_resample


@contextmanager
def causal_loader_patch(
    benchmark: Any,
    *,
    band_low: float,
    band_high: float,
    band_order: int,
    notch_q: float,
) -> Iterator[None]:
    original_epoch_and_resample = benchmark.epoch_and_resample
    benchmark.epoch_and_resample = causal_epoch_and_resample_factory(
        benchmark,
        band_low=band_low,
        band_high=band_high,
        band_order=band_order,
        notch_q=notch_q,
    )
    try:
        yield
    finally:
        benchmark.epoch_and_resample = original_epoch_and_resample


def grouped_summary_rows(results: list[dict[str, Any]]) -> list[dict[str, Any]]:
    grouped: dict[tuple[str, str, str, int, int], list[dict[str, Any]]] = {}
    for row in results:
        grouped.setdefault(
            (
                row["preprocess_mode"],
                row["algorithm"],
                row["dataset"],
                row["target_fs"],
                row["window"],
            ),
            [],
        ).append(row)

    summaries = []
    for (mode, algorithm, dataset, target_fs, window), rows in sorted(grouped.items()):
        summaries.append(
            {
                "preprocess_mode": mode,
                "algorithm": algorithm,
                "dataset": dataset,
                "target_fs": target_fs,
                "window": window,
                "requested_window_seconds": rows[0]["requested_window_seconds"],
                "window_seconds": rows[0]["window_seconds"],
                "subjects": len({row["subject"] for row in rows}),
                "mean_pyntbci_accuracy": float(np.mean([row["pyntbci_accuracy"] for row in rows])),
                "mean_rust_exact_accuracy": float(np.mean([row["rust_exact_accuracy"] for row in rows])),
                "mean_rust_exact_match_rate": float(np.mean([row["rust_exact_match_rate"] for row in rows])),
            }
        )
    return summaries


def build_delta_rows(results: list[dict[str, Any]]) -> list[dict[str, Any]]:
    keyed: dict[tuple[Any, ...], dict[str, Any]] = {}
    for row in results:
        keyed[
            (
                row["preprocess_mode"],
                row["algorithm"],
                row["dataset"],
                row["target_fs"],
                row["window"],
                row["subject"],
                row["fold_index"],
            )
        ] = row

    deltas = []
    for key, baseline in keyed.items():
        if key[0] != "reference":
            continue
        causal_key = ("causal",) + key[1:]
        causal = keyed.get(causal_key)
        if causal is None:
            continue
        deltas.append(
            {
                "algorithm": baseline["algorithm"],
                "dataset": baseline["dataset"],
                "target_fs": baseline["target_fs"],
                "window": baseline["window"],
                "window_seconds": baseline["window_seconds"],
                "subject": baseline["subject"],
                "fold_index": baseline["fold_index"],
                "pyntbci_delta": float(causal["pyntbci_accuracy"] - baseline["pyntbci_accuracy"]),
                "rust_exact_delta": float(causal["rust_exact_accuracy"] - baseline["rust_exact_accuracy"]),
            }
        )
    return deltas


def optional_mean(rows: list[dict[str, Any]], key: str) -> float | None:
    values = [row[key] for row in rows if row[key] is not None]
    if not values:
        return None
    return float(np.mean(values))


def render_console(console: Console, results: list[dict[str, Any]], deltas: list[dict[str, Any]]) -> None:
    summary = Table(title="Reference vs causal preprocessing summary")
    summary.add_column("Mode")
    summary.add_column("Algorithm")
    summary.add_column("Dataset")
    summary.add_column("fs")
    summary.add_column("Req s")
    summary.add_column("Actual s")
    summary.add_column("Subjects")
    summary.add_column("Mean PyntBCI")
    summary.add_column("Mean Rust exact")
    summary.add_column("Mean exact match")
    for row in grouped_summary_rows(results):
        summary.add_row(
            row["preprocess_mode"],
            row["algorithm"],
            row["dataset"],
            str(row["target_fs"]),
            f"{row['requested_window_seconds']:.3f}",
            f"{row['window_seconds']:.3f}",
            str(row["subjects"]),
            f"{row['mean_pyntbci_accuracy']:.4f}",
            f"{row['mean_rust_exact_accuracy']:.4f}",
            f"{row['mean_rust_exact_match_rate']:.4f}",
        )
    console.print(summary)

    delta_table = Table(title="Causal minus reference deltas")
    delta_table.add_column("Algorithm")
    delta_table.add_column("Dataset")
    delta_table.add_column("fs")
    delta_table.add_column("Actual s")
    delta_table.add_column("Mean PyntBCI delta")
    delta_table.add_column("Mean Rust exact delta")
    grouped: dict[tuple[str, str, int, int], list[dict[str, Any]]] = {}
    for row in deltas:
        grouped.setdefault(
            (row["algorithm"], row["dataset"], row["target_fs"], row["window"]),
            [],
        ).append(row)
    for (algorithm, dataset, target_fs, _window), rows in sorted(grouped.items()):
        delta_table.add_row(
            algorithm,
            dataset,
            str(target_fs),
            f"{rows[0]['window_seconds']:.3f}",
            f"{np.mean([row['pyntbci_delta'] for row in rows]):.4f}",
            f"{np.mean([row['rust_exact_delta'] for row in rows]):.4f}",
        )
    console.print(delta_table)


def fmt_optional(value: float | None) -> str:
    if value is None or not np.isfinite(value):
        return "-"
    return f"{value:.4f}"


def render_html(output: Path, config: dict[str, Any], results: list[dict[str, Any]], deltas: list[dict[str, Any]]) -> None:
    summary_rows = "\n".join(
        (
            "<tr>"
            f"<td>{html.escape(row['preprocess_mode'])}</td>"
            f"<td>{html.escape(row['algorithm'])}</td>"
            f"<td>{html.escape(row['dataset'])}</td>"
            f"<td>{row['target_fs']}</td>"
            f"<td>{row['requested_window_seconds']:.3f}</td>"
            f"<td>{row['window_seconds']:.3f}</td>"
            f"<td>{row['subjects']}</td>"
            f"<td>{row['mean_pyntbci_accuracy']:.4f}</td>"
            f"<td>{row['mean_rust_exact_accuracy']:.4f}</td>"
            f"<td>{row['mean_rust_exact_match_rate']:.4f}</td>"
            "</tr>"
        )
        for row in grouped_summary_rows(results)
    )
    delta_grouped: dict[tuple[str, str, int, int], list[dict[str, Any]]] = {}
    for row in deltas:
        delta_grouped.setdefault(
            (row["algorithm"], row["dataset"], row["target_fs"], row["window"]),
            [],
        ).append(row)
    delta_rows = "\n".join(
        (
            "<tr>"
            f"<td>{html.escape(key[0])}</td>"
            f"<td>{html.escape(key[1])}</td>"
            f"<td>{key[2]}</td>"
            f"<td>{rows[0]['window_seconds']:.3f}</td>"
            f"<td>{np.mean([row['pyntbci_delta'] for row in rows]):.4f}</td>"
            f"<td>{np.mean([row['rust_exact_delta'] for row in rows]):.4f}</td>"
            "</tr>"
        )
        for key, rows in sorted(delta_grouped.items())
    )
    config_html = html.escape(json.dumps(config, indent=2))
    document = f"""<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Causal Preprocessing Benchmark</title>
  <style>
    body {{
      margin: 0;
      background: #f6f1e8;
      color: #1f2933;
      font-family: Georgia, serif;
    }}
    main {{
      max-width: 1200px;
      margin: 0 auto;
      padding: 28px 18px 48px;
    }}
    .card {{
      background: #fffdf8;
      border: 1px solid #d9cfbf;
      border-radius: 16px;
      padding: 18px;
      margin-bottom: 18px;
    }}
    .table-wrap {{
      overflow: auto;
      border: 1px solid #d9cfbf;
      border-radius: 12px;
    }}
    table {{
      width: 100%;
      border-collapse: collapse;
      font-family: Menlo, monospace;
      font-size: 0.88rem;
    }}
    th, td {{
      padding: 10px 12px;
      border-bottom: 1px solid #d9cfbf;
      text-align: left;
    }}
    th {{
      background: #faf6ef;
      color: #0f766e;
    }}
    pre {{
      margin: 0;
      overflow: auto;
      background: #fbf8f1;
      border: 1px solid #d9cfbf;
      border-radius: 12px;
      padding: 14px;
    }}
  </style>
</head>
<body>
  <main>
    <section class="card">
      <h1>Causal Preprocessing Benchmark</h1>
      <p>Side-by-side classification comparison between the current reference preprocessing path and a causal SOS filter chain intended to approximate the embedded deployment path.</p>
    </section>
    <section class="card">
      <h2>Summary</h2>
      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Mode</th>
              <th>Algorithm</th>
              <th>Dataset</th>
              <th>fs</th>
              <th>Req s</th>
              <th>Actual s</th>
              <th>Subjects</th>
              <th>Mean PyntBCI</th>
              <th>Mean Rust exact</th>
              <th>Mean exact match</th>
            </tr>
          </thead>
          <tbody>{summary_rows}</tbody>
        </table>
      </div>
    </section>
    <section class="card">
      <h2>Causal minus reference deltas</h2>
      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Algorithm</th>
              <th>Dataset</th>
              <th>fs</th>
              <th>Actual s</th>
              <th>Mean PyntBCI delta</th>
              <th>Mean Rust exact delta</th>
            </tr>
          </thead>
          <tbody>{delta_rows}</tbody>
        </table>
      </div>
    </section>
    <section class="card">
      <h2>Config</h2>
      <pre>{config_html}</pre>
    </section>
  </main>
</body>
</html>
"""
    output.write_text(document, encoding="utf-8")


def load_subject_with_mode(
    benchmark: Any,
    mode: str,
    dataset: str,
    subject: int,
    data_dir: Path,
    target_fs: int,
    *,
    band_low: float,
    band_high: float,
    band_order: int,
    notch_q: float,
) -> Any:
    if mode == "reference":
        return benchmark.load_subject(dataset, subject, data_dir, target_fs)
    if mode == "causal":
        with causal_loader_patch(
            benchmark,
            band_low=band_low,
            band_high=band_high,
            band_order=band_order,
            notch_q=notch_q,
        ):
            return benchmark.load_subject(dataset, subject, data_dir, target_fs)
    raise ValueError(f"Unsupported preprocess mode {mode}")


def main() -> None:
    args = parse_args()
    benchmark = load_benchmark_module()
    console = Console()
    rust_binary = benchmark.build_rust_binary()
    target_fs_grid = args.target_fs_grid or [args.target_fs]
    for target_fs in target_fs_grid:
        benchmark.validate_target_fs(target_fs)

    fold_indices = (
        args.fold_index if args.fold_index is not None else list(range(args.folds))
    )
    results: list[dict[str, Any]] = []

    for dataset in args.datasets:
        subjects = args.subjects or benchmark.subject_list_for_dataset(dataset)
        if args.max_subjects is not None:
            subjects = subjects[: args.max_subjects]

        for subject in subjects:
            for target_fs in target_fs_grid:
                subject_data: dict[str, Any] = {}
                for mode in ("reference", "causal"):
                    console.print(
                        f"[cyan]loading[/cyan] mode={mode} dataset={dataset} subject={subject} target_fs={target_fs}"
                    )
                    data = load_subject_with_mode(
                        benchmark,
                        mode,
                        dataset,
                        subject,
                        args.data_dir,
                        target_fs,
                        band_low=args.band_low,
                        band_high=args.band_high,
                        band_order=args.band_order,
                        notch_q=args.notch_q,
                    )
                    subject_data[mode] = data
                    console.print(
                        f"[green]loaded[/green] mode={mode} dataset={dataset} subject={subject} "
                        f"target_fs={target_fs} shape={tuple(data.x.shape)}"
                    )

                if not np.array_equal(subject_data["reference"].y, subject_data["causal"].y):
                    raise ValueError(
                        f"Label mismatch between reference and causal loaders for {dataset=} {subject=}"
                    )

                for mode, data in subject_data.items():
                    window_requests_seconds = benchmark.decode_window_requests(
                        data.trial_seconds,
                        explicit=args.window_seconds_grid,
                        step_seconds=args.window_step_seconds,
                    )
                    for algorithm in args.algorithms:
                        for fold_idx in fold_indices:
                            console.print(
                                f"[blue]benchmarking[/blue] mode={mode} algorithm={algorithm} "
                                f"dataset={dataset} subject={subject} target_fs={target_fs} "
                                f"fold={fold_idx}/{args.folds - 1}"
                            )
                            rows = benchmark.benchmark_subject_fold_windows(
                                algorithm,
                                data,
                                rust_binary=rust_binary,
                                fold_idx=fold_idx,
                                folds=args.folds,
                                window_requests_seconds=window_requests_seconds,
                                adc_bits=args.adc_bits,
                                adc_headroom=args.adc_headroom,
                                encoding_length=args.encoding_length,
                                event=args.event,
                            )
                            for row in rows:
                                row["preprocess_mode"] = mode
                            results.extend(rows)

    deltas = build_delta_rows(results)
    payload = {
        "config": {
            "datasets": args.datasets,
            "algorithms": args.algorithms,
            "folds": args.folds,
            "fold_indices": fold_indices,
            "target_fs_grid": target_fs_grid,
            "window_seconds_grid": args.window_seconds_grid,
            "window_step_seconds": args.window_step_seconds,
            "band_low": args.band_low,
            "band_high": args.band_high,
            "band_order": args.band_order,
            "notch_q": args.notch_q,
        },
        "results": results,
        "deltas": deltas,
    }

    args.output_json.parent.mkdir(parents=True, exist_ok=True)
    args.output_html.parent.mkdir(parents=True, exist_ok=True)
    args.output_json.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    render_html(args.output_html, payload["config"], results, deltas)
    render_console(console, results, deltas)
    console.print(f"[green]wrote[/green] {args.output_json}")
    console.print(f"[green]wrote[/green] {args.output_html}")


if __name__ == "__main__":
    main()
