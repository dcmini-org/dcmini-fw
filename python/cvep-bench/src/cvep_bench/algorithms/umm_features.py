from __future__ import annotations

from dataclasses import dataclass
import math
from typing import Literal

import numpy as np


LayoutName = Literal["channel_prime", "time_prime"]
EpochScheduleName = Literal["rounded_stride", "fractional_onset"]
ConfidenceModelName = Literal["inferred_normalized_margin", "margin_over_winner"]


@dataclass(frozen=True)
class UmmBlockStructureSpec:
    n_channels: int
    n_timepoints: int
    layout: LayoutName

    @property
    def feature_count(self) -> int:
        return self.n_channels * self.n_timepoints


@dataclass(frozen=True)
class UmmFeatures:
    features: np.ndarray
    codebook: np.ndarray
    epoch_samples: int
    epoch_stride_samples: int
    epochs_per_trial: int
    epoch_indices: np.ndarray
    epoch_start_samples: np.ndarray
    layout: LayoutName
    n_channels: int
    n_timepoints: int
    epoch_schedule: EpochScheduleName
    response_lag_samples: int
    trial_demean: bool
    epoch_demean: bool


@dataclass
class RunningUmmState:
    epochs_seen: int
    epoch_weight_sum: float
    avg_epoch: np.ndarray
    covariance: np.ndarray
    target_epochs_seen: int
    target_weight_sum: float
    avg_target: np.ndarray
    non_target_epochs_seen: int
    non_target_weight_sum: float
    avg_non_target: np.ndarray
    last_confidence: float


def seconds_to_samples(seconds: float, fs: int) -> int:
    return max(1, int(math.floor(seconds * fs + 0.5)))


def stimulus_epoch_window(
    window_start_samples: int,
    window_samples: int,
    n_stimulus_epochs: int,
    fs: int,
    presentation_rate: int,
    epoch_schedule: EpochScheduleName,
    response_lag_samples: int = 0,
) -> tuple[np.ndarray, np.ndarray]:
    if epoch_schedule == "rounded_stride":
        stride = max(1, seconds_to_samples(1.0 / presentation_rate, fs))
        starts = np.arange(n_stimulus_epochs, dtype=np.int64) * stride
    elif epoch_schedule == "fractional_onset":
        starts = np.rint(
            np.arange(n_stimulus_epochs, dtype=np.float64) * fs / presentation_rate
        ).astype(np.int64)
    else:
        raise ValueError(f"Unsupported epoch schedule {epoch_schedule}")
    starts = starts + int(response_lag_samples)
    valid = np.logical_and(
        starts >= window_start_samples, starts < window_start_samples + window_samples
    )
    return np.nonzero(valid)[0].astype(np.int64, copy=False), (
        starts[valid] - window_start_samples
    ).astype(np.int64, copy=False)


def flatten_epoch(epoch: np.ndarray, layout: LayoutName) -> np.ndarray:
    if layout == "channel_prime":
        return epoch.T.reshape(-1)
    if layout == "time_prime":
        return epoch.reshape(-1)
    raise ValueError(f"Unsupported layout {layout}")


def build_umm_features(
    trials: np.ndarray,
    stimulus: np.ndarray,
    fs: int,
    presentation_rate: int,
    epoch_seconds: float,
    layout: LayoutName,
    epoch_schedule: EpochScheduleName = "fractional_onset",
    response_lag_seconds: float = 0.0,
    trial_demean: bool = False,
    epoch_demean: bool = False,
    window_start_seconds: float = 0.0,
) -> UmmFeatures:
    n_trials, n_channels, window_samples = trials.shape
    epoch_samples = seconds_to_samples(epoch_seconds, fs)
    epoch_stride_samples = max(1, seconds_to_samples(1.0 / presentation_rate, fs))
    epoch_indices, epoch_start_samples = stimulus_epoch_window(
        int(math.floor(window_start_seconds * fs + 0.5)),
        window_samples,
        stimulus.shape[1],
        fs,
        presentation_rate,
        epoch_schedule,
        int(math.floor(response_lag_seconds * fs + 0.5)),
    )
    epochs_per_trial = int(epoch_start_samples.shape[0])
    feature_count = n_channels * epoch_samples
    features = np.zeros((n_trials, feature_count, epochs_per_trial), dtype=np.float32)
    prepared_trials = trials.astype(np.float32, copy=False)
    if trial_demean:
        prepared_trials = prepared_trials - prepared_trials.mean(axis=2, keepdims=True)
    for trial_idx in range(n_trials):
        trial = prepared_trials[trial_idx]
        for epoch_idx, start in enumerate(epoch_start_samples):
            stop = min(window_samples, start + epoch_samples)
            epoch = np.zeros((n_channels, epoch_samples), dtype=np.float32)
            epoch[:, : stop - start] = trial[:, start:stop]
            if epoch_demean:
                epoch -= epoch.mean(axis=1, keepdims=True)
            features[trial_idx, :, epoch_idx] = flatten_epoch(epoch, layout)
    codebook = (stimulus[:, epoch_indices] != 0).astype(np.uint8, copy=False)
    return UmmFeatures(
        features=features,
        codebook=codebook,
        epoch_samples=epoch_samples,
        epoch_stride_samples=epoch_stride_samples,
        epochs_per_trial=epochs_per_trial,
        epoch_indices=epoch_indices,
        epoch_start_samples=epoch_start_samples,
        layout=layout,
        n_channels=n_channels,
        n_timepoints=epoch_samples,
        epoch_schedule=epoch_schedule,
        response_lag_samples=int(math.floor(response_lag_seconds * fs + 0.5)),
        trial_demean=trial_demean,
        epoch_demean=epoch_demean,
    )


def make_structure(features: UmmFeatures) -> UmmBlockStructureSpec:
    return UmmBlockStructureSpec(
        n_channels=features.n_channels,
        n_timepoints=features.n_timepoints,
        layout=features.layout,
    )


def epoch_summary(epochs: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    mean = np.mean(epochs, axis=1)
    cov = (
        np.zeros((epochs.shape[0], epochs.shape[0]), dtype=np.float32)
        if epochs.shape[1] <= 1
        else np.cov(epochs, bias=False).astype(np.float32, copy=False)
    )
    return mean.astype(np.float32, copy=False), cov


def partition_means(
    epochs: np.ndarray, labels: np.ndarray
) -> tuple[int, int, np.ndarray, np.ndarray] | None:
    target_mask = labels != 0
    non_target_mask = ~target_mask
    target_count = int(np.sum(target_mask))
    non_target_count = int(np.sum(non_target_mask))
    if target_count == 0 or non_target_count == 0:
        return None
    return (
        target_count,
        non_target_count,
        np.mean(epochs[:, target_mask], axis=1).astype(np.float32, copy=False),
        np.mean(epochs[:, non_target_mask], axis=1).astype(np.float32, copy=False),
    )


def combine_mean(
    mean_a: np.ndarray, weight_a: float, mean_b: np.ndarray, weight_b: float
) -> np.ndarray:
    if weight_a <= 0.0:
        return mean_b.copy()
    if weight_b <= 0.0:
        return mean_a.copy()
    total = weight_a + weight_b
    return ((mean_a * weight_a) + (mean_b * weight_b)) / total


def combine_covariance_summaries(
    mean_a: np.ndarray,
    cov_a: np.ndarray,
    weight_a: float,
    mean_b: np.ndarray,
    cov_b: np.ndarray,
    weight_b: float,
) -> tuple[np.ndarray, np.ndarray, float]:
    if weight_a <= 0.0:
        return mean_b.copy(), cov_b.copy(), weight_b
    if weight_b <= 0.0:
        return mean_a.copy(), cov_a.copy(), weight_a
    total = weight_a + weight_b
    out_mean = combine_mean(mean_a, weight_a, mean_b, weight_b)
    delta = mean_b - mean_a
    out_cov = (
        cov_a * max(weight_a - 1.0, 0.0)
        + cov_b * max(weight_b - 1.0, 0.0)
        + np.outer(delta, delta) * ((weight_a * weight_b) / total)
    ) / max(total - 1.0, 1.0)
    return (
        out_mean.astype(np.float32, copy=False),
        out_cov.astype(np.float32, copy=False),
        total,
    )


def linear_taper(lag: int, n_timepoints: int) -> float:
    return (
        1.0
        if n_timepoints == 0
        else float(n_timepoints - abs(lag)) / float(n_timepoints)
    )


def feature_index(
    structure: UmmBlockStructureSpec, time_idx: int, channel_idx: int
) -> int:
    if structure.layout == "channel_prime":
        return time_idx * structure.n_channels + channel_idx
    if structure.layout == "time_prime":
        return channel_idx * structure.n_timepoints + time_idx
    raise ValueError(f"Unsupported layout {structure.layout}")


def tapered_block_toeplitz_covariance(
    covariance: np.ndarray, structure: UmmBlockStructureSpec | None
) -> np.ndarray:
    if structure is None:
        return covariance
    if structure.feature_count != covariance.shape[0]:
        raise ValueError(
            f"Structure feature count {structure.feature_count} does not match covariance shape {covariance.shape}"
        )
    out = np.zeros_like(covariance, dtype=np.float32)
    for lag in range(-(structure.n_timepoints - 1), structure.n_timepoints):
        abs_lag = abs(lag)
        count = structure.n_timepoints - abs_lag
        taper = linear_taper(lag, structure.n_timepoints)
        for row_channel in range(structure.n_channels):
            for col_channel in range(structure.n_channels):
                values = []
                for block_idx in range(count):
                    row_time = block_idx if lag >= 0 else block_idx + abs_lag
                    col_time = block_idx + abs_lag if lag >= 0 else block_idx
                    values.append(
                        covariance[
                            feature_index(structure, row_time, row_channel),
                            feature_index(structure, col_time, col_channel),
                        ]
                    )
                averaged = float(np.mean(values)) * taper
                for block_idx in range(count):
                    row_time = block_idx if lag >= 0 else block_idx + abs_lag
                    col_time = block_idx + abs_lag if lag >= 0 else block_idx
                    out[
                        feature_index(structure, row_time, row_channel),
                        feature_index(structure, col_time, col_channel),
                    ] = averaged
    return out


def mahalanobis_delta_score(
    delta: np.ndarray, covariance: np.ndarray, regularization: float
) -> float:
    reg = covariance.astype(np.float64, copy=True)
    reg.flat[:: reg.shape[0] + 1] += regularization
    try:
        chol = np.linalg.cholesky(reg)
    except np.linalg.LinAlgError:
        return 0.0
    whitened = np.linalg.solve(chol, delta.astype(np.float64, copy=False))
    return float(np.dot(whitened, whitened))


def decision_from_scores(scores: np.ndarray) -> tuple[int, float, float]:
    best_class = int(np.argmax(scores))
    best_score = float(scores[best_class])
    if scores.shape[0] < 2:
        return best_class, best_score, float("-inf")
    return best_class, best_score, float(np.partition(scores, -2)[-2])


def confidence_from_scores(
    best_score: float, runner_up: float, confidence_model: ConfidenceModelName
) -> float:
    if not math.isfinite(best_score):
        return 0.0
    if not math.isfinite(runner_up):
        return 1.0
    margin = max(best_score - runner_up, 0.0)
    if confidence_model == "inferred_normalized_margin":
        return float(
            np.clip(margin / (abs(best_score) + abs(runner_up) + 1.0e-6), 0.0, 1.0)
        )
    if confidence_model == "margin_over_winner":
        return float(np.clip(margin / (abs(best_score) + 1.0e-6), 0.0, 1.0))
    raise ValueError(f"Unsupported confidence model {confidence_model}")


def instantaneous_umm_predictions(
    trial_features: np.ndarray,
    codebook: np.ndarray,
    regularization: float,
    structure: UmmBlockStructureSpec | None,
) -> tuple[np.ndarray, np.ndarray]:
    predictions = np.zeros(trial_features.shape[0], dtype=np.int64)
    scores = np.zeros((trial_features.shape[0], codebook.shape[0]), dtype=np.float32)
    for trial_idx in range(trial_features.shape[0]):
        epochs = trial_features[trial_idx]
        _mean, covariance = epoch_summary(epochs)
        covariance = tapered_block_toeplitz_covariance(covariance, structure)
        for class_idx in range(codebook.shape[0]):
            partition = partition_means(epochs, codebook[class_idx])
            if partition is None:
                continue
            _target_count, _non_target_count, target_mean, non_target_mean = partition
            scores[trial_idx, class_idx] = mahalanobis_delta_score(
                target_mean - non_target_mean, covariance, regularization
            )
        predictions[trial_idx] = int(np.argmax(scores[trial_idx]))
    return predictions, scores


def empty_running_state(feature_count: int) -> RunningUmmState:
    zeros = np.zeros(feature_count, dtype=np.float32)
    cov = np.zeros((feature_count, feature_count), dtype=np.float32)
    return RunningUmmState(
        0, 0.0, zeros.copy(), cov, 0, 0.0, zeros.copy(), 0, 0.0, zeros.copy(), 0.0
    )


def cumulative_umm_predictions(
    trial_features: np.ndarray,
    codebook: np.ndarray,
    regularization: float,
    structure: UmmBlockStructureSpec | None,
    confidence_model: ConfidenceModelName,
) -> tuple[np.ndarray, np.ndarray, RunningUmmState]:
    state = empty_running_state(trial_features.shape[1])
    predictions = np.zeros(trial_features.shape[0], dtype=np.int64)
    scores = np.zeros((trial_features.shape[0], codebook.shape[0]), dtype=np.float32)
    for trial_idx in range(trial_features.shape[0]):
        epochs = trial_features[trial_idx]
        trial_mean, trial_cov = epoch_summary(epochs)
        covariance = tapered_block_toeplitz_covariance(
            state.covariance if state.epoch_weight_sum > 1.0 else trial_cov, structure
        )
        for class_idx in range(codebook.shape[0]):
            partition = partition_means(epochs, codebook[class_idx])
            if partition is None:
                continue
            target_count, non_target_count, trial_target, trial_non_target = partition
            combined_target = combine_mean(
                state.avg_target,
                state.target_weight_sum,
                trial_target,
                float(target_count),
            )
            combined_non_target = combine_mean(
                state.avg_non_target,
                state.non_target_weight_sum,
                trial_non_target,
                float(non_target_count),
            )
            scores[trial_idx, class_idx] = mahalanobis_delta_score(
                combined_target - combined_non_target, covariance, regularization
            )
        best_class, best_score, runner_up = decision_from_scores(scores[trial_idx])
        predictions[trial_idx] = best_class
        confidence = confidence_from_scores(best_score, runner_up, confidence_model)
        state.last_confidence = confidence
        state.epochs_seen += epochs.shape[1]
        partition = partition_means(epochs, codebook[best_class])
        if partition is None or confidence <= 0.0:
            continue
        target_count, non_target_count, trial_target, trial_non_target = partition
        state.target_epochs_seen += target_count
        state.non_target_epochs_seen += non_target_count
        state.avg_epoch, state.covariance, state.epoch_weight_sum = (
            combine_covariance_summaries(
                state.avg_epoch,
                state.covariance,
                state.epoch_weight_sum,
                trial_mean,
                trial_cov,
                confidence * epochs.shape[1],
            )
        )
        target_weight = confidence * target_count
        state.avg_target = combine_mean(
            state.avg_target, state.target_weight_sum, trial_target, target_weight
        )
        state.target_weight_sum += target_weight
        non_target_weight = confidence * non_target_count
        state.avg_non_target = combine_mean(
            state.avg_non_target,
            state.non_target_weight_sum,
            trial_non_target,
            non_target_weight,
        )
        state.non_target_weight_sum += non_target_weight
    return predictions, scores, state
