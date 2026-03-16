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
"""Export UMM epoch features from a downloaded c-VEP dataset subject."""

from __future__ import annotations

import argparse
import importlib.util
import json
from pathlib import Path
import sys
from typing import Any

import numpy as np

from umm_feature_utils import build_umm_features


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
    parser.add_argument("--dataset", required=True)
    parser.add_argument("--subject", type=int, required=True)
    parser.add_argument("--data-dir", type=Path, default=CRATE_ROOT / "data")
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
        "--layout",
        choices=["channel_prime", "time_prime"],
        default="channel_prime",
    )
    parser.add_argument("--trial-demean", action="store_true")
    parser.add_argument("--epoch-demean", action="store_true")
    parser.add_argument("--folds", type=int, default=5)
    parser.add_argument("--fold-index", type=int, default=0)
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    benchmark = load_benchmark_module()
    benchmark.validate_target_fs(args.target_fs)
    data = benchmark.load_subject(args.dataset, args.subject, args.data_dir, args.target_fs)

    fold_parts = benchmark.fold_slices(data.x.shape[0], args.folds)
    test_idx = fold_parts[args.fold_index]
    x_test = data.x[test_idx]
    y_test = data.y[test_idx]

    full_window_seconds = data.trial_seconds
    requested_window_seconds = (
        full_window_seconds if args.window_seconds is None else args.window_seconds
    )
    window_samples = min(
        benchmark.seconds_to_samples(requested_window_seconds, data.fs),
        x_test.shape[2],
    )
    x_window = x_test[:, :, :window_samples]

    exported = build_umm_features(
        trials=x_window,
        stimulus=data.stimulus,
        fs=data.fs,
        presentation_rate=data.presentation_rate,
        epoch_seconds=args.epoch_seconds,
        layout=args.layout,
        epoch_schedule=args.epoch_schedule,
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
        "epoch_start_samples_first10": exported.epoch_start_samples[:10].astype(int).tolist(),
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
        "source_confidence_note": (
            "Accessible UMM sources confirm that confidence depends on the winning "
            "class relative to the runner-up class, but do not expose a single exact "
            "public formula."
        ),
    }

    np.savez_compressed(
        args.output,
        features=exported.features.astype(np.float32),
        codebook=exported.codebook.astype(np.uint8),
        labels=y_test.astype(np.int64),
        metadata=np.asarray(json.dumps(metadata)),
    )

    if args.metadata_json is not None:
        args.metadata_json.write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")

    print(f"output={args.output}")
    print(
        f"dataset={data.dataset} subject={data.subject} trials={exported.features.shape[0]} "
        f"classes={exported.codebook.shape[0]} features={exported.features.shape[1]} "
        f"epochs={exported.epochs_per_trial}"
    )
    print(
        f"layout={exported.layout} schedule={exported.epoch_schedule} "
        f"lag_samples={exported.response_lag_samples} "
        f"epoch_samples={exported.epoch_samples} "
        f"epoch_stride_samples={exported.epoch_stride_samples}"
    )


if __name__ == "__main__":
    main()
