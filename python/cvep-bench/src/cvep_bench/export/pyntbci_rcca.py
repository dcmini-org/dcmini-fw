from __future__ import annotations

import argparse
import json
from pathlib import Path

import numpy as np

from cvep_bench.algorithms.pyntbci_models import build_rcca_bank, fit_rcca
from cvep_bench.export.common import (
    chronological_split,
    class_accuracy,
    load_npz_dataset,
)
from cvep_bench.export.predictors import exact_projected_predict


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


def main() -> None:
    args = parse_args()
    x, y, fs, extra = load_npz_dataset(args.input, args.trial_seconds)
    stimulus = np.asarray(extra["V"], dtype=np.float64)
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
    )
    class_labels = np.asarray(model.classes_, dtype=np.int64)
    spatial_filters, templates = build_rcca_bank(
        model, class_labels.shape[0], x.shape[1], x.shape[2]
    )
    pyntbci_pred = np.asarray(model.predict(x_test), dtype=np.int64)
    exact_pred = exact_projected_predict(
        x_test, spatial_filters, templates, class_labels
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
        "classes": int(class_labels.shape[0]),
        "channels": int(x.shape[1]),
        "samples": int(x.shape[2]),
        "stimulus_cycle_samples": int(stimulus.shape[1]),
        "score_metric": "correlation",
        "supports_decoding_matrix": False,
    }
    np.savez_compressed(
        args.output,
        fs=np.asarray(fs, dtype=np.int64),
        class_labels=class_labels,
        stimulus=stimulus.astype(np.uint8),
        projected_templates=templates.astype(np.float32),
        spatial_filters=spatial_filters.astype(np.float32),
        projected_template_norms=np.sqrt(
            np.maximum((templates * templates).sum(axis=1), 1e-12)
        ).astype(np.float32),
        pyntbci_accuracy=np.asarray(
            class_accuracy(y_test, pyntbci_pred), dtype=np.float64
        ),
        rcca_exact_accuracy=np.asarray(
            class_accuracy(y_test, exact_pred), dtype=np.float64
        ),
        x_train_shape=np.asarray(x_train.shape, dtype=np.int64),
        x_test_shape=np.asarray(x_test.shape, dtype=np.int64),
        metadata=np.asarray(json.dumps(metadata)),
    )
    if args.metadata_json is not None:
        args.metadata_json.write_text(
            json.dumps(metadata, indent=2) + "\n", encoding="utf-8"
        )
