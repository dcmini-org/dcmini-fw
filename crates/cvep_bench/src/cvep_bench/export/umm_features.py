from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import cast

import numpy as np

from cvep_bench.algorithms.umm_features import (
    EpochScheduleName,
    LayoutName,
    build_umm_features,
)
from cvep_bench.datasets.loaders import load_subject, validate_target_fs
from cvep_bench.datasets.windows import fold_slices, seconds_to_samples


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dataset", required=True)
    parser.add_argument("--subject", type=int, required=True)
    parser.add_argument(
        "--data-dir", type=Path, default=Path("crates/cvep-decoder/data")
    )
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--metadata-json", type=Path, default=None)
    parser.add_argument("--target-fs", type=int, default=250)
    parser.add_argument("--window-seconds", type=float, default=None)
    parser.add_argument("--epoch-seconds", type=float, default=0.3)
    parser.add_argument(
        "--epoch-schedule",
        choices=["rounded_stride", "fractional_onset"],
        default="fractional_onset",
    )
    parser.add_argument("--response-lag-seconds", type=float, default=0.0)
    parser.add_argument(
        "--layout", choices=["channel_prime", "time_prime"], default="channel_prime"
    )
    parser.add_argument("--trial-demean", action="store_true")
    parser.add_argument("--epoch-demean", action="store_true")
    parser.add_argument("--folds", type=int, default=5)
    parser.add_argument("--fold-index", type=int, default=0)
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    validate_target_fs(args.target_fs)
    data = load_subject(args.dataset, args.subject, args.data_dir, args.target_fs)
    fold_parts = fold_slices(data.x.shape[0], args.folds)
    test_idx = fold_parts[args.fold_index]
    x_test = data.x[test_idx]
    y_test = data.y[test_idx]
    requested_window_seconds = (
        data.trial_seconds if args.window_seconds is None else args.window_seconds
    )
    window_samples = min(
        seconds_to_samples(requested_window_seconds, data.fs), x_test.shape[2]
    )
    x_window = x_test[:, :, :window_samples]
    exported = build_umm_features(
        trials=x_window,
        stimulus=data.stimulus,
        fs=data.fs,
        presentation_rate=data.presentation_rate,
        epoch_seconds=args.epoch_seconds,
        layout=cast(LayoutName, args.layout),
        epoch_schedule=cast(EpochScheduleName, args.epoch_schedule),
        response_lag_seconds=args.response_lag_seconds,
        trial_demean=args.trial_demean,
        epoch_demean=args.epoch_demean,
    )
    metadata = {
        "dataset": data.dataset,
        "subject": data.subject,
        "target_fs": data.fs,
        "presentation_rate": data.presentation_rate,
        "trial_seconds": data.trial_seconds,
        "requested_window_seconds": requested_window_seconds,
        "window_seconds": window_samples / data.fs,
        "window_samples": int(window_samples),
        "epoch_seconds": args.epoch_seconds,
        "epoch_schedule": exported.epoch_schedule,
        "response_lag_seconds": args.response_lag_seconds,
        "response_lag_samples": exported.response_lag_samples,
        "epoch_samples": exported.epoch_samples,
        "epoch_stride_samples": exported.epoch_stride_samples,
        "epochs_per_trial": exported.epochs_per_trial,
        "epoch_start_samples_first10": exported.epoch_start_samples[:10]
        .astype(int)
        .tolist(),
        "layout": exported.layout,
        "n_channels": exported.n_channels,
        "n_timepoints": exported.n_timepoints,
        "feature_count": exported.features.shape[1],
        "classes": int(exported.codebook.shape[0]),
        "trials": int(exported.features.shape[0]),
        "trial_demean": exported.trial_demean,
        "epoch_demean": exported.epoch_demean,
        "folds": args.folds,
        "fold_index": args.fold_index,
        "source_confidence_note": "Accessible UMM sources confirm that confidence depends on the winning class relative to the runner-up class, but do not expose a single exact public formula.",
    }
    np.savez_compressed(
        args.output,
        features=exported.features.astype(np.float32),
        codebook=exported.codebook.astype(np.uint8),
        labels=y_test.astype(np.int64),
        metadata=np.asarray(json.dumps(metadata)),
    )
    if args.metadata_json is not None:
        args.metadata_json.write_text(
            json.dumps(metadata, indent=2) + "\n", encoding="utf-8"
        )
    print(f"output={args.output}")
    print(
        f"dataset={data.dataset} subject={data.subject} trials={exported.features.shape[0]} classes={exported.codebook.shape[0]} features={exported.features.shape[1]} epochs={exported.epochs_per_trial}"
    )
    print(
        f"layout={exported.layout} schedule={exported.epoch_schedule} lag_samples={exported.response_lag_samples} epoch_samples={exported.epoch_samples} epoch_stride_samples={exported.epoch_stride_samples}"
    )
