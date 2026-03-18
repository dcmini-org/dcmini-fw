from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any

import numpy as np

from cvep_bench.algorithms.pyntbci_models import (
    build_etrca_bank,
    fit_etrca,
    quantize_trials_to_adc,
)
from cvep_bench.export.common import (
    chronological_split,
    class_accuracy,
    load_npz_dataset,
)


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


def main() -> None:
    args = parse_args()
    x, y, fs, extra = load_npz_dataset(args.input, args.trial_seconds)
    x_train, y_train, x_test, y_test = chronological_split(
        x, y, args.folds, args.fold_index
    )
    cycle_size = 2.1
    model = fit_etrca(x_train, y_train, fs, cycle_size)
    model_class_labels = np.asarray(model.classes_, dtype=np.int64)
    spatial_filters, projected_templates = build_etrca_bank(
        model, x_train.shape[2], model_class_labels
    )
    pyntbci_pred = np.asarray(model.predict(x_test), dtype=np.int64)
    exact_pred = exact_etrca_predict(
        x_test, spatial_filters, projected_templates, model_class_labels
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
        "classes": int(model_class_labels.shape[0]),
        "channels": int(x.shape[1]),
        "samples": int(x.shape[2]),
        "has_codes": "V" in extra,
        "fixture_adc_bits": args.adc_bits,
        "fixture_adc_headroom": args.adc_headroom,
    }
    np.savez_compressed(
        args.output,
        fs=np.asarray(fs, dtype=np.int64),
        class_labels=model_class_labels.astype(np.int64),
        projected_templates=projected_templates.astype(np.float32),
        spatial_filters=spatial_filters.astype(np.float32),
        projected_template_norms=np.sqrt(
            np.maximum((projected_templates * projected_templates).sum(axis=1), 1e-12)
        ).astype(np.float32),
        pyntbci_accuracy=np.asarray(
            class_accuracy(y_test, pyntbci_pred), dtype=np.float64
        ),
        etrca_exact_accuracy=np.asarray(
            class_accuracy(y_test, exact_pred), dtype=np.float64
        ),
        x_train_shape=np.asarray(x_train.shape, dtype=np.int64),
        x_test_shape=np.asarray(x_test.shape, dtype=np.int64),
        metadata=np.asarray(json.dumps(metadata)),
    )
    if args.fixture_json is not None:
        quantized_trials, _scale = quantize_trials_to_adc(
            x_test, args.adc_bits, args.adc_headroom
        )
        fixture = {
            "algorithm": "etrca",
            "dataset": args.input.name,
            "subject": args.subject,
            "classes": int(model_class_labels.shape[0]),
            "channels": int(x.shape[1]),
            "window": int(x.shape[2]),
            "spatial_filters": spatial_filters.astype(np.float32).tolist(),
            "projected_templates": projected_templates.astype(np.float32).tolist(),
            "benchmark_predictions": pyntbci_pred.astype(np.int64).tolist(),
            "benchmark_labels": y_test.astype(np.int64).tolist(),
            "trials_i32": quantized_trials.tolist(),
        }
        args.fixture_json.write_text(json.dumps(fixture), encoding="utf-8")
    if args.metadata_json is not None:
        args.metadata_json.write_text(
            json.dumps(metadata, indent=2) + "\n", encoding="utf-8"
        )
