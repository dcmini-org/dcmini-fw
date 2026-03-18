from __future__ import annotations

import math
import os
import tempfile
from pathlib import Path

import mne
import numpy as np
from scipy.signal import resample_poly

from cvep_bench.datasets.models import PreprocessingOptions
from cvep_bench.datasets.profiles import default_preprocessing_options


os.environ.setdefault("MNE_DONTWRITE_HOME", "true")
os.environ.setdefault(
    "MPLCONFIGDIR", str(Path(tempfile.gettempdir()) / "matplotlib-cache")
)
mne.set_log_level("ERROR")


def preprocess_raw(
    raw: mne.io.BaseRaw, preprocessing: PreprocessingOptions | None = None
) -> None:
    settings = (
        default_preprocessing_options() if preprocessing is None else preprocessing
    )
    if settings.notch_hz > 0.0:
        upper = raw.info["sfreq"] / 2.0
        freqs = np.arange(settings.notch_hz, upper, settings.notch_hz, dtype=np.float64)
        freqs = freqs[freqs > 0]
    else:
        freqs = np.asarray([], dtype=np.float64)
    if freqs.size:
        raw.notch_filter(freqs=freqs, picks="eeg", verbose=False)
    raw.filter(
        l_freq=settings.band_low, h_freq=settings.band_high, picks="eeg", verbose=False
    )


def epoch_and_resample(
    raw: mne.io.BaseRaw,
    events: np.ndarray,
    target_fs: int,
    tmin: float,
    tmax: float,
    event_id: dict[str, int] | None = None,
    preprocessing: PreprocessingOptions | None = None,
) -> np.ndarray:
    settings = (
        default_preprocessing_options() if preprocessing is None else preprocessing
    )
    preprocess_raw(raw, preprocessing=settings)
    epochs = mne.Epochs(
        raw,
        events=events,
        event_id=event_id,
        tmin=tmin - settings.pretrial_buffer_seconds,
        tmax=tmax,
        baseline=None,
        picks="eeg",
        preload=True,
        verbose=False,
    )
    epochs.resample(sfreq=target_fs, verbose=False)
    return np.asarray(epochs.get_data(tmin=tmin, tmax=tmax), dtype=np.float64)


def extract_trials_from_raw(
    raw: mne.io.BaseRaw,
    onsets: np.ndarray,
    trial_seconds: float,
    target_fs: int,
    preprocessing: PreprocessingOptions | None = None,
) -> np.ndarray:
    settings = (
        default_preprocessing_options() if preprocessing is None else preprocessing
    )
    raw_samples = int(
        round((trial_seconds + settings.pretrial_buffer_seconds) * raw.info["sfreq"])
    )
    valid_onsets = [
        int(onset) for onset in onsets if onset + raw_samples <= raw.n_times
    ]
    if not valid_onsets:
        raise ValueError("No valid onsets remained after trial extraction bounds check")
    events = np.column_stack(
        (
            np.asarray(valid_onsets, dtype=np.int64),
            np.zeros(len(valid_onsets), dtype=np.int64),
            np.ones(len(valid_onsets), dtype=np.int64),
        )
    )
    return epoch_and_resample(
        raw,
        events=events,
        target_fs=target_fs,
        tmin=0.0,
        tmax=trial_seconds,
        event_id={"trial": 1},
        preprocessing=settings,
    )


def resample_trials(x: np.ndarray, fs_raw: float, target_fs: int) -> np.ndarray:
    fs_raw_int = int(round(fs_raw))
    gcd = math.gcd(fs_raw_int, target_fs)
    up = target_fs // gcd
    down = fs_raw_int // gcd
    resampled = resample_poly(x, up=up, down=down, axis=2)
    target_samples = int(round(x.shape[2] * target_fs / fs_raw))
    if resampled.shape[2] > target_samples:
        resampled = resampled[:, :, :target_samples]
    elif resampled.shape[2] < target_samples:
        resampled = np.pad(
            resampled, ((0, 0), (0, 0), (0, target_samples - resampled.shape[2]))
        )
    return resampled.astype(np.float64)
