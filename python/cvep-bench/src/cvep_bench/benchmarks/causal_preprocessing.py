from __future__ import annotations

import argparse
from contextlib import contextmanager
from pathlib import Path
from typing import Any, Iterator

import mne
import numpy as np
from rich.console import Console
from scipy import signal

import cvep_bench.datasets.loaders as loaders
from cvep_bench.benchmarks.orchestration import ensure_output_dirs, resolve_subjects
from cvep_bench.benchmarks.pyntbci_vs_rust import (
    DEFAULT_DATA_DIR,
    benchmark_subject_fold_windows,
)
from cvep_bench.benchmarks.reporting import (
    render_rich_table,
    render_tabular_html,
    write_json_payload,
)
from cvep_bench.cli.arg_groups import (
    add_adc_args,
    add_data_dir_arg,
    add_dataset_args,
    add_fold_args,
    add_output_args,
    add_target_fs_args,
    add_window_args,
    resolve_fold_indices,
)
from cvep_bench.datasets.loaders import load_subject, trial_seconds_for_dataset
from cvep_bench.datasets.profiles import default_preprocessing_options
from cvep_bench.datasets.windows import decode_window_requests


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    add_data_dir_arg(parser, DEFAULT_DATA_DIR)
    add_output_args(
        parser,
        output_dir=DEFAULT_DATA_DIR,
        stem="benchmark_causal_vs_reference",
        include_csv=False,
    )
    parser.add_argument(
        "--algorithms", nargs="+", choices=["etrca", "rcca"], default=["etrca", "rcca"]
    )
    add_dataset_args(parser, default_datasets=["Thielen2021", "CastillosCVEP100"])
    add_fold_args(parser)
    add_target_fs_args(parser, default=250, include_grid=True)
    add_window_args(parser, default_grid=None, include_step=True)
    add_adc_args(parser)
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
    original = loaders.epoch_and_resample
    loaders.epoch_and_resample = causal_epoch_and_resample_factory(
        band_low, band_high, band_order, notch_q
    )
    try:
        yield
    finally:
        loaders.epoch_and_resample = original


def grouped_summary(rows: list[dict[str, Any]]) -> list[dict[str, Any]]:
    grouped: dict[tuple[str, str, int, float], list[dict[str, Any]]] = {}
    for row in rows:
        key = (
            row["algorithm"],
            row["dataset"],
            row["target_fs"],
            row["requested_window_seconds"],
        )
        grouped.setdefault(key, []).append(row)
    out = []
    for (algorithm, dataset, target_fs, requested_window_seconds), members in sorted(
        grouped.items()
    ):
        out.append(
            {
                "algorithm": algorithm,
                "dataset": dataset,
                "target_fs": target_fs,
                "requested_window_seconds": requested_window_seconds,
                "subjects": len({row["subject"] for row in members}),
                "reference_accuracy": float(
                    np.mean([row["reference_accuracy"] for row in members])
                ),
                "causal_accuracy": float(
                    np.mean([row["causal_accuracy"] for row in members])
                ),
                "causal_minus_reference": float(
                    np.mean([row["causal_minus_reference"] for row in members])
                ),
            }
        )
    return out


def serialize_config(args: argparse.Namespace) -> dict[str, Any]:
    config = vars(args).copy()
    for key, value in list(config.items()):
        if isinstance(value, Path):
            config[key] = str(value)
    return config


def main() -> None:
    args = parse_args()
    ensure_output_dirs([args.output_json, args.output_html])
    target_fs_grid = args.target_fs_grid or [args.target_fs]
    fold_indices = resolve_fold_indices(args.folds, args.fold_index)
    results = []
    for dataset in args.datasets:
        subjects = resolve_subjects(dataset, args.subjects, args.max_subjects)
        for subject in subjects:
            for target_fs in target_fs_grid:
                full_trial_seconds = trial_seconds_for_dataset(dataset)
                window_requests = decode_window_requests(
                    full_trial_seconds,
                    args.window_seconds_grid,
                    args.window_step_seconds,
                )
                reference_data = load_subject(
                    dataset, subject, args.data_dir, target_fs
                )
                with causal_loader_patch(
                    args.band_low, args.band_high, args.band_order, args.notch_q
                ):
                    causal_data = load_subject(
                        dataset, subject, args.data_dir, target_fs
                    )
                for algorithm in args.algorithms:
                    for fold_idx in fold_indices:
                        ref_rows = benchmark_subject_fold_windows(
                            algorithm,
                            reference_data,
                            None,
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
                        causal_rows = benchmark_subject_fold_windows(
                            algorithm,
                            causal_data,
                            None,
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
    payload = {"config": serialize_config(args), "results": results}
    write_json_payload(args.output_json, payload)
    summary = grouped_summary(results)
    render_tabular_html(
        args.output_html,
        title="Causal vs Reference Preprocessing",
        subtitle="Compare reference MNE preprocessing against causal SOS filtering before epoch extraction.",
        config=payload["config"],
        summary_columns=[
            ("Algorithm", "algorithm"),
            ("Dataset", "dataset"),
            ("fs", "target_fs"),
            ("Window", "requested_window_seconds"),
            ("Subjects", "subjects"),
            ("Reference", "reference_accuracy"),
            ("Causal", "causal_accuracy"),
            ("Delta", "causal_minus_reference"),
        ],
        summary_rows=summary,
        detail_columns=[
            ("Algorithm", "algorithm"),
            ("Dataset", "dataset"),
            ("Subject", "subject"),
            ("Fold", "fold_index"),
            ("fs", "target_fs"),
            ("Window", "requested_window_seconds"),
            ("Reference", "reference_accuracy"),
            ("Causal", "causal_accuracy"),
            ("Delta", "causal_minus_reference"),
        ],
        detail_rows=results,
    )
    render_rich_table(
        Console(),
        title="Causal vs Reference Preprocessing",
        columns=[
            ("algorithm", "algorithm"),
            ("dataset", "dataset"),
            ("fs", "target_fs"),
            ("window", "requested_window_seconds"),
            ("reference", "reference_accuracy"),
            ("causal", "causal_accuracy"),
            ("delta", "causal_minus_reference"),
        ],
        rows=summary[:20],
        formatters={
            "requested_window_seconds": lambda value: f"{value:.3f}",
            "reference_accuracy": lambda value: f"{value:.4f}",
            "causal_accuracy": lambda value: f"{value:.4f}",
            "causal_minus_reference": lambda value: f"{value:.4f}",
        },
    )
