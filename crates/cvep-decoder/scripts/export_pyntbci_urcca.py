#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10"
# dependencies = [
#   "numpy>=1.24.4",
#   "pyntbci>=1.8",
# ]
# ///
"""Export a fixed-length urCCA encoding bank from a PyntBCI-style dataset."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import numpy as np


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--trial-seconds", type=float, default=4.2)
    parser.add_argument("--fold-index", type=int, default=0)
    parser.add_argument("--folds", type=int, default=5)
    parser.add_argument("--event", type=str, default="refe")
    parser.add_argument("--encoding-length", type=float, default=0.3)
    parser.add_argument("--onset-event", action="store_true")
    parser.add_argument("--metadata-json", type=Path, default=None)
    parser.add_argument("--fixture-json", type=Path, default=None)
    parser.add_argument("--state-dump-json", type=Path, default=None)
    parser.add_argument("--state-dump-trials", type=int, default=2)
    return parser.parse_args()


def chronological_split(
    x: np.ndarray, y: np.ndarray, folds: int, fold_index: int
) -> tuple[np.ndarray, np.ndarray]:
    n_trials = x.shape[0]
    if n_trials % folds != 0:
        raise ValueError(
            f"Expected trial count divisible by folds, got {n_trials=} and {folds=}"
        )
    split = np.repeat(np.arange(folds), n_trials // folds)
    test_mask = split == fold_index
    return x[test_mask], y[test_mask]


def main() -> None:
    args = parse_args()

    try:
        import pyntbci
    except ImportError as exc:
        raise SystemExit(
            "pyntbci is required for this export. Run the script with "
            "`uv run --script` or install `pyntbci` into the active environment."
        ) from exc

    raw = np.load(args.input)
    fs = int(np.asarray(raw["fs"]).item())
    n_samples = int(round(args.trial_seconds * fs))
    x = np.asarray(raw["X"], dtype=np.float64)[:, :, :n_samples]
    y = np.asarray(raw["y"], dtype=np.int64)
    stimulus = np.asarray(raw["V"], dtype=np.float64)

    model = pyntbci.classifiers.urCCA(
        stimulus=stimulus,
        fs=fs,
        event=args.event,
        onset_event=args.onset_event,
        encoding_length=args.encoding_length,
    )

    if n_samples < model.Ms.shape[2]:
        encodings = model.Ms[:, :, :n_samples].copy()
    else:
        repeats = int(np.ceil(n_samples / model.Ms.shape[2]))
        encodings = np.concatenate(
            (model.Ms, np.tile(model.Mw, (1, 1, repeats))), axis=2
        )[:, :, :n_samples]

    x_test, y_test = chronological_split(x, y, args.folds, args.fold_index)
    online_pred = []
    online_model = pyntbci.classifiers.urCCA(
        stimulus=stimulus,
        fs=fs,
        event=args.event,
        onset_event=args.onset_event,
        encoding_length=args.encoding_length,
    )
    state_snapshots: list[dict[str, object]] = []
    for idx in range(x_test.shape[0]):
        online_model.fit(x_test[idx])
        pred = int(online_model.predict())
        online_pred.append(pred)
        online_model.update(pred)
        if idx < args.state_dump_trials:
            cca = online_model.ccas[0]
            state_snapshots.append(
                {
                    "trial_index": idx,
                    "predicted_class": pred,
                    "scores": np.asarray(online_model.rho, dtype=np.float32).tolist(),
                    "samples_seen": int(cca.n_x_),
                    "avg_x": np.asarray(cca.avg_x_).reshape(-1).astype(np.float32).tolist(),
                    "avg_y": np.asarray(cca.avg_y_).reshape(-1).astype(np.float32).tolist(),
                    "cov_x": np.asarray(cca.cov_x_, dtype=np.float32).tolist(),
                    "cov_y": np.asarray(cca.cov_y_, dtype=np.float32).tolist(),
                    "cov_xy": np.asarray(
                        cca.cov_xy_[: x.shape[1], x.shape[1] :], dtype=np.float32
                    ).tolist(),
                }
            )

    online_pred = np.asarray(online_pred, dtype=np.int64)
    online_accuracy = float(np.mean(online_pred == y_test))

    metadata = {
        "source": "pyntbci_urcca",
        "input": str(args.input),
        "trial_seconds": args.trial_seconds,
        "folds": args.folds,
        "fold_index": args.fold_index,
        "event": args.event,
        "encoding_length": args.encoding_length,
        "onset_event": args.onset_event,
        "classes": int(stimulus.shape[0]),
        "features": int(encodings.shape[1]),
        "channels": int(x.shape[1]),
        "samples": int(n_samples),
        "stimulus_cycle_samples": int(stimulus.shape[1]),
        "pyntbci_online_accuracy": online_accuracy,
    }

    np.savez_compressed(
        args.output,
        encodings=encodings.astype(np.float32),
        stimulus=stimulus.astype(np.uint8),
        fs=np.asarray(fs, dtype=np.int64),
        class_labels=np.arange(stimulus.shape[0], dtype=np.int64),
        benchmark_predictions=online_pred,
        benchmark_labels=y_test.astype(np.int64),
        metadata=np.asarray(json.dumps(metadata)),
    )

    print(f"input={args.input}")
    print(f"output={args.output}")
    print(
        f"classes={stimulus.shape[0]} channels={x.shape[1]} "
        f"features={encodings.shape[1]} samples={n_samples} fs={fs}"
    )
    print(f"pyntbci_online_accuracy={online_accuracy:.4f}")

    if args.metadata_json is not None:
        args.metadata_json.write_text(
            json.dumps(metadata, indent=2) + "\n", encoding="utf-8"
        )

    if args.fixture_json is not None:
        fixture = {
            "classes": int(stimulus.shape[0]),
            "channels": int(x.shape[1]),
            "features": int(encodings.shape[1]),
            "window": int(n_samples),
            "encodings": encodings.astype(np.float32).tolist(),
            "trials": x_test.astype(np.float32).tolist(),
            "benchmark_predictions": online_pred.tolist(),
            "benchmark_labels": y_test.astype(np.int64).tolist(),
            "regularization": 0.0,
            "reference_states": state_snapshots,
        }
        args.fixture_json.write_text(
            json.dumps(fixture), encoding="utf-8"
        )

    if args.state_dump_json is not None:
        args.state_dump_json.write_text(
            json.dumps(state_snapshots), encoding="utf-8"
        )


if __name__ == "__main__":
    main()
