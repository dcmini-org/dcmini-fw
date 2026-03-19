from __future__ import annotations

from dataclasses import dataclass
from typing import Any

import numpy as np

from cvep_bench.datasets.windowing import WindowSlice, iter_sliding_windows


MODE_STATELESS = "stateless_instantaneous"
MODE_WITHIN_TRIAL = "within_trial_accumulated"
MODE_CROSS_TRIAL = "cross_trial_cumulative"
MODE_HYBRID = "hybrid_continuous_cumulative"

STOP_FIXED_DWELL = "fixed_dwell"
STOP_MARGIN = "margin_threshold"

UPDATE_PSEUDO = "pseudo_label"
UPDATE_CONFIDENCE = "confidence_gated"
UPDATE_ORACLE = "oracle"

UPDATE_SCOPE_ALL_OBSERVED = "all_observed_offsets"
UPDATE_SCOPE_EMITTED_ONLY = "emitted_offset_only"


@dataclass(frozen=True)
class ContinuousPrototypeConfig:
    window_seconds: float
    update_seconds: float
    max_dwell_seconds: float
    event: str
    onset_event: bool
    encoding_length: float


@dataclass(frozen=True)
class ContinuousDecision:
    mode: str
    stop_rule: str
    stop_threshold: float | None
    predicted_class: int
    true_class: int
    correct: bool
    decision_seconds: float
    update_index: int
    stopped_early: bool
    forced_stop: bool
    winner_score: float
    runner_up_score: float
    score_margin: float


def make_shifted_urcca_model(
    stimulus: np.ndarray,
    fs: int,
    *,
    event: str,
    onset_event: bool,
    encoding_length: float,
    start_sample: int,
) -> Any:
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
    return model


def score_trial_with_model(model: Any, trial: np.ndarray) -> np.ndarray:
    try:
        model.fit(trial)
    except Exception:  # noqa: BLE001
        return np.zeros(model.Ms.shape[0], dtype=np.float64)
    return np.asarray(model.rho, dtype=np.float64)


def decision_from_scores(scores: np.ndarray) -> tuple[int, float, float, float]:
    winner = int(np.argmax(scores))
    winner_score = float(scores[winner])
    if scores.size < 2:
        return winner, winner_score, float("-inf"), float("inf")
    top2 = np.partition(scores, -2)[-2:]
    runner_up = float(top2[-2])
    margin = float(top2[-1] - top2[-2])
    return winner, winner_score, runner_up, margin


def should_stop(
    stop_rule: str,
    *,
    margin: float,
    is_final_update: bool,
    threshold: float | None,
) -> tuple[bool, bool]:
    if stop_rule == STOP_FIXED_DWELL:
        return is_final_update, is_final_update
    if stop_rule == STOP_MARGIN:
        if threshold is None:
            raise ValueError("margin threshold stop rule requires threshold")
        if margin >= threshold:
            return True, False
        return is_final_update, is_final_update
    raise ValueError(f"Unsupported stop rule {stop_rule}")


def decision_windows(
    *,
    trial_samples: int,
    fs: int,
    window_seconds: float,
    update_seconds: float,
    max_dwell_seconds: float,
) -> list[WindowSlice]:
    max_samples = min(trial_samples, int(round(max_dwell_seconds * fs)))
    return iter_sliding_windows(max_samples, fs, [window_seconds], update_seconds)


def initialize_offset_models(
    windows: list[WindowSlice],
    stimulus: np.ndarray,
    fs: int,
    *,
    event: str,
    onset_event: bool,
    encoding_length: float,
) -> dict[int, Any]:
    return {
        idx: make_shifted_urcca_model(
            stimulus,
            fs,
            event=event,
            onset_event=onset_event,
            encoding_length=encoding_length,
            start_sample=window.start_sample,
        )
        for idx, window in enumerate(windows)
    }


def update_models_for_observed_offsets(
    model_bank: dict[int, Any],
    observed_update_indices: list[int],
    emitted_update_index: int,
    predicted_class: int,
    update_scope: str,
) -> None:
    if update_scope == UPDATE_SCOPE_ALL_OBSERVED:
        update_indices = observed_update_indices
    elif update_scope == UPDATE_SCOPE_EMITTED_ONLY:
        update_indices = [emitted_update_index]
    else:
        raise ValueError(f"Unsupported update scope {update_scope}")
    for update_index in update_indices:
        model_bank[update_index].update(predicted_class)


def run_trial(
    trial: np.ndarray,
    true_class: int,
    *,
    mode: str,
    stop_rule: str,
    stop_threshold: float | None,
    windows: list[WindowSlice],
    stimulus: np.ndarray,
    fs: int,
    event: str,
    onset_event: bool,
    encoding_length: float,
    persistent_models: dict[int, Any] | None,
    update_policy: str = UPDATE_PSEUDO,
    update_min_margin: float = 0.05,
    update_scope: str = UPDATE_SCOPE_EMITTED_ONLY,
    update_min_consecutive_winners: int = 1,
) -> ContinuousDecision:
    if mode in {MODE_CROSS_TRIAL, MODE_HYBRID} and persistent_models is None:
        raise ValueError(f"Mode {mode} requires persistent models")

    accumulated_scores: np.ndarray | None = None
    observed_update_indices: list[int] = []
    winner_history: list[int] = []
    final_decision: ContinuousDecision | None = None

    for update_index, window in enumerate(windows):
        window_signal = trial[:, window.start_sample : window.end_sample]
        if mode in {MODE_STATELESS, MODE_WITHIN_TRIAL}:
            model = make_shifted_urcca_model(
                stimulus,
                fs,
                event=event,
                onset_event=onset_event,
                encoding_length=encoding_length,
                start_sample=window.start_sample,
            )
        else:
            assert persistent_models is not None
            model = persistent_models[update_index]

        current_scores = score_trial_with_model(model, window_signal)
        if mode in {MODE_WITHIN_TRIAL, MODE_HYBRID}:
            if accumulated_scores is None:
                accumulated_scores = np.zeros_like(current_scores)
            accumulated_scores += current_scores
            active_scores = accumulated_scores
        else:
            active_scores = current_scores

        winner, winner_score, runner_up_score, margin = decision_from_scores(
            active_scores
        )
        is_final_update = update_index == len(windows) - 1
        stop_now, forced_stop = should_stop(
            stop_rule,
            margin=margin,
            is_final_update=is_final_update,
            threshold=stop_threshold,
        )
        observed_update_indices.append(update_index)
        winner_history.append(winner)

        if stop_now:
            final_decision = ContinuousDecision(
                mode=mode,
                stop_rule=stop_rule,
                stop_threshold=stop_threshold,
                predicted_class=winner,
                true_class=int(true_class),
                correct=winner == int(true_class),
                decision_seconds=window.end_sample / fs,
                update_index=update_index,
                stopped_early=not forced_stop,
                forced_stop=forced_stop,
                winner_score=winner_score,
                runner_up_score=runner_up_score,
                score_margin=margin,
            )
            break

    if final_decision is None:
        raise RuntimeError("continuous-state CCA failed to emit a decision")

    if mode in {MODE_CROSS_TRIAL, MODE_HYBRID}:
        assert persistent_models is not None
        should_update = False
        if update_policy == UPDATE_PSEUDO:
            should_update = True
        elif update_policy == UPDATE_CONFIDENCE:
            should_update = (
                final_decision.score_margin >= update_min_margin
                and final_decision.winner_score > 0.0
            )
        elif update_policy == UPDATE_ORACLE:
            should_update = final_decision.correct
        else:
            raise ValueError(f"Unsupported update policy {update_policy}")
        if should_update:
            if update_min_consecutive_winners > 1:
                recent = winner_history[-update_min_consecutive_winners:]
                should_update = len(recent) == update_min_consecutive_winners and all(
                    label == final_decision.predicted_class for label in recent
                )
        if should_update:
            update_models_for_observed_offsets(
                persistent_models,
                observed_update_indices,
                final_decision.update_index,
                final_decision.predicted_class,
                update_scope,
            )

    return final_decision
