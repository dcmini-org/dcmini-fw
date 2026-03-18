from __future__ import annotations

from typing import Any

import numpy as np


def fit_etrca(
    x_train: np.ndarray, y_train: np.ndarray, fs: int, cycle_size: float | None
) -> Any:
    import pyntbci

    model = pyntbci.classifiers.eTRCA(
        lags=None, fs=fs, cycle_size=cycle_size, ensemble=True
    )
    model.fit(x_train, y_train)
    return model


def build_etrca_bank(
    model: Any, n_samples: int, classes: np.ndarray
) -> tuple[np.ndarray, np.ndarray]:
    n_classes = classes.shape[0]
    spatial = (
        np.repeat(model.w_[:, :, np.newaxis], n_classes, axis=2)
        if model.w_.ndim == 2
        else np.asarray(model.w_)
    )
    if spatial.shape[1] != 1:
        raise ValueError(f"Expected one spatial component, got {spatial.shape}")
    templates = np.asarray(model.get_T(n_samples))[:, 0, :].astype(np.float64)
    spatial_filters = np.zeros((n_classes, spatial.shape[0]), dtype=np.float64)
    for class_idx, _class_label in enumerate(classes):
        spatial_filters[class_idx] = spatial[:, 0, class_idx]
    return spatial_filters, templates


def fit_rcca(
    x_train: np.ndarray,
    y_train: np.ndarray,
    stimulus: np.ndarray,
    fs: int,
    event: str,
    encoding_length: float,
) -> Any:
    import pyntbci

    model = pyntbci.classifiers.rCCA(
        stimulus=stimulus,
        fs=fs,
        event=event,
        encoding_length=encoding_length,
        score_metric="correlation",
        ensemble=False,
        n_components=1,
    )
    model.fit(x_train, y_train)
    return model


def build_rcca_bank(
    model: Any, n_classes: int, n_channels: int, n_samples: int
) -> tuple[np.ndarray, np.ndarray]:
    if n_samples < model.Ts_.shape[2]:
        templates = model.Ts_
    else:
        templates = np.concatenate(
            (model.Ts_, np.tile(model.Tw_, (1, 1, n_samples // model.Ts_.shape[2]))),
            axis=2,
        )
    templates = templates[:, :, :n_samples].copy()
    templates -= templates.mean(axis=2, keepdims=True)
    templates = templates[:, 0, :]
    spatial_filters = (
        np.repeat(model.w_[:, 0][np.newaxis, :], n_classes, axis=0)
        if model.w_.ndim == 2
        else np.asarray(model.w_[:, 0, :]).T
    )
    if spatial_filters.shape != (n_classes, n_channels):
        raise ValueError(
            f"Unexpected rCCA filter shape {spatial_filters.shape}, expected {(n_classes, n_channels)}"
        )
    return spatial_filters.astype(np.float64), templates.astype(np.float64)


def quantize_trials_to_adc(
    x: np.ndarray, signed_bits: int, headroom: float
) -> tuple[np.ndarray, float]:
    adc_peak = float((1 << (signed_bits - 1)) - 1)
    data_peak = float(np.max(np.abs(x)))
    scale = 1.0 if data_peak == 0.0 else (adc_peak * headroom) / data_peak
    quantized = np.rint(x * scale).clip(-adc_peak - 1.0, adc_peak).astype(np.int32)
    return quantized, scale
