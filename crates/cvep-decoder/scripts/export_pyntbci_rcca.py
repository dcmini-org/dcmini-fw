#!/usr/bin/env python3
# /// script
# dependencies = [
#   "numpy>=1.24.4",
#   "pyntbci>=1.8",
# ]
# ///
"""Export an exact Rust-friendly rCCA bank from a PyntBCI dataset."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any

import numpy as np


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--trial-seconds", type=float, default=4.2)
    parser.add_argument("--fold-index", type=int, default=0)
    parser.add_argument("--folds", type=int, default=5)
    parser.add_argument("--encoding-length", type=float, default=0.3)
    parser.add_argument("--event", type=str, default="refe")
    parser.add_argument("--ensemble", action="store_true")
    parser.add_argument("--metadata-json", type=Path, default=None)
    return parser.parse_args()


def load_dataset(
    path: Path,
    trial_seconds: float,
) -> tuple[np.ndarray, np.ndarray, np.ndarray, int]:
    raw = np.load(path)
    fs = int(np.asarray(raw["fs"]).item())
    n_samples = int(round(trial_seconds * fs))
    x = np.asarray(raw["X"], dtype=np.float64)[:, :, :n_samples]
    y = np.asarray(raw["y"], dtype=np.int64)
    stimulus = np.asarray(raw["V"], dtype=np.float64)
    return x, y, stimulus, fs


def chronological_split(
    x: np.ndarray,
    y: np.ndarray,
    folds: int,
    fold_index: int,
) -> tuple[np.ndarray, np.ndarray, np.ndarray, np.ndarray]:
    n_trials = x.shape[0]
    if n_trials % folds != 0:
        raise ValueError(
            f"Expected trial count divisible by folds, got {n_trials=} and {folds=}"
        )
    if not 0 <= fold_index < folds:
        raise ValueError(f"fold_index must be in [0, {folds}), got {fold_index}")

    split = np.repeat(np.arange(folds), n_trials // folds)
    train_mask = split != fold_index
    test_mask = ~train_mask
    return x[train_mask], y[train_mask], x[test_mask], y[test_mask]


def fit_rcca(
    x_train: np.ndarray,
    y_train: np.ndarray,
    stimulus: np.ndarray,
    fs: int,
    event: str,
    encoding_length: float,
    ensemble: bool,
) -> Any:
    try:
        import pyntbci
    except ImportError as exc:
        raise SystemExit(
            "pyntbci is required for this export. Run the script with "
            "`uv run --script` or install `pyntbci` into the active environment."
        ) from exc

    model = pyntbci.classifiers.rCCA(
        stimulus=stimulus,
        fs=fs,
        event=event,
        encoding_length=encoding_length,
        score_metric="correlation",
        ensemble=ensemble,
        n_components=1,
    )
    model.fit(x_train, y_train)
    return model


def full_templates(model: Any, n_samples: int) -> np.ndarray:
    if n_samples < model.Ts_.shape[2]:
        templates = model.Ts_
    else:
        repeats = n_samples // model.Ts_.shape[2]
        templates = np.concatenate(
            (model.Ts_, np.tile(model.Tw_, (1, 1, repeats))), axis=2
        )
    templates = templates[:, :, :n_samples].copy()
    templates -= templates.mean(axis=2, keepdims=True)
    return templates


def export_runtime_bank(
    model: Any,
    n_classes: int,
    n_channels: int,
    n_samples: int,
) -> tuple[np.ndarray, np.ndarray]:
    templates = full_templates(model, n_samples)[:, 0, :]

    if model.w_.ndim == 2:
        spatial_filters = np.repeat(model.w_[:, 0][np.newaxis, :], n_classes, axis=0)
    else:
        spatial_filters = np.asarray(model.w_[:, 0, :]).T

    if spatial_filters.shape != (n_classes, n_channels):
        raise ValueError(
            f"Unexpected spatial filter shape {spatial_filters.shape}, expected {(n_classes, n_channels)}"
        )

    return spatial_filters, templates


def exact_projected_predict(
    x: np.ndarray,
    spatial_filters: np.ndarray,
    templates: np.ndarray,
    class_labels: np.ndarray,
) -> np.ndarray:
    scores = np.zeros((x.shape[0], spatial_filters.shape[0]), dtype=np.float64)
    template_norms = np.sqrt(np.maximum((templates * templates).sum(axis=1), 1e-12))

    for class_idx in range(spatial_filters.shape[0]):
        projected = np.einsum("tcs,c->ts", x, spatial_filters[class_idx], optimize=True)
        projected -= projected.mean(axis=1, keepdims=True)
        numerator = projected @ templates[class_idx]
        trial_norms = np.sqrt(np.maximum((projected * projected).sum(axis=1), 1e-12))
        scores[:, class_idx] = numerator / (trial_norms * template_norms[class_idx])

    return class_labels[np.argmax(scores, axis=1)]


def class_accuracy(y_true: np.ndarray, y_pred: np.ndarray) -> float:
    return float(np.mean(y_true == y_pred))


def main() -> None:
    args = parse_args()
    x, y, stimulus, fs = load_dataset(args.input, args.trial_seconds)
    x_train, y_train, x_test, y_test = chronological_split(
        x, y, args.folds, args.fold_index
    )

    model = fit_rcca(
        x_train,
        y_train,
        stimulus,
        fs,
        event=args.event,
        encoding_length=args.encoding_length,
        ensemble=args.ensemble,
    )

    model_class_labels = np.asarray(model.classes_, dtype=np.int64)
    n_classes = model_class_labels.shape[0]
    spatial_filters, templates = export_runtime_bank(
        model, n_classes, x.shape[1], x.shape[2]
    )

    pyntbci_pred = np.asarray(model.predict(x_test), dtype=np.int64)
    exact_pred = exact_projected_predict(
        x_test, spatial_filters, templates, model_class_labels
    )

    metadata = {
        "source": "pyntbci_rcca",
        "input": str(args.input),
        "trial_seconds": args.trial_seconds,
        "folds": args.folds,
        "fold_index": args.fold_index,
        "event": args.event,
        "encoding_length": args.encoding_length,
        "ensemble": args.ensemble,
        "classes": int(n_classes),
        "channels": int(x.shape[1]),
        "samples": int(x.shape[2]),
        "stimulus_cycle_samples": int(stimulus.shape[1]),
        "score_metric": "correlation",
        "supports_decoding_matrix": False,
    }

    np.savez_compressed(
        args.output,
        fs=np.asarray(fs, dtype=np.int64),
        class_labels=model_class_labels,
        stimulus=stimulus.astype(np.uint8),
        projected_templates=templates.astype(np.float32),
        spatial_filters=spatial_filters.astype(np.float32),
        projected_template_norms=np.sqrt(
            np.maximum((templates * templates).sum(axis=1), 1e-12)
        ).astype(np.float32),
        pyntbci_accuracy=np.asarray(class_accuracy(y_test, pyntbci_pred), dtype=np.float64),
        rcca_exact_accuracy=np.asarray(class_accuracy(y_test, exact_pred), dtype=np.float64),
        x_train_shape=np.asarray(x_train.shape, dtype=np.int64),
        x_test_shape=np.asarray(x_test.shape, dtype=np.int64),
        metadata=np.asarray(json.dumps(metadata)),
    )

    print(f"input={args.input}")
    print(f"output={args.output}")
    print(f"trials train/test={x_train.shape[0]}/{x_test.shape[0]}")
    print(f"classes={n_classes} channels={x.shape[1]} samples={x.shape[2]} fs={fs}")
    print(f"pyntbci_accuracy={class_accuracy(y_test, pyntbci_pred):.4f}")
    print(f"rcca_exact_accuracy={class_accuracy(y_test, exact_pred):.4f}")

    if args.metadata_json is not None:
        args.metadata_json.write_text(
            json.dumps(metadata, indent=2) + "\n", encoding="utf-8"
        )


if __name__ == "__main__":
    main()
