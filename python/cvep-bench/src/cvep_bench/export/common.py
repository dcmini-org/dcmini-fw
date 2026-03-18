from __future__ import annotations

from pathlib import Path

import numpy as np

from cvep_bench.evaluation.splits import chronological_split


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
