#!/usr/bin/env python3
# /// script
# dependencies = [
#   "numpy>=1.24.4",
#   "pyntbci>=1.8",
# ]
# ///
"""Export an exact Rust-friendly eTRCA bank from a PyntBCI dataset."""

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
    parser.add_argument("--subject", type=str, default=None)
    parser.add_argument("--metadata-json", type=Path, default=None)
    parser.add_argument("--fixture-json", type=Path, default=None)
    parser.add_argument("--adc-bits", type=int, default=24)
    parser.add_argument("--adc-headroom", type=float, default=0.95)
    return parser.parse_args()


def load_dataset(
    path: Path, trial_seconds: float
) -> tuple[np.ndarray, np.ndarray, int, dict[str, Any]]:
    raw = np.load(path)
    fs = int(np.asarray(raw["fs"]).item())
    n_samples = int(round(trial_seconds * fs))
    x = np.asarray(raw["X"], dtype=np.float64)[:, :, :n_samples]
    y = np.asarray(raw["y"], dtype=np.int64)

    extra: dict[str, Any] = {}
    if "V" in raw:
        extra["V"] = np.asarray(raw["V"])
    return x, y, fs, extra


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


def fit_etrca(
    x_train: np.ndarray,
    y_train: np.ndarray,
    fs: int,
    cycle_size: float,
) -> Any:
    try:
        import pyntbci
    except ImportError as exc:
        raise SystemExit(
            "pyntbci is required for this export. Run the script with "
            "`uv run --script` or install `pyntbci` into the active environment."
        ) from exc

    model = pyntbci.classifiers.eTRCA(
        lags=None,
        fs=fs,
        cycle_size=cycle_size,
        ensemble=True,
    )
    model.fit(x_train, y_train)
    return model


def export_runtime_bank(
    model: Any,
    x_train: np.ndarray,
    class_labels: np.ndarray,
) -> tuple[np.ndarray, np.ndarray]:
    n_classes = class_labels.shape[0]
    _, n_channels, n_samples = x_train.shape

    if model.w_.ndim == 2:
        spatial = np.repeat(model.w_[:, :, np.newaxis], n_classes, axis=2)
    else:
        spatial = np.asarray(model.w_)

    if spatial.shape[1] != 1:
        raise ValueError(
            f"Expected one spatial component for firmware export, got {spatial.shape}"
        )

    projected_templates = np.asarray(model.get_T(n_samples), dtype=np.float64)[:, 0, :]
    spatial_filters = np.zeros((n_classes, n_channels), dtype=np.float64)

    for class_pos, _class_label in enumerate(class_labels):
        spatial_filters[class_pos] = spatial[:, 0, class_pos]

    return spatial_filters, projected_templates


def exact_etrca_predict(
    x: np.ndarray,
    spatial_filters: np.ndarray,
    projected_templates: np.ndarray,
    class_labels: np.ndarray,
) -> np.ndarray:
    scores = np.zeros((x.shape[0], spatial_filters.shape[0]), dtype=np.float64)
    template_norms = np.sqrt(
        np.maximum((projected_templates * projected_templates).sum(axis=1), 1e-12)
    )

    for class_idx in range(spatial_filters.shape[0]):
        projected = np.einsum("tcs,c->ts", x, spatial_filters[class_idx], optimize=True)
        projected -= projected.mean(axis=1, keepdims=True)
        numerator = projected @ projected_templates[class_idx]
        trial_norms = np.sqrt(np.maximum((projected * projected).sum(axis=1), 1e-12))
        scores[:, class_idx] = numerator / (trial_norms * template_norms[class_idx])

    return class_labels[np.argmax(scores, axis=1)]


def class_accuracy(y_true: np.ndarray, y_pred: np.ndarray) -> float:
    return float(np.mean(y_true == y_pred))


def quantize_trials_to_adc(
    x: np.ndarray,
    signed_bits: int,
    headroom: float,
) -> tuple[np.ndarray, float]:
    if signed_bits < 2:
        raise ValueError(f"signed_bits must be >= 2, got {signed_bits}")
    if not 0.0 < headroom <= 1.0:
        raise ValueError(f"adc_headroom must be in (0, 1], got {headroom}")

    adc_peak = float((1 << (signed_bits - 1)) - 1)
    data_peak = float(np.max(np.abs(x)))
    scale = 1.0 if data_peak == 0.0 else (adc_peak * headroom) / data_peak
    quantized = np.rint(x * scale).clip(-adc_peak - 1.0, adc_peak).astype(np.int32)
    return quantized, scale


def export_bank(
    output: Path,
    *,
    x_train: np.ndarray,
    x_test: np.ndarray,
    y_test: np.ndarray,
    fs: int,
    model: Any,
    projected_templates: np.ndarray,
    spatial_filters: np.ndarray,
    class_labels: np.ndarray,
    metadata: dict[str, Any],
) -> None:
    pyntbci_pred = np.asarray(model.predict(x_test), dtype=np.int64)
    exact_pred = exact_etrca_predict(
        x_test, spatial_filters, projected_templates, class_labels
    )

    np.savez_compressed(
        output,
        fs=np.asarray(fs, dtype=np.int64),
        class_labels=class_labels.astype(np.int64),
        projected_templates=projected_templates.astype(np.float32),
        spatial_filters=spatial_filters.astype(np.float32),
        projected_template_norms=np.sqrt(
            np.maximum((projected_templates * projected_templates).sum(axis=1), 1e-12)
        ).astype(np.float32),
        pyntbci_accuracy=np.asarray(class_accuracy(y_test, pyntbci_pred), dtype=np.float64),
        etrca_exact_accuracy=np.asarray(class_accuracy(y_test, exact_pred), dtype=np.float64),
        x_train_shape=np.asarray(x_train.shape, dtype=np.int64),
        x_test_shape=np.asarray(x_test.shape, dtype=np.int64),
        metadata=np.asarray(json.dumps(metadata)),
    )


def main() -> None:
    args = parse_args()

    x, y, fs, extra = load_dataset(args.input, args.trial_seconds)
    class_labels = np.asarray(np.unique(y), dtype=np.int64)
    x_train, y_train, x_test, y_test = chronological_split(
        x, y, args.folds, args.fold_index
    )

    cycle_size = 2.1
    model = fit_etrca(x_train, y_train, fs, cycle_size)
    model_class_labels = np.asarray(model.classes_, dtype=np.int64)
    spatial_filters, projected_templates = export_runtime_bank(
        model, x_train, model_class_labels
    )

    metadata = {
        "source": "pyntbci_etrca",
        "input": str(args.input),
        "subject": args.subject,
        "trial_seconds": args.trial_seconds,
        "folds": args.folds,
        "fold_index": args.fold_index,
        "cycle_size": cycle_size,
        "ensemble": True,
        "classes": int(class_labels.shape[0]),
        "channels": int(x.shape[1]),
        "samples": int(x.shape[2]),
        "has_codes": "V" in extra,
        "fixture_adc_bits": args.adc_bits,
        "fixture_adc_headroom": args.adc_headroom,
    }

    export_bank(
        args.output,
        x_train=x_train,
        x_test=x_test,
        y_test=y_test,
        fs=fs,
        model=model,
        projected_templates=projected_templates,
        spatial_filters=spatial_filters,
        class_labels=model_class_labels,
        metadata=metadata,
    )

    pyntbci_pred = np.asarray(model.predict(x_test), dtype=np.int64)
    pyntbci_accuracy = class_accuracy(y_test, pyntbci_pred)
    exact_accuracy = class_accuracy(
        y_test,
        exact_etrca_predict(
            x_test, spatial_filters, projected_templates, model_class_labels
        ),
    )

    print(f"input={args.input}")
    print(f"output={args.output}")
    print(f"trials train/test={x_train.shape[0]}/{x_test.shape[0]}")
    print(f"classes={class_labels.shape[0]} channels={x.shape[1]} samples={x.shape[2]} fs={fs}")
    print(f"pyntbci_accuracy={pyntbci_accuracy:.4f}")
    print(f"etrca_exact_accuracy={exact_accuracy:.4f}")

    if args.metadata_json is not None:
        args.metadata_json.write_text(
            json.dumps(metadata, indent=2) + "\n", encoding="utf-8"
        )

    if args.fixture_json is not None:
        fixture_trials, adc_scale = quantize_trials_to_adc(
            x_test,
            signed_bits=args.adc_bits,
            headroom=args.adc_headroom,
        )
        fixture = {
            "classes": int(class_labels.shape[0]),
            "channels": int(x.shape[1]),
            "window": int(x.shape[2]),
            "adc_bits": int(args.adc_bits),
            "adc_scale": float(adc_scale),
            "class_labels": model_class_labels.astype(np.int64).tolist(),
            "spatial_filters": spatial_filters.astype(np.float32).tolist(),
            "projected_templates": projected_templates.astype(np.float32).tolist(),
            "benchmark_predictions": pyntbci_pred.astype(np.int64).tolist(),
            "benchmark_labels": y_test.astype(np.int64).tolist(),
            "trials_i32": fixture_trials.tolist(),
        }
        args.fixture_json.write_text(json.dumps(fixture), encoding="utf-8")


if __name__ == "__main__":
    main()
