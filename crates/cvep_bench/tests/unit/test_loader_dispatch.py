from __future__ import annotations

from cvep_bench.datasets.windows import loader_trial_seconds_for_algorithm


def test_thielen2021_cca_uses_direct_window_loading() -> None:
    assert (
        loader_trial_seconds_for_algorithm("Thielen2021", "instantaneous_cca", 4.2)
        == 4.2
    )
    assert (
        loader_trial_seconds_for_algorithm("Thielen2021", "cumulative_cca", 4.2) == 4.2
    )
    assert loader_trial_seconds_for_algorithm("Thielen2021", "rcca", 4.2) == 4.2


def test_non_cca_or_non_thielen2021_stays_on_full_trial_path() -> None:
    assert loader_trial_seconds_for_algorithm("Thielen2021", "etrca", 4.2) is None
    assert (
        loader_trial_seconds_for_algorithm("Thielen2015", "instantaneous_cca", 4.2)
        is None
    )
