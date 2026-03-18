from __future__ import annotations

from pathlib import Path

import numpy as np


def chronological_split(
    x: np.ndarray, y: np.ndarray, folds: int, fold_index: int
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


def load_npz_dataset(
    path: Path, trial_seconds: float
) -> tuple[np.ndarray, np.ndarray, int, dict[str, np.ndarray]]:
    raw = np.load(path)
    fs = int(np.asarray(raw["fs"]).item())
    n_samples = int(round(trial_seconds * fs))
    x = np.asarray(raw["X"], dtype=np.float64)[:, :, :n_samples]
    y = np.asarray(raw["y"], dtype=np.int64)
    extra = {
        key: np.asarray(raw[key]) for key in raw.files if key not in {"X", "y", "fs"}
    }
    return x, y, fs, extra


def class_accuracy(y_true: np.ndarray, y_pred: np.ndarray) -> float:
    return float(np.mean(y_true == y_pred))
