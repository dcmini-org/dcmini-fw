from __future__ import annotations

import numpy as np


def fold_slices(n_trials: int, folds: int) -> list[np.ndarray]:
    return [
        np.asarray(indices, dtype=np.int64)
        for indices in np.array_split(np.arange(n_trials), folds)
    ]


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


def chronological_test_split(
    x: np.ndarray, y: np.ndarray, folds: int, fold_index: int
) -> tuple[np.ndarray, np.ndarray]:
    _x_train, _y_train, x_test, y_test = chronological_split(x, y, folds, fold_index)
    return x_test, y_test
