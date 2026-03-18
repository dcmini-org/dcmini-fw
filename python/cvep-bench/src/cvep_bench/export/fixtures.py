from __future__ import annotations

from typing import Any


def etrca_fixture(
    *,
    dataset: str,
    subject: str | None,
    class_labels,
    x,
    spatial_filters,
    projected_templates,
    benchmark_predictions,
    benchmark_labels,
    trials_i32,
) -> dict[str, Any]:
    return {
        "algorithm": "etrca",
        "dataset": dataset,
        "subject": subject,
        "classes": int(class_labels.shape[0]),
        "channels": int(x.shape[1]),
        "window": int(x.shape[2]),
        "spatial_filters": spatial_filters.astype("float32").tolist(),
        "projected_templates": projected_templates.astype("float32").tolist(),
        "benchmark_predictions": benchmark_predictions.astype("int64").tolist(),
        "benchmark_labels": benchmark_labels.astype("int64").tolist(),
        "trials_i32": trials_i32.tolist(),
    }


def urcca_fixture(
    *,
    stimulus,
    x_test,
    encodings,
    benchmark_predictions,
    benchmark_labels,
    regularization: float,
    reference_states=None,
) -> dict[str, Any]:
    return {
        "classes": int(stimulus.shape[0]),
        "channels": int(x_test.shape[1]),
        "features": int(encodings.shape[1]),
        "window": int(x_test.shape[2]),
        "encodings": encodings.astype("float32").tolist(),
        "trials": x_test.astype("float32").tolist(),
        "benchmark_predictions": benchmark_predictions.tolist(),
        "benchmark_labels": benchmark_labels.astype("int64").tolist(),
        "regularization": regularization,
        "reference_states": reference_states or [],
    }
