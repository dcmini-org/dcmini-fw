from __future__ import annotations

from dataclasses import dataclass

import numpy as np


@dataclass
class SubjectData:
    dataset: str
    subject: int
    x: np.ndarray
    y: np.ndarray
    fs: int
    stimulus: np.ndarray
    cycle_size: float | None
    trial_seconds: float
    presentation_rate: int


@dataclass(frozen=True)
class PreprocessingOptions:
    band_low: float
    band_high: float
    notch_hz: float
    pretrial_buffer_seconds: float
    drop_first_seconds: float


@dataclass(frozen=True)
class BenchmarkProfile:
    name: str
    description: str
    target_fs: int
    band_low: float
    band_high: float
    notch_hz: float
    drop_first_seconds: float
    event: str
    onset_event: bool
    encoding_length: float
    default_window_seconds_grid: tuple[float, ...] | None = None
