from __future__ import annotations

from pathlib import Path

from cvep_bench.algorithms.cca_reference import (
    cumulative_cca_predictions_pyntbci,
    instantaneous_cca_predictions_pyntbci,
)
from cvep_bench.datasets.loaders import load_subject
from cvep_bench.datasets.profiles import default_preprocessing_options
from cvep_bench.datasets.windows import fold_slices, stimulus_to_sample_rate

DATA_ROOT = Path(__file__).resolve().parents[4] / "crates" / "cvep-decoder" / "data"


def _run_source(source: str) -> dict[str, float]:
    data = load_subject(
        "Thielen2021",
        1,
        DATA_ROOT,
        240,
        trial_seconds=4.2,
        preprocessing=default_preprocessing_options(),
        thielen2021_source=source,
    )
    idx = fold_slices(data.x.shape[0], 5)[0]
    x = data.x[idx]
    y = data.y[idx]
    stimulus = stimulus_to_sample_rate(data.stimulus, data.presentation_rate, data.fs)
    instantaneous = instantaneous_cca_predictions_pyntbci(
        x, stimulus, data.fs, event="refe", onset_event=False, encoding_length=0.3
    )
    cumulative = cumulative_cca_predictions_pyntbci(
        x, stimulus, data.fs, event="refe", onset_event=False, encoding_length=0.3
    )
    return {
        "instantaneous_cca": float((instantaneous.predictions == y).mean()),
        "cumulative_cca": float((cumulative.predictions == y).mean()),
    }


def test_packaged_and_raw_4p2s_240hz_stay_close() -> None:
    packaged = _run_source("packaged")
    raw = _run_source("raw")
    assert packaged["instantaneous_cca"] >= 0.85
    assert packaged["cumulative_cca"] >= 0.95
    assert abs(packaged["instantaneous_cca"] - raw["instantaneous_cca"]) <= 0.10
    assert abs(packaged["cumulative_cca"] - raw["cumulative_cca"]) <= 0.10
