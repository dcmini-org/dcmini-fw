from __future__ import annotations


def loader_trial_seconds_for_algorithm(
    dataset: str,
    algorithm: str,
    requested_window_seconds: float,
) -> float | None:
    if dataset == "Thielen2021" and algorithm in {
        "rcca",
        "instantaneous_cca",
        "cumulative_cca",
    }:
        return requested_window_seconds
    return None
