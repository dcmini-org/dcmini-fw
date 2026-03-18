from __future__ import annotations

import numpy as np


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
