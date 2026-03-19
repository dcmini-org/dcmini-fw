from __future__ import annotations

import numpy as np

from cvep_bench.algorithms.continuous_state_cca import (
    MODE_CROSS_TRIAL,
    STOP_FIXED_DWELL,
    STOP_MARGIN,
    UPDATE_CONFIDENCE,
    UPDATE_ORACLE,
    UPDATE_PSEUDO,
    UPDATE_SCOPE_ALL_OBSERVED,
    UPDATE_SCOPE_EMITTED_ONLY,
    decision_from_scores,
    decision_windows,
    run_trial,
    should_stop,
    update_models_for_observed_offsets,
)


class _DummyModel:
    def __init__(self, scores: np.ndarray | None = None) -> None:
        self.updated = []
        self.Ms = np.zeros((20, 1, 8), dtype=np.float64)
        self.Mw = np.zeros((20, 1, 8), dtype=np.float64)
        self._scores = np.zeros(20, dtype=np.float64) if scores is None else scores

    def update(self, predicted_class: int) -> None:
        self.updated.append(predicted_class)

    def fit(self, trial: np.ndarray) -> None:
        self.rho = self._scores


def test_decision_from_scores_computes_margin() -> None:
    winner, winner_score, runner_up, margin = decision_from_scores(
        np.asarray([0.1, 0.4, 0.2])
    )
    assert winner == 1
    assert abs(winner_score - 0.4) < 1e-9
    assert abs(runner_up - 0.2) < 1e-9
    assert abs(margin - 0.2) < 1e-9


def test_margin_stop_rule_triggers_when_threshold_crossed() -> None:
    stop_now, forced_stop = should_stop(
        STOP_MARGIN, margin=0.2, is_final_update=False, threshold=0.1
    )
    assert stop_now is True
    assert forced_stop is False


def test_fixed_dwell_only_stops_at_final_update() -> None:
    stop_now, forced_stop = should_stop(
        STOP_FIXED_DWELL, margin=0.0, is_final_update=False, threshold=None
    )
    assert stop_now is False
    assert forced_stop is False
    stop_now, forced_stop = should_stop(
        STOP_FIXED_DWELL, margin=0.0, is_final_update=True, threshold=None
    )
    assert stop_now is True
    assert forced_stop is True


def test_decision_windows_include_final_partial_step() -> None:
    windows = decision_windows(
        trial_samples=525,
        fs=125,
        window_seconds=1.0,
        update_seconds=0.25,
        max_dwell_seconds=4.2,
    )
    assert windows[0].end_sample == 125
    assert windows[-1].end_sample == 525
    assert windows[-1].start_sample == 400


def test_update_models_for_observed_offsets_updates_only_seen_offsets() -> None:
    models = {0: _DummyModel(), 1: _DummyModel(), 2: _DummyModel()}
    update_models_for_observed_offsets(
        models,
        [0, 2],
        emitted_update_index=2,
        predicted_class=7,
        update_scope=UPDATE_SCOPE_ALL_OBSERVED,
    )
    assert models[0].updated == [7]
    assert models[1].updated == []
    assert models[2].updated == [7]


def test_update_models_for_emitted_offset_only_touches_final_offset() -> None:
    models = {0: _DummyModel(), 1: _DummyModel(), 2: _DummyModel()}
    update_models_for_observed_offsets(
        models,
        [0, 1, 2],
        emitted_update_index=2,
        predicted_class=5,
        update_scope=UPDATE_SCOPE_EMITTED_ONLY,
    )
    assert models[0].updated == []
    assert models[1].updated == []
    assert models[2].updated == [5]


def test_run_trial_oracle_update_policy_only_updates_on_correct_prediction() -> None:
    trial = np.zeros((2, 8), dtype=np.float64)
    windows = [
        type(
            "W",
            (),
            {
                "start_sample": 0,
                "end_sample": 8,
                "window_samples": 8,
                "window_seconds": 1.0,
            },
        )()
    ]
    models = {0: _DummyModel(np.asarray([0.1, 0.9], dtype=np.float64))}
    decision = run_trial(
        trial,
        0,
        mode=MODE_CROSS_TRIAL,
        stop_rule=STOP_FIXED_DWELL,
        stop_threshold=None,
        windows=windows,
        stimulus=np.zeros((2, 8), dtype=np.float64),
        fs=8,
        event="refe",
        onset_event=False,
        encoding_length=0.3,
        persistent_models=models,
        update_policy=UPDATE_ORACLE,
        update_min_margin=0.05,
        update_scope=UPDATE_SCOPE_EMITTED_ONLY,
        update_min_consecutive_winners=1,
    )
    assert decision.correct is False
    assert models[0].updated == []


def test_run_trial_confidence_gated_policy_updates_on_margin() -> None:
    trial = np.zeros((2, 8), dtype=np.float64)
    windows = [
        type(
            "W",
            (),
            {
                "start_sample": 0,
                "end_sample": 8,
                "window_samples": 8,
                "window_seconds": 1.0,
            },
        )()
    ]
    models = {0: _DummyModel(np.asarray([0.1, 0.9], dtype=np.float64))}
    decision = run_trial(
        trial,
        1,
        mode=MODE_CROSS_TRIAL,
        stop_rule=STOP_FIXED_DWELL,
        stop_threshold=None,
        windows=windows,
        stimulus=np.zeros((2, 8), dtype=np.float64),
        fs=8,
        event="refe",
        onset_event=False,
        encoding_length=0.3,
        persistent_models=models,
        update_policy=UPDATE_CONFIDENCE,
        update_min_margin=0.5,
        update_scope=UPDATE_SCOPE_EMITTED_ONLY,
        update_min_consecutive_winners=1,
    )
    assert decision.correct is True
    assert models[0].updated == [1]


def test_confidence_gated_update_requires_consecutive_winners() -> None:
    trial = np.zeros((2, 8), dtype=np.float64)
    windows = [
        type(
            "W",
            (),
            {
                "start_sample": 0,
                "end_sample": 8,
                "window_samples": 8,
                "window_seconds": 1.0,
            },
        )(),
        type(
            "W",
            (),
            {
                "start_sample": 2,
                "end_sample": 10,
                "window_samples": 8,
                "window_seconds": 1.0,
            },
        )(),
    ]
    models = {
        0: _DummyModel(np.asarray([0.9, 0.1], dtype=np.float64)),
        1: _DummyModel(np.asarray([0.1, 0.9], dtype=np.float64)),
    }
    decision = run_trial(
        trial,
        1,
        mode=MODE_CROSS_TRIAL,
        stop_rule=STOP_FIXED_DWELL,
        stop_threshold=None,
        windows=windows,
        stimulus=np.zeros((2, 10), dtype=np.float64),
        fs=10,
        event="refe",
        onset_event=False,
        encoding_length=0.3,
        persistent_models=models,
        update_policy=UPDATE_CONFIDENCE,
        update_min_margin=0.5,
        update_scope=UPDATE_SCOPE_EMITTED_ONLY,
        update_min_consecutive_winners=2,
    )
    assert decision.correct is True
    assert models[0].updated == []
    assert models[1].updated == []
