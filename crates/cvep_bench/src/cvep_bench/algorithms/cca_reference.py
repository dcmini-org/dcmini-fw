from __future__ import annotations

from dataclasses import dataclass

import numpy as np


@dataclass(frozen=True)
class CcaReferenceResult:
    predictions: np.ndarray
    scores: np.ndarray


def quantize_trials_to_i32(
    x: np.ndarray, signed_bits: int, headroom: float
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
        full = np.concatenate((model.Ms, np.tile(model.Mw, (1, 1, repeats))), axis=2)[
            :, :, :total_samples
        ]
    return np.asarray(
        full[:, :, start_sample : start_sample + window_samples], dtype=np.float64
    )


def instantaneous_cca_predictions_pyntbci(
    trials: np.ndarray,
    stimulus: np.ndarray,
    fs: int,
    *,
    event: str,
    onset_event: bool,
    encoding_length: float,
    start_sample: int = 0,
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
        if start_sample:
            model.Ms = model.Ms[:, :, start_sample:].copy()
            model.Mw = model.Mw[:, :, start_sample:].copy()
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
    start_sample: int = 0,
) -> CcaReferenceResult:
    import pyntbci

    model = pyntbci.classifiers.urCCA(
        stimulus=stimulus,
        fs=fs,
        event=event,
        onset_event=onset_event,
        encoding_length=encoding_length,
    )
    if start_sample:
        model.Ms = model.Ms[:, :, start_sample:].copy()
        model.Mw = model.Mw[:, :, start_sample:].copy()
    predictions = np.zeros(trials.shape[0], dtype=np.int64)
    scores = np.zeros((trials.shape[0], stimulus.shape[0]), dtype=np.float64)
    for trial_idx in range(trials.shape[0]):
        model.fit(trials[trial_idx])
        predictions[trial_idx] = int(model.predict())
        scores[trial_idx] = np.asarray(model.rho, dtype=np.float64)
        model.update(predictions[trial_idx])
    return CcaReferenceResult(predictions=predictions, scores=scores)


def cumulative_cca_predictions_pyntbci_confidence_gated(
    trials: np.ndarray,
    stimulus: np.ndarray,
    fs: int,
    *,
    event: str,
    onset_event: bool,
    encoding_length: float,
    min_margin: float,
    start_sample: int = 0,
) -> CcaReferenceResult:
    import pyntbci

    model = pyntbci.classifiers.urCCA(
        stimulus=stimulus,
        fs=fs,
        event=event,
        onset_event=onset_event,
        encoding_length=encoding_length,
    )
    if start_sample:
        model.Ms = model.Ms[:, :, start_sample:].copy()
        model.Mw = model.Mw[:, :, start_sample:].copy()
    predictions = np.zeros(trials.shape[0], dtype=np.int64)
    scores = np.zeros((trials.shape[0], stimulus.shape[0]), dtype=np.float64)
    for trial_idx in range(trials.shape[0]):
        model.fit(trials[trial_idx])
        trial_scores = np.asarray(model.rho, dtype=np.float64)
        prediction = int(np.argmax(trial_scores))
        predictions[trial_idx] = prediction
        scores[trial_idx] = trial_scores
        if trial_scores.size == 1:
            margin = float("inf")
        else:
            top2 = np.partition(trial_scores, -2)[-2:]
            margin = float(top2[-1] - top2[-2])
        if margin >= min_margin:
            model.update(prediction)
    return CcaReferenceResult(predictions=predictions, scores=scores)


def _top_canonical_correlation(
    trial: np.ndarray, encoding: np.ndarray, regularization: float
) -> float:
    x = np.asarray(trial, dtype=np.float64)
    y = np.asarray(encoding, dtype=np.float64)
    x = x - x.mean(axis=1, keepdims=True)
    y = y - y.mean(axis=1, keepdims=True)
    scale = 1.0 / max(x.shape[1], 1)
    sxx = (x @ x.T) * scale + np.eye(x.shape[0]) * regularization
    syy = (y @ y.T) * scale + np.eye(y.shape[0]) * regularization
    sxy = (x @ y.T) * scale
    matrix = (
        np.linalg.pinv(sxx, hermitian=True)
        @ sxy
        @ np.linalg.pinv(syy, hermitian=True)
        @ sxy.T
    )
    eigvals = np.linalg.eigvals(matrix)
    return float(np.sqrt(max(float(np.max(eigvals.real)), 0.0)))


def class_scores_from_encodings(
    trial: np.ndarray, encodings: np.ndarray, regularization: float
) -> np.ndarray:
    return np.asarray(
        [
            _top_canonical_correlation(trial, encodings[class_idx], regularization)
            for class_idx in range(encodings.shape[0])
        ],
        dtype=np.float64,
    )


def instantaneous_cca_predictions_reference(
    trials: np.ndarray, encodings: np.ndarray, regularization: float
) -> CcaReferenceResult:
    scores = np.stack(
        [
            class_scores_from_encodings(trial, encodings, regularization)
            for trial in trials
        ],
        axis=0,
    )
    return CcaReferenceResult(
        predictions=scores.argmax(axis=1).astype(np.int64), scores=scores
    )


def cumulative_cca_predictions_reference(
    trials: np.ndarray,
    encodings: np.ndarray,
    regularization: float,
    *,
    min_margin: float | None = None,
) -> CcaReferenceResult:
    predictions = np.zeros(trials.shape[0], dtype=np.int64)
    scores = np.zeros((trials.shape[0], encodings.shape[0]), dtype=np.float64)
    running_trials: list[np.ndarray] = []
    running_labels: list[int] = []
    current_encodings = np.asarray(encodings, dtype=np.float64)
    for trial_idx, trial in enumerate(trials):
        trial_scores = class_scores_from_encodings(
            trial, current_encodings, regularization
        )
        prediction = int(np.argmax(trial_scores))
        predictions[trial_idx] = prediction
        scores[trial_idx] = trial_scores
        if min_margin is not None and trial_scores.size > 1:
            top2 = np.partition(trial_scores, -2)[-2:]
            if float(top2[-1] - top2[-2]) < min_margin:
                continue
        running_trials.append(trial)
        running_labels.append(prediction)
        updated = []
        for class_idx in range(current_encodings.shape[0]):
            class_members = [
                running_trials[idx]
                for idx, label in enumerate(running_labels)
                if label == class_idx
            ]
            if class_members:
                template = np.mean(np.stack(class_members, axis=0), axis=0)
            else:
                template = current_encodings[class_idx]
            updated.append(template)
        current_encodings = np.asarray(updated, dtype=np.float64)
    return CcaReferenceResult(predictions=predictions, scores=scores)
