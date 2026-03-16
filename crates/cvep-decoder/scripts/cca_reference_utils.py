#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10"
# dependencies = [
#   "numpy>=2.2.6",
#   "pyntbci>=1.8.3",
# ]
# ///
"""Reference helpers for zero-training CCA benchmarks."""

from __future__ import annotations

from dataclasses import dataclass

import numpy as np


@dataclass(frozen=True)
class CcaReferenceResult:
    predictions: np.ndarray
    scores: np.ndarray


def quantize_trials_to_i32(
    x: np.ndarray,
    signed_bits: int,
    headroom: float,
) -> tuple[np.ndarray, float]:
    adc_peak = float((1 << (signed_bits - 1)) - 1)
    data_peak = float(np.max(np.abs(x)))
    scale = 1.0 if data_peak == 0.0 else (adc_peak * headroom) / data_peak
    quantized = np.rint(x * scale).clip(-adc_peak - 1.0, adc_peak).astype(np.int32)
    return quantized, scale


def build_cca_encodings(
    stimulus: np.ndarray,
    fs: int,
    window_samples: int,
    *,
    event: str,
    onset_event: bool,
    encoding_length: float,
    start_sample: int = 0,
) -> np.ndarray:
    import pyntbci

    model = pyntbci.classifiers.urCCA(
        stimulus=stimulus,
        fs=fs,
        event=event,
        onset_event=onset_event,
        encoding_length=encoding_length,
    )
    total_samples = start_sample + window_samples
    if total_samples <= model.Ms.shape[2]:
        full = model.Ms[:, :, :total_samples].copy()
    else:
        repeats = int(np.ceil((total_samples - model.Ms.shape[2]) / model.Mw.shape[2]))
        full = np.concatenate(
            (model.Ms, np.tile(model.Mw, (1, 1, repeats))),
            axis=2,
        )[:, :, :total_samples]
    return np.asarray(full[:, :, start_sample : start_sample + window_samples], dtype=np.float64)


def instantaneous_cca_predictions_pyntbci(
    trials: np.ndarray,
    stimulus: np.ndarray,
    fs: int,
    *,
    event: str,
    onset_event: bool,
    encoding_length: float,
) -> CcaReferenceResult:
    import pyntbci

    predictions = np.zeros(trials.shape[0], dtype=np.int64)
    scores = np.zeros((trials.shape[0], stimulus.shape[0]), dtype=np.float64)
    for trial_idx in range(trials.shape[0]):
        model = pyntbci.classifiers.urCCA(
            stimulus=stimulus,
            fs=fs,
            event=event,
            onset_event=onset_event,
            encoding_length=encoding_length,
        )
        model.fit(trials[trial_idx])
        predictions[trial_idx] = int(model.predict())
        scores[trial_idx] = np.asarray(model.rho, dtype=np.float64)
    return CcaReferenceResult(predictions=predictions, scores=scores)


def cumulative_cca_predictions_pyntbci(
    trials: np.ndarray,
    stimulus: np.ndarray,
    fs: int,
    *,
    event: str,
    onset_event: bool,
    encoding_length: float,
) -> CcaReferenceResult:
    import pyntbci

    model = pyntbci.classifiers.urCCA(
        stimulus=stimulus,
        fs=fs,
        event=event,
        onset_event=onset_event,
        encoding_length=encoding_length,
    )
    predictions = np.zeros(trials.shape[0], dtype=np.int64)
    scores = np.zeros((trials.shape[0], stimulus.shape[0]), dtype=np.float64)
    for trial_idx in range(trials.shape[0]):
        model.fit(trials[trial_idx])
        predictions[trial_idx] = int(model.predict())
        scores[trial_idx] = np.asarray(model.rho, dtype=np.float64)
        model.update(predictions[trial_idx])
    return CcaReferenceResult(predictions=predictions, scores=scores)


def _top_canonical_correlation(
    trial: np.ndarray,
    encoding: np.ndarray,
    regularization: float,
) -> float:
    x = np.asarray(trial, dtype=np.float64)
    y = np.asarray(encoding, dtype=np.float64)
    x = x - x.mean(axis=1, keepdims=True)
    y = y - y.mean(axis=1, keepdims=True)
    samples = x.shape[1]
    scale = 1.0 / max(samples, 1)
    sxx = (x @ x.T) * scale
    syy = (y @ y.T) * scale
    sxy = (x @ y.T) * scale
    sxx += np.eye(sxx.shape[0], dtype=np.float64) * regularization
    syy += np.eye(syy.shape[0], dtype=np.float64) * regularization
    inv_sxx = np.linalg.pinv(sxx, hermitian=True)
    inv_syy = np.linalg.pinv(syy, hermitian=True)
    matrix = inv_sxx @ sxy @ inv_syy @ sxy.T
    eigvals = np.linalg.eigvals(matrix)
    best = float(np.max(eigvals.real))
    return float(np.sqrt(max(best, 0.0)))


def class_scores_from_encodings(
    trial: np.ndarray,
    encodings: np.ndarray,
    regularization: float,
) -> np.ndarray:
    scores = np.zeros(encodings.shape[0], dtype=np.float64)
    for class_idx in range(encodings.shape[0]):
        scores[class_idx] = _top_canonical_correlation(
            trial,
            encodings[class_idx],
            regularization,
        )
    return scores


def instantaneous_cca_predictions_reference(
    trials: np.ndarray,
    encodings: np.ndarray,
    regularization: float,
) -> CcaReferenceResult:
    scores = np.stack(
        [
            class_scores_from_encodings(trial, encodings, regularization)
            for trial in trials
        ],
        axis=0,
    )
    predictions = scores.argmax(axis=1).astype(np.int64)
    return CcaReferenceResult(predictions=predictions, scores=scores)
