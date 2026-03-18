from __future__ import annotations

import argparse
import html
import json
from contextlib import contextmanager
from pathlib import Path
from typing import Any, Iterator

import numpy as np
import mne
from rich.console import Console
from rich.table import Table
from scipy import signal

from cvep_bench.benchmarks import pyntbci_vs_rust as benchmark
from cvep_bench.benchmarks.pyntbci_vs_rust import DEFAULT_DATA_DIR
from cvep_bench.datasets.profiles import default_preprocessing_options


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data-dir", type=Path, default=DEFAULT_DATA_DIR)
    parser.add_argument(
        "--output-json",
        type=Path,
        default=DEFAULT_DATA_DIR / "benchmark_causal_vs_reference.json",
    )
    parser.add_argument(
        "--output-html",
        type=Path,
        default=DEFAULT_DATA_DIR / "benchmark_causal_vs_reference.html",
    )
    parser.add_argument(
        "--algorithms", nargs="+", choices=["etrca", "rcca"], default=["etrca", "rcca"]
    )
    parser.add_argument(
        "--datasets", nargs="+", default=["Thielen2021", "CastillosCVEP100"]
    )
    parser.add_argument("--subjects", type=int, nargs="+", default=None)
    parser.add_argument("--max-subjects", type=int, default=None)
    parser.add_argument("--folds", type=int, default=5)
    parser.add_argument("--fold-index", type=int, nargs="+", default=None)
    parser.add_argument("--target-fs", type=int, default=250)
    parser.add_argument("--target-fs-grid", type=int, nargs="+", default=None)
    parser.add_argument("--window-step-seconds", type=float, default=None)
    parser.add_argument("--window-seconds-grid", type=float, nargs="+", default=None)
    parser.add_argument("--adc-bits", type=int, default=24)
    parser.add_argument("--adc-headroom", type=float, default=0.95)
    parser.add_argument("--encoding-length", type=float, default=0.3)
    parser.add_argument("--event", type=str, default="refe")
    parser.add_argument("--band-low", type=float, default=1.0)
    parser.add_argument("--band-high", type=float, default=65.0)
    parser.add_argument("--band-order", type=int, default=4)
    parser.add_argument("--notch-q", type=float, default=30.0)
    return parser.parse_args()


def design_causal_sos(
    fs: float, band_low: float, band_high: float, band_order: int, notch_q: float
) -> np.ndarray:
    sos_parts = [
        signal.butter(
            band_order, [band_low, band_high], btype="bandpass", output="sos", fs=fs
        )
    ]
    harmonic = 50.0
    while harmonic < fs / 2.0:
        b, a = signal.iirnotch(harmonic, notch_q, fs)
        sos_parts.append(signal.tf2sos(b, a))
        harmonic += 50.0
    return np.concatenate(sos_parts, axis=0).astype(np.float64)


def causal_epoch_and_resample_factory(
    band_low: float, band_high: float, band_order: int, notch_q: float
):
    def epoch_and_resample(
        raw, events, target_fs, tmin, tmax, event_id=None, preprocessing=None
    ):
        eeg_picks = mne.pick_types(raw.info, eeg=True, exclude=())
        continuous = raw.get_data(picks=eeg_picks).astype(np.float64, copy=False)
        sos = design_causal_sos(
            float(raw.info["sfreq"]), band_low, band_high, band_order, notch_q
        )
        raw._data[eeg_picks, :] = signal.sosfilt(sos, continuous, axis=1)
        epochs = mne.Epochs(
            raw,
            events=events,
            event_id=event_id,
            tmin=tmin - default_preprocessing_options().pretrial_buffer_seconds,
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
    band_low: float, band_high: float, band_order: int, notch_q: float
) -> Iterator[None]:
    original = benchmark.load_subject.__globals__["epoch_and_resample"]
    benchmark.load_subject.__globals__["epoch_and_resample"] = (
        causal_epoch_and_resample_factory(band_low, band_high, band_order, notch_q)
    )
    try:
        yield
    finally:
        benchmark.load_subject.__globals__["epoch_and_resample"] = original


def render_html(output: Path, payload: dict[str, Any]) -> None:
    rows = "\n".join(
        (
            "<tr>"
            f"<td>{html.escape(row['algorithm'])}</td><td>{html.escape(row['dataset'])}</td><td>{row['target_fs']}</td><td>{row['requested_window_seconds']:.3f}</td><td>{row['reference_accuracy']:.4f}</td><td>{row['causal_accuracy']:.4f}</td><td>{row['causal_minus_reference']:.4f}</td>"
            "</tr>"
        )
        for row in payload["results"]
    )
    output.write_text(
        f"<!doctype html><html lang='en'><body><pre>{html.escape(json.dumps(payload['config'], indent=2))}</pre><table><tbody>{rows}</tbody></table></body></html>",
        encoding="utf-8",
    )


def main() -> None:
    args = parse_args()
    rust_binary = None
    target_fs_grid = args.target_fs_grid or [args.target_fs]
    fold_indices = (
        args.fold_index if args.fold_index is not None else list(range(args.folds))
    )
    results = []
    for dataset in args.datasets:
        subjects = args.subjects or benchmark.subject_list_for_dataset(dataset)
        if args.max_subjects is not None:
            subjects = subjects[: args.max_subjects]
        for subject in subjects:
            for target_fs in target_fs_grid:
                full_trial_seconds = benchmark.trial_seconds_for_dataset(dataset)
                window_requests = benchmark.decode_window_requests(
                    full_trial_seconds,
                    args.window_seconds_grid,
                    args.window_step_seconds,
                )
                reference_data = benchmark.load_subject(
                    dataset, subject, args.data_dir, target_fs
                )
                with causal_loader_patch(
                    args.band_low, args.band_high, args.band_order, args.notch_q
                ):
                    causal_data = benchmark.load_subject(
                        dataset, subject, args.data_dir, target_fs
                    )
                for algorithm in args.algorithms:
                    for fold_idx in fold_indices:
                        ref_rows = benchmark.benchmark_subject_fold_windows(
                            algorithm,
                            reference_data,
                            rust_binary,
                            fold_idx,
                            args.folds,
                            window_requests,
                            args.adc_bits,
                            args.adc_headroom,
                            args.encoding_length,
                            args.event,
                            default_preprocessing_options(),
                            "reference",
                        )
                        causal_rows = benchmark.benchmark_subject_fold_windows(
                            algorithm,
                            causal_data,
                            rust_binary,
                            fold_idx,
                            args.folds,
                            window_requests,
                            args.adc_bits,
                            args.adc_headroom,
                            args.encoding_length,
                            args.event,
                            default_preprocessing_options(),
                            "causal",
                        )
                        for ref_row, causal_row in zip(
                            ref_rows, causal_rows, strict=True
                        ):
                            results.append(
                                {
                                    "algorithm": algorithm,
                                    "dataset": dataset,
                                    "subject": subject,
                                    "fold_index": fold_idx,
                                    "target_fs": target_fs,
                                    "requested_window_seconds": ref_row[
                                        "requested_window_seconds"
                                    ],
                                    "reference_accuracy": ref_row["pyntbci_accuracy"],
                                    "causal_accuracy": causal_row["pyntbci_accuracy"],
                                    "causal_minus_reference": causal_row[
                                        "pyntbci_accuracy"
                                    ]
                                    - ref_row["pyntbci_accuracy"],
                                }
                            )
    payload = {"config": vars(args), "results": results}
    args.output_json.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    render_html(args.output_html, payload)
    table = Table(title="Causal vs Reference Preprocessing")
    for col in ["algorithm", "dataset", "fs", "window", "reference", "causal", "delta"]:
        table.add_column(col)
    for row in results[:20]:
        table.add_row(
            row["algorithm"],
            row["dataset"],
            str(row["target_fs"]),
            f"{row['requested_window_seconds']:.3f}",
            f"{row['reference_accuracy']:.4f}",
            f"{row['causal_accuracy']:.4f}",
            f"{row['causal_minus_reference']:.4f}",
        )
    Console().print(table)
