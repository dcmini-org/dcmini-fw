from __future__ import annotations

from typing import Any

import numpy as np


def mean_trial_correlation(
    lhs: np.ndarray, rhs: np.ndarray
) -> dict[str, float | list[float]]:
    corrs = []
    for idx in range(lhs.shape[0]):
        a = lhs[idx].reshape(-1) - lhs[idx].mean()
        b = rhs[idx].reshape(-1) - rhs[idx].mean()
        denom = np.linalg.norm(a) * np.linalg.norm(b)
        corrs.append(float(a.dot(b) / denom) if denom else 0.0)
    return {
        "mean": float(np.mean(corrs)),
        "min": float(np.min(corrs)),
        "max": float(np.max(corrs)),
        "first10": [float(v) for v in corrs[:10]],
    }


def compare_outputs(python: np.ndarray, rust: np.ndarray) -> dict[str, Any]:
    delta = rust - python
    abs_delta = np.abs(delta)
    return {
        "mae": float(np.mean(abs_delta)),
        "rmse": float(np.sqrt(np.mean(delta**2))),
        "max_abs_error": float(np.max(abs_delta)),
        "channel_metrics": [
            {
                "channel": ch,
                "mae": float(np.mean(abs_delta[:, ch])),
                "rmse": float(np.sqrt(np.mean(delta[:, ch] ** 2))),
                "max_abs_error": float(np.max(abs_delta[:, ch])),
                "python_std": float(np.std(python[:, ch])),
                "rust_std": float(np.std(rust[:, ch])),
            }
            for ch in range(python.shape[1])
        ],
        "first_frame_python": python[0].tolist(),
        "first_frame_rust": rust[0].tolist(),
        "last_frame_python": python[-1].tolist(),
        "last_frame_rust": rust[-1].tolist(),
    }
