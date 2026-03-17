#!/usr/bin/env python3
# /// script
# dependencies = [
#     "h5py>=3.16.0",
#     "mne>=1.11.0",
#     "numpy>=2.2.6",
#     "pyntbci>=1.8.3",
#     "rich>=14.3.3",
#     "scipy>=1.15.3",
# ]
# ///
"""Benchmark PyntBCI against the Rust c-VEP decoder across local datasets."""

from __future__ import annotations

import argparse
import html
import json
import math
import os
import subprocess
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import h5py
import numpy as np
from rich.console import Console
from rich.table import Table
from scipy.io import loadmat
from scipy.signal import resample_poly

os.environ.setdefault("MNE_DONTWRITE_HOME", "true")
os.environ.setdefault(
    "MPLCONFIGDIR",
    str(Path(tempfile.gettempdir()) / "matplotlib-cache"),
)

import mne

mne.set_log_level("ERROR")

WORKSPACE_ROOT = Path(__file__).resolve().parents[3]
CRATE_ROOT = Path(__file__).resolve().parents[1]


THIELEN2021_SESSIONS = (
    "20181128",
    "20181206",
    "20181217",
    "20181217",
    "20181217",
    "20181218",
    "20181218",
    "20181219",
    "20181219",
    "20181220",
    "20181220",
    "20181220",
    "20190107",
    "20190107",
    "20190110",
    "20190110",
    "20190110",
    "20190117",
    "20190117",
    "20190118",
    "20190118",
    "20190118",
    "20190220",
    "20190222",
    "20190225",
    "20190301",
    "20190307",
    "20190308",
    "20190311",
    "20190311",
)

THIELEN2015_RUNS = 3
THIELEN2021_BLOCKS = 5
THIELEN2015_PRESENTATION_RATE = 120
THIELEN2021_PRESENTATION_RATE = 60
CASTILLOS_PRESENTATION_RATE = 60
DC_MINI_BASE_FS = 250
PRETRIAL_BUFFER_SECONDS = 0.5
BANDPASS_HZ = (1.0, 65.0)
NOTCH_HZ = 50.0

CASTILLOS_PARADIGMS = {
    "CastillosBurstVEP40": "burst40",
    "CastillosBurstVEP100": "burst100",
    "CastillosCVEP40": "mseq40",
    "CastillosCVEP100": "mseq100",
}

DEFAULT_DATASETS = [
    "Thielen2015",
    "Thielen2021",
    "CastillosBurstVEP40",
    "CastillosBurstVEP100",
    "CastillosCVEP40",
    "CastillosCVEP100",
]


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


THIELEN2021_KEY_WINDOWS = (1.05, 2.1, 4.2, 5.25, 10.5, 31.5)

BENCHMARK_PROFILES: dict[str, BenchmarkProfile] = {
    "legacy": BenchmarkProfile(
        name="legacy",
        description="Current default benchmark settings.",
        target_fs=250,
        band_low=1.0,
        band_high=65.0,
        notch_hz=50.0,
        drop_first_seconds=0.0,
        event="refe",
        onset_event=False,
        encoding_length=0.3,
        default_window_seconds_grid=None,
    ),
    "matched_embedded_125": BenchmarkProfile(
        name="matched_embedded_125",
        description="Embedded-relevant 125 Hz comparison profile.",
        target_fs=125,
        band_low=6.0,
        band_high=50.0,
        notch_hz=50.0,
        drop_first_seconds=0.0,
        event="refe",
        onset_event=False,
        encoding_length=0.3,
        default_window_seconds_grid=THIELEN2021_KEY_WINDOWS,
    ),
    "matched_diagnostic_125": BenchmarkProfile(
        name="matched_diagnostic_125",
        description="Embedded 125 Hz profile with first 500 ms removed.",
        target_fs=125,
        band_low=6.0,
        band_high=50.0,
        notch_hz=50.0,
        drop_first_seconds=0.5,
        event="refe",
        onset_event=False,
        encoding_length=0.3,
        default_window_seconds_grid=THIELEN2021_KEY_WINDOWS,
    ),
    "matched_onset_aware_125": BenchmarkProfile(
        name="matched_onset_aware_125",
        description="Embedded 125 Hz profile with onset-aware CCA.",
        target_fs=125,
        band_low=6.0,
        band_high=50.0,
        notch_hz=50.0,
        drop_first_seconds=0.0,
        event="refe",
        onset_event=True,
        encoding_length=0.3,
        default_window_seconds_grid=THIELEN2021_KEY_WINDOWS,
    ),
    "literature_oriented_125": BenchmarkProfile(
        name="literature_oriented_125",
        description="Literature-inspired zero-training CCA profile at 125 Hz.",
        target_fs=125,
        band_low=6.0,
        band_high=50.0,
        notch_hz=50.0,
        drop_first_seconds=0.5,
        event="refe",
        onset_event=True,
        encoding_length=0.3,
        default_window_seconds_grid=THIELEN2021_KEY_WINDOWS,
    ),
}


def benchmark_profile_names() -> list[str]:
    return list(BENCHMARK_PROFILES)


def resolve_benchmark_profile(name: str) -> BenchmarkProfile:
    try:
        return BENCHMARK_PROFILES[name]
    except KeyError as exc:
        raise ValueError(f"Unknown benchmark profile {name}") from exc


def default_preprocessing_options() -> PreprocessingOptions:
    profile = resolve_benchmark_profile("legacy")
    return PreprocessingOptions(
        band_low=profile.band_low,
        band_high=profile.band_high,
        notch_hz=profile.notch_hz,
        pretrial_buffer_seconds=PRETRIAL_BUFFER_SECONDS,
        drop_first_seconds=profile.drop_first_seconds,
    )


def resolve_preprocessing_options(
    profile: BenchmarkProfile,
    *,
    band_low: float | None,
    band_high: float | None,
    notch_hz: float | None,
    drop_first_seconds: float | None,
) -> PreprocessingOptions:
    return PreprocessingOptions(
        band_low=profile.band_low if band_low is None else band_low,
        band_high=profile.band_high if band_high is None else band_high,
        notch_hz=profile.notch_hz if notch_hz is None else notch_hz,
        pretrial_buffer_seconds=PRETRIAL_BUFFER_SECONDS,
        drop_first_seconds=(
            profile.drop_first_seconds
            if drop_first_seconds is None
            else drop_first_seconds
        ),
    )


def resolve_window_grid(
    profile: BenchmarkProfile,
    dataset: str,
    explicit: list[float] | None,
    step_seconds: float | None,
) -> list[float] | None:
    if explicit is not None or step_seconds is not None:
        return explicit
    if dataset != "Thielen2021" or profile.default_window_seconds_grid is None:
        return explicit
    return list(profile.default_window_seconds_grid)


def resolve_target_fs(
    profile: BenchmarkProfile,
    target_fs: int | None,
) -> int:
    return profile.target_fs if target_fs is None else target_fs


def resolve_event(profile: BenchmarkProfile, event: str | None) -> str:
    return profile.event if event is None else event


def resolve_onset_event(profile: BenchmarkProfile, onset_event: bool | None) -> bool:
    return profile.onset_event if onset_event is None else onset_event


def resolve_encoding_length(
    profile: BenchmarkProfile,
    encoding_length: float | None,
) -> float:
    return profile.encoding_length if encoding_length is None else encoding_length


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--profile",
        choices=benchmark_profile_names(),
        default="legacy",
        help="Named benchmark profile controlling default preprocessing and target fs.",
    )
    parser.add_argument(
        "--data-dir",
        type=Path,
        default=CRATE_ROOT / "data",
        help="Local data root containing the downloaded datasets.",
    )
    parser.add_argument(
        "--output-json",
        type=Path,
        default=CRATE_ROOT / "data/benchmark_results.json",
        help="Path for the raw benchmark results JSON.",
    )
    parser.add_argument(
        "--output-csv",
        type=Path,
        default=CRATE_ROOT / "data/benchmark_results.csv",
        help="Path for the flattened benchmark results CSV.",
    )
    parser.add_argument(
        "--output-html",
        type=Path,
        default=CRATE_ROOT / "data/benchmark_results.html",
        help="Path for the HTML summary report.",
    )
    parser.add_argument(
        "--algorithms",
        nargs="+",
        choices=["etrca", "rcca"],
        default=["etrca", "rcca"],
        help="Algorithms to benchmark.",
    )
    parser.add_argument(
        "--datasets",
        nargs="+",
        default=DEFAULT_DATASETS,
        help="Datasets to benchmark.",
    )
    parser.add_argument(
        "--subjects",
        type=int,
        nargs="+",
        default=None,
        help="Optional explicit subject list to use for every dataset.",
    )
    parser.add_argument(
        "--max-subjects",
        type=int,
        default=None,
        help="Optional cap on subjects per dataset.",
    )
    parser.add_argument(
        "--folds",
        type=int,
        default=5,
        help="Number of chronological folds.",
    )
    parser.add_argument(
        "--fold-index",
        type=int,
        nargs="+",
        default=None,
        help="Optional specific fold indices. Defaults to all folds.",
    )
    parser.add_argument(
        "--target-fs",
        type=int,
        default=None,
        help="Resample all trials to this frequency before fitting. Must divide 250 Hz exactly.",
    )
    parser.add_argument(
        "--target-fs-grid",
        type=int,
        nargs="+",
        default=None,
        help="Optional list of target sample rates to sweep. If omitted, uses --target-fs.",
    )
    parser.add_argument(
        "--window-step-seconds",
        type=float,
        default=None,
        help="Optional decoding-window sweep step in seconds. If set, evaluates 0.5 s style ladders up to the dataset's standard length.",
    )
    parser.add_argument(
        "--window-seconds-grid",
        type=float,
        nargs="+",
        default=None,
        help="Optional explicit decoding-window lengths in seconds. If omitted, uses the full standard window.",
    )
    parser.add_argument(
        "--adc-bits",
        type=int,
        default=24,
        help="Signed ADC bit depth used to map held-out trials into ADC codes.",
    )
    parser.add_argument(
        "--adc-headroom",
        type=float,
        default=0.95,
        help="Fraction of signed full scale to use when mapping held-out trials into ADC codes.",
    )
    parser.add_argument(
        "--encoding-length",
        type=float,
        default=None,
        help="Encoding length in seconds for rCCA.",
    )
    parser.add_argument(
        "--event",
        type=str,
        default=None,
        help="Stimulus event string for rCCA.",
    )
    parser.add_argument(
        "--thielen2021-source",
        choices=["raw", "packaged"],
        default="raw",
        help="Source for Thielen2021 tensors when checking zero-training parity.",
    )
    parser.add_argument(
        "--band-low",
        type=float,
        default=None,
        help="Optional lower cutoff for preprocessing band-pass filter.",
    )
    parser.add_argument(
        "--band-high",
        type=float,
        default=None,
        help="Optional upper cutoff for preprocessing band-pass filter.",
    )
    parser.add_argument(
        "--notch-hz",
        type=float,
        default=None,
        help="Optional mains-notch spacing in Hz. Set <=0 to disable notch filtering.",
    )
    parser.add_argument(
        "--drop-first-seconds",
        type=float,
        default=None,
        help="Optional leading segment to discard within each requested decode window.",
    )
    parser.add_argument("--skip-rust", action="store_true")
    return parser.parse_args()


def subject_list_for_dataset(dataset: str) -> list[int]:
    if dataset == "Thielen2015":
        return list(range(1, 13))
    if dataset == "Thielen2021":
        return list(range(1, 31))
    if dataset in CASTILLOS_PARADIGMS:
        return list(range(1, 13))
    raise ValueError(f"Unsupported dataset {dataset}")


def trial_seconds_for_dataset(dataset: str) -> float:
    if dataset == "Thielen2015":
        return 4.2
    if dataset == "Thielen2021":
        return 31.5
    if dataset in CASTILLOS_PARADIGMS:
        return 2.2
    raise ValueError(f"Unsupported dataset {dataset}")


def validate_target_fs(target_fs: int) -> None:
    if target_fs <= 0:
        raise ValueError(f"target_fs must be positive, got {target_fs}")


def round_half_up(value: float) -> int:
    return int(math.floor(value + 0.5))


def seconds_to_samples(seconds: float, fs: int) -> int:
    return max(1, round_half_up(seconds * fs))


def decode_window_requests(
    full_trial_seconds: float,
    explicit: list[float] | None,
    step_seconds: float | None,
) -> list[float]:
    if explicit is not None:
        values = sorted({float(value) for value in explicit})
    elif step_seconds is not None:
        if step_seconds <= 0.0:
            raise ValueError(
                f"window_step_seconds must be positive, got {step_seconds}"
            )
        values = []
        steps = int(math.floor(full_trial_seconds / step_seconds))
        for idx in range(1, steps + 1):
            values.append(round(idx * step_seconds, 6))
    else:
        values = [full_trial_seconds]

    filtered = [value for value in values if 0.0 < value <= full_trial_seconds]
    if not filtered:
        raise ValueError("No valid window lengths remain after filtering")

    if explicit is not None:
        return sorted(filtered)

    if not any(
        math.isclose(value, full_trial_seconds, abs_tol=1e-9) for value in filtered
    ):
        filtered.append(full_trial_seconds)
    return sorted(filtered)


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


def validate_dataset_algorithm_target_fs(
    dataset: str,
    algorithm: str,
    target_fs: int,
) -> None:
    _ = dataset
    _ = algorithm
    _ = target_fs


def effective_etrca_cycle_size(
    cycle_size: float | None,
    fs: int,
) -> float | None:
    if cycle_size is None:
        return None
    cycle_samples = cycle_size * fs
    if math.isclose(cycle_samples, round(cycle_samples), abs_tol=1e-9):
        return cycle_size
    return None


def load_subject(
    dataset: str,
    subject: int,
    data_dir: Path,
    target_fs: int,
    trial_seconds: float | None = None,
    preprocessing: PreprocessingOptions | None = None,
    thielen2021_source: str = "raw",
) -> SubjectData:
    if dataset == "Thielen2015":
        return load_thielen2015_subject(
            subject, data_dir, target_fs, preprocessing=preprocessing
        )
    if dataset == "Thielen2021":
        return load_thielen2021_subject(
            subject,
            data_dir,
            target_fs,
            trial_seconds=trial_seconds,
            preprocessing=preprocessing,
            source=thielen2021_source,
        )
    if dataset in CASTILLOS_PARADIGMS:
        return load_castillos_subject(
            dataset, subject, data_dir, target_fs, preprocessing=preprocessing
        )
    raise ValueError(f"Unsupported dataset {dataset}")


def load_thielen2015_subject(
    subject: int,
    data_dir: Path,
    target_fs: int,
    preprocessing: PreprocessingOptions | None = None,
) -> SubjectData:
    root = (
        data_dir
        / "MNE-thielen2015-data"
        / "dcc"
        / "DSC_2018.00047_553_v3"
        / "sourcedata"
    )
    trial_seconds = trial_seconds_for_dataset("Thielen2015")
    runs_x = []
    runs_y = []
    stimulus = None

    for run_idx in range(1, THIELEN2015_RUNS + 1):
        block = f"test_sync_{run_idx}"
        prefix = f"sub-{subject:02d}_{block}"
        gdf_path = root / f"sub-{subject:02d}" / block / f"{prefix}.gdf"
        mat_path = root / f"sub-{subject:02d}" / block / f"{prefix}.mat"

        raw = mne.io.read_raw_gdf(
            gdf_path,
            stim_channel="status",
            preload=True,
            verbose=False,
        )
        raw.drop_channels(
            [f"ANA{i}" for i in range(1, 33)] + [f"EXG{i}" for i in range(1, 9)]
        )

        info = loadmat(mat_path)
        labels = info["labels"].astype(np.int64).flatten() - 1
        subset = info["subset"].astype(np.int64).flatten() - 1
        layout = info["layout"].astype(np.int64).flatten() - 1
        codes = info["codes"][:, subset[layout]]
        repeated = np.tile(codes, (4, 1)).T.astype(np.float64)

        if stimulus is None:
            stimulus = repeated
        else:
            assert np.array_equal(stimulus, repeated)

        events = mne.find_events(raw, verbose=False)
        onsets = events[:, 0]
        trials = extract_trials_from_raw(
            raw,
            onsets,
            trial_seconds,
            target_fs,
            preprocessing=preprocessing,
        )
        runs_x.append(trials)
        runs_y.append(labels)

    x = np.concatenate(runs_x, axis=0)
    y = np.concatenate(runs_y, axis=0)
    assert stimulus is not None
    return SubjectData(
        dataset="Thielen2015",
        subject=subject,
        x=x,
        y=y,
        fs=target_fs,
        stimulus=stimulus,
        cycle_size=stimulus.shape[1] / 4 / THIELEN2015_PRESENTATION_RATE,
        trial_seconds=trial_seconds,
        presentation_rate=THIELEN2015_PRESENTATION_RATE,
    )


def load_thielen2021_subject(
    subject: int,
    data_dir: Path,
    target_fs: int,
    trial_seconds: float | None = None,
    preprocessing: PreprocessingOptions | None = None,
    source: str = "raw",
) -> SubjectData:
    if source == "packaged":
        return load_thielen2021_packaged_subject(subject, target_fs, trial_seconds)
    if source != "raw":
        raise ValueError(f"Unsupported Thielen2021 source {source}")
    root = data_dir / "MNE-thielen2021-data" / "dcc" / "DSC_2018.00122_448_v3"
    if trial_seconds is None:
        trial_seconds = trial_seconds_for_dataset("Thielen2021")
    session = THIELEN2021_SESSIONS[subject - 1]
    runs_x = []
    runs_y = []

    codes_path = root / "resources" / "mgold_61_6521_flip_balanced_20.mat"
    codes = loadmat(codes_path)["codes"]
    stimulus = np.asarray(codes.T, dtype=np.float64)

    for block_idx in range(1, THIELEN2021_BLOCKS + 1):
        block = f"block_{block_idx}"
        gdf_path = (
            root
            / "sourcedata"
            / "offline"
            / f"sub-{subject:02d}"
            / block
            / f"sub-{subject:02d}_{session}_{block}_main_eeg.gdf"
        )
        labels_path = (
            root
            / "sourcedata"
            / "offline"
            / f"sub-{subject:02d}"
            / block
            / "trainlabels.mat"
        )

        raw = mne.io.read_raw_gdf(
            gdf_path,
            stim_channel="status",
            preload=True,
            verbose=False,
        )
        mne.rename_channels(
            raw.info,
            {
                "AF3": "Fpz",
                "F3": "T7",
                "FC5": "O1",
                "P7": "POz",
                "P8": "Oz",
                "FC6": "Iz",
                "F4": "O2",
                "AF4": "T8",
            },
        )

        labels = (
            np.array(h5py.File(labels_path, "r")["v"]).astype(np.int64).flatten() - 1
        )
        events = mne.find_events(raw, verbose=False)
        cond = np.logical_or(
            np.diff(events[:, 0]) < 1.8 * raw.info["sfreq"],
            np.diff(events[:, 0]) > 2.4 * raw.info["sfreq"],
        )
        idx = np.concatenate(([0], 1 + np.where(cond)[0]))
        onsets = events[idx, 0]

        trials = extract_trials_from_raw(
            raw,
            onsets,
            trial_seconds,
            target_fs,
            preprocessing=preprocessing,
        )
        runs_x.append(trials)
        runs_y.append(labels)

    x = np.concatenate(runs_x, axis=0)
    y = np.concatenate(runs_y, axis=0)
    return SubjectData(
        dataset="Thielen2021",
        subject=subject,
        x=x,
        y=y,
        fs=target_fs,
        stimulus=stimulus,
        cycle_size=codes.shape[0] / THIELEN2021_PRESENTATION_RATE,
        trial_seconds=trial_seconds,
        presentation_rate=THIELEN2021_PRESENTATION_RATE,
    )


def load_thielen2021_packaged_subject(
    subject: int,
    target_fs: int,
    trial_seconds: float | None = None,
) -> SubjectData:
    if trial_seconds is None:
        trial_seconds = trial_seconds_for_dataset("Thielen2021")

    import pyntbci

    if pyntbci.__file__ is None:
        raise RuntimeError("pyntbci package path is unavailable")

    packaged_path = (
        Path(pyntbci.__file__).resolve().parent
        / "data"
        / f"thielen2021_sub-{subject:02d}.npz"
    )
    raw = np.load(packaged_path)
    fs_raw = int(np.asarray(raw["fs"]).item())
    n_samples = seconds_to_samples(trial_seconds, fs_raw)
    x = np.asarray(raw["X"], dtype=np.float64)[:, :, :n_samples]
    if target_fs != fs_raw:
        x = resample_trials(x, fs_raw, target_fs)
    y = np.asarray(raw["y"], dtype=np.int64)
    stimulus = np.asarray(raw["V"], dtype=np.float64)
    return SubjectData(
        dataset="Thielen2021",
        subject=subject,
        x=x,
        y=y,
        fs=target_fs,
        stimulus=stimulus,
        cycle_size=2.1,
        trial_seconds=trial_seconds,
        presentation_rate=fs_raw,
    )


def load_castillos_subject(
    dataset: str,
    subject: int,
    data_dir: Path,
    target_fs: int,
    preprocessing: PreprocessingOptions | None = None,
) -> SubjectData:
    paradigm = CASTILLOS_PARADIGMS[dataset]
    path = (
        data_dir
        / "MNE-4class-vep-data"
        / "records"
        / "8255618"
        / "files"
        / "4Class-CVEP"
        / f"P{subject}"
        / f"P{subject}_{paradigm}.set"
    )

    raw = mne.io.read_raw_eeglab(path, preload=True, verbose=False)
    to_remove = []
    for idx, description in enumerate(list(raw.annotations.description)):
        if "collects" in description or "iti" in description or description == "[]":
            to_remove.append(idx)
            continue
        code = description.split("_")[0]
        label = description.split("_")[1]
        code = code.replace("\n", "").replace("[", "").replace("]", "").replace(" ", "")
        raw.annotations.description[idx] = f"{code}_{label}"
    if to_remove:
        raw.annotations.delete(np.asarray(to_remove))

    events, event_id = mne.events_from_annotations(raw, event_id="auto", verbose=False)
    labels = events[:, -1]
    labels = labels - np.min(labels)
    x = epoch_and_resample(
        raw,
        events=events,
        target_fs=target_fs,
        tmin=0.0,
        tmax=2.2,
        event_id=event_id,
        preprocessing=preprocessing,
    )
    stimulus = castillos_codebook(event_id)
    return SubjectData(
        dataset=dataset,
        subject=subject,
        x=x,
        y=labels.astype(np.int64),
        fs=target_fs,
        stimulus=stimulus.astype(np.float64),
        cycle_size=None,
        trial_seconds=2.2,
        presentation_rate=CASTILLOS_PRESENTATION_RATE,
    )


def castillos_codebook(event_id: dict[str, int]) -> np.ndarray:
    out = [None] * len(event_id)
    offset = min(event_id.values())
    for key, value in event_id.items():
        code = key.split("_")[0].replace(".", "").replace("2", "")
        out[value - offset] = np.asarray(list(map(int, code)), dtype=np.int64)
    if any(item is None for item in out):
        raise ValueError("Failed to reconstruct Castillos codebook")
    return np.stack(out, axis=0)


def notch_frequencies(fs_raw: float) -> np.ndarray:
    upper = fs_raw / 2.0
    freqs = np.arange(NOTCH_HZ, upper, NOTCH_HZ, dtype=np.float64)
    return freqs[freqs > 0]


def preprocess_raw(
    raw: mne.io.BaseRaw,
    preprocessing: PreprocessingOptions | None = None,
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
        l_freq=settings.band_low,
        h_freq=settings.band_high,
        picks="eeg",
        verbose=False,
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
    valid_onsets = []
    for onset in onsets:
        stop = onset + raw_samples
        if stop <= raw.n_times:
            valid_onsets.append(int(onset))
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


def resample_trials(
    x: np.ndarray,
    fs_raw: float,
    target_fs: int,
) -> np.ndarray:
    fs_raw_int = int(round(fs_raw))
    gcd = math.gcd(fs_raw_int, target_fs)
    up = target_fs // gcd
    down = fs_raw_int // gcd
    resampled = resample_poly(x, up=up, down=down, axis=2)
    target_samples = int(round(x.shape[2] * target_fs / fs_raw))
    if resampled.shape[2] > target_samples:
        resampled = resampled[:, :, :target_samples]
    elif resampled.shape[2] < target_samples:
        pad = target_samples - resampled.shape[2]
        resampled = np.pad(resampled, ((0, 0), (0, 0), (0, pad)))
    return resampled.astype(np.float64)


def stimulus_to_sample_rate(
    stimulus: np.ndarray,
    presentation_rate: int,
    fs: int,
) -> np.ndarray:
    if presentation_rate <= 0:
        raise ValueError(f"presentation_rate must be positive, got {presentation_rate}")
    duration_seconds = stimulus.shape[1] / presentation_rate
    total_samples = seconds_to_samples(duration_seconds, fs)
    sample_positions = np.floor(
        np.arange(total_samples) * presentation_rate / fs
    ).astype(np.int64)
    sample_positions = np.clip(sample_positions, 0, stimulus.shape[1] - 1)
    return np.asarray(stimulus[:, sample_positions], dtype=np.float64)


def slice_windowed_trials_and_stimulus(
    x: np.ndarray,
    stimulus: np.ndarray,
    fs: int,
    presentation_rate: int,
    requested_window_seconds: float,
    drop_first_seconds: float,
) -> tuple[np.ndarray, np.ndarray, dict[str, float | int]]:
    nominal_window_samples = min(
        seconds_to_samples(requested_window_seconds, fs), x.shape[2]
    )
    nominal_window_seconds = nominal_window_samples / fs
    nominal_stimulus_samples = min(
        seconds_to_samples(requested_window_seconds, presentation_rate),
        stimulus.shape[1],
    )
    x_window = x[:, :, :nominal_window_samples]
    stimulus_window = stimulus[:, :nominal_stimulus_samples]

    trim_seconds = max(0.0, drop_first_seconds)
    if trim_seconds > 0.0:
        trim_samples = min(
            seconds_to_samples(trim_seconds, fs),
            max(0, nominal_window_samples - 1),
        )
        trim_stimulus_samples = min(
            seconds_to_samples(trim_seconds, presentation_rate),
            max(0, nominal_stimulus_samples - 1),
        )
    else:
        trim_samples = 0
        trim_stimulus_samples = 0

    x_effective = x_window[:, :, trim_samples:]
    stimulus_effective = stimulus_window[:, trim_stimulus_samples:]
    effective_window_samples = x_effective.shape[2]
    effective_window_seconds = effective_window_samples / fs
    effective_stimulus_samples = stimulus_effective.shape[1]

    return (
        x_effective,
        stimulus_effective,
        {
            "nominal_window_samples": nominal_window_samples,
            "nominal_window_seconds": nominal_window_seconds,
            "effective_window_samples": effective_window_samples,
            "effective_window_seconds": effective_window_seconds,
            "leading_trim_seconds": trim_seconds,
            "leading_trim_samples": trim_samples,
            "nominal_stimulus_samples": nominal_stimulus_samples,
            "effective_stimulus_samples": effective_stimulus_samples,
        },
    )


def fold_slices(n_trials: int, folds: int) -> list[np.ndarray]:
    return [
        np.asarray(indices, dtype=np.int64)
        for indices in np.array_split(np.arange(n_trials), folds)
    ]


def fit_etrca(
    x_train: np.ndarray,
    y_train: np.ndarray,
    fs: int,
    cycle_size: float | None,
) -> Any:
    import pyntbci

    model = pyntbci.classifiers.eTRCA(
        lags=None,
        fs=fs,
        cycle_size=cycle_size,
        ensemble=True,
    )
    model.fit(x_train, y_train)
    return model


def build_etrca_bank(
    model: Any,
    n_samples: int,
    classes: np.ndarray,
) -> tuple[np.ndarray, np.ndarray]:
    n_classes = classes.shape[0]

    if model.w_.ndim == 2:
        spatial = np.repeat(model.w_[:, :, np.newaxis], n_classes, axis=2)
    else:
        spatial = np.asarray(model.w_)

    if spatial.shape[1] != 1:
        raise ValueError(f"Expected one spatial component, got {spatial.shape}")

    templates = np.asarray(model.get_T(n_samples))[:, 0, :].astype(np.float64)
    spatial_filters = np.zeros((n_classes, spatial.shape[0]), dtype=np.float64)
    for class_idx, _class_label in enumerate(classes):
        spatial_filters[class_idx] = spatial[:, 0, class_idx]
    return spatial_filters, templates


def fit_rcca(
    x_train: np.ndarray,
    y_train: np.ndarray,
    stimulus: np.ndarray,
    fs: int,
    event: str,
    encoding_length: float,
) -> Any:
    import pyntbci

    model = pyntbci.classifiers.rCCA(
        stimulus=stimulus,
        fs=fs,
        event=event,
        encoding_length=encoding_length,
        score_metric="correlation",
        ensemble=False,
        n_components=1,
    )
    model.fit(x_train, y_train)
    return model


def build_rcca_bank(
    model: Any,
    n_classes: int,
    n_channels: int,
    n_samples: int,
) -> tuple[np.ndarray, np.ndarray]:
    if n_samples < model.Ts_.shape[2]:
        templates = model.Ts_
    else:
        repeats = n_samples // model.Ts_.shape[2]
        templates = np.concatenate(
            (model.Ts_, np.tile(model.Tw_, (1, 1, repeats))), axis=2
        )
    templates = templates[:, :, :n_samples].copy()
    templates -= templates.mean(axis=2, keepdims=True)
    templates = templates[:, 0, :]

    if model.w_.ndim == 2:
        spatial_filters = np.repeat(model.w_[:, 0][np.newaxis, :], n_classes, axis=0)
    else:
        spatial_filters = np.asarray(model.w_[:, 0, :]).T

    if spatial_filters.shape != (n_classes, n_channels):
        raise ValueError(
            f"Unexpected rCCA filter shape {spatial_filters.shape}, expected {(n_classes, n_channels)}"
        )
    return spatial_filters.astype(np.float64), templates.astype(np.float64)


def quantize_trials_to_adc(
    x: np.ndarray,
    signed_bits: int,
    headroom: float,
) -> tuple[np.ndarray, float]:
    adc_peak = float((1 << (signed_bits - 1)) - 1)
    data_peak = float(np.max(np.abs(x)))
    scale = 1.0 if data_peak == 0.0 else (adc_peak * headroom) / data_peak
    quantized = np.rint(x * scale).clip(-adc_peak - 1.0, adc_peak).astype(np.int32)
    return quantized, scale


def build_rust_binary() -> Path:
    subprocess.run(
        [
            "cargo",
            "build",
            "--quiet",
            "-p",
            "cvep-decoder",
            "--features",
            "host-tools",
            "--bin",
            "projected_correlation_benchmark",
        ],
        check=True,
        cwd=WORKSPACE_ROOT,
    )
    return WORKSPACE_ROOT / "target" / "debug" / "projected_correlation_benchmark"


def run_rust_fixture(fixture_path: Path, rust_binary: Path) -> dict[str, Any]:
    result = subprocess.run(
        [
            str(rust_binary),
            str(fixture_path),
        ],
        check=True,
        capture_output=True,
        text=True,
    )
    return json.loads(result.stdout)


def benchmark_subject_fold_windows(
    algorithm: str,
    data: SubjectData,
    rust_binary: Path | None,
    fold_idx: int,
    folds: int,
    window_requests_seconds: list[float],
    adc_bits: int,
    adc_headroom: float,
    encoding_length: float,
    event: str,
    preprocessing: PreprocessingOptions,
    profile_name: str,
) -> list[dict[str, Any]]:
    classes = np.unique(data.y)
    fold_parts = fold_slices(data.x.shape[0], folds)
    test_idx = fold_parts[fold_idx]
    train_idx = np.concatenate(
        [part for idx, part in enumerate(fold_parts) if idx != fold_idx]
    )

    x_train = data.x[train_idx]
    y_train = data.y[train_idx]
    x_test = data.x[test_idx]
    y_test = data.y[test_idx]
    stimulus_fs = stimulus_to_sample_rate(
        data.stimulus,
        presentation_rate=data.presentation_rate,
        fs=data.fs,
    )

    etrca_model = None
    if algorithm == "etrca":
        etrca_model = fit_etrca(
            x_train,
            y_train,
            data.fs,
            effective_etrca_cycle_size(data.cycle_size, data.fs),
        )

    rows: list[dict[str, Any]] = []
    for requested_window_seconds in window_requests_seconds:
        x_train_window, stimulus_window, window_info = (
            slice_windowed_trials_and_stimulus(
                x_train,
                stimulus_fs,
                data.fs,
                data.fs,
                requested_window_seconds,
                preprocessing.drop_first_seconds,
            )
        )
        x_test_window, _stimulus_unused, _ = slice_windowed_trials_and_stimulus(
            x_test,
            stimulus_fs,
            data.fs,
            data.fs,
            requested_window_seconds,
            preprocessing.drop_first_seconds,
        )
        window_samples = int(window_info["effective_window_samples"])
        actual_window_seconds = float(window_info["effective_window_seconds"])

        if algorithm == "etrca":
            assert etrca_model is not None
            model = etrca_model
        elif algorithm == "rcca":
            model = fit_rcca(
                x_train_window,
                y_train,
                stimulus_window,
                data.fs,
                event=event,
                encoding_length=encoding_length,
            )
        else:
            raise ValueError(f"Unsupported algorithm {algorithm}")

        if algorithm == "etrca":
            spatial_filters, templates = build_etrca_bank(
                model, window_samples, classes
            )
        elif algorithm == "rcca":
            spatial_filters, templates = build_rcca_bank(
                model,
                n_classes=stimulus_window.shape[0],
                n_channels=data.x.shape[1],
                n_samples=window_samples,
            )
        else:
            raise ValueError(f"Unsupported algorithm {algorithm}")

        benchmark_predictions = np.asarray(model.predict(x_test_window), dtype=np.int64)
        quantized_trials, adc_scale = quantize_trials_to_adc(
            x_test_window,
            signed_bits=adc_bits,
            headroom=adc_headroom,
        )

        fixture = {
            "algorithm": algorithm,
            "dataset": data.dataset,
            "subject": data.subject,
            "classes": int(classes.shape[0]),
            "channels": int(data.x.shape[1]),
            "window": int(window_samples),
            "spatial_filters": spatial_filters.astype(np.float32).tolist(),
            "projected_templates": templates.astype(np.float32).tolist(),
            "benchmark_predictions": benchmark_predictions.astype(np.int64).tolist(),
            "benchmark_labels": y_test.astype(np.int64).tolist(),
            "trials_i32": quantized_trials.tolist(),
        }

        if rust_binary is None:
            rust = None
        else:
            with tempfile.TemporaryDirectory(prefix="cvep-benchmark-") as tmp_dir:
                fixture_path = Path(tmp_dir) / "fixture.json"
                fixture_path.write_text(json.dumps(fixture), encoding="utf-8")
                rust = run_rust_fixture(fixture_path, rust_binary)

        rows.append(
            {
                "algorithm": algorithm,
                "dataset": data.dataset,
                "subject": data.subject,
                "fold_index": fold_idx,
                "folds": folds,
                "classes": fixture["classes"],
                "channels": fixture["channels"],
                "target_fs": data.fs,
                "profile": profile_name,
                "cycle_size_seconds": effective_etrca_cycle_size(
                    data.cycle_size, data.fs
                ),
                "train_window_seconds": data.trial_seconds,
                "requested_window_seconds": requested_window_seconds,
                "window_seconds": float(window_info["nominal_window_seconds"]),
                "effective_window_seconds": actual_window_seconds,
                "leading_trim_seconds": float(window_info["leading_trim_seconds"]),
                "window": fixture["window"],
                "train_trials": int(x_train.shape[0]),
                "test_trials": int(x_test.shape[0]),
                "band_low": preprocessing.band_low,
                "band_high": preprocessing.band_high,
                "notch_hz": preprocessing.notch_hz,
                "pyntbci_accuracy": float(np.mean(benchmark_predictions == y_test)),
                "rust_exact_accuracy": (
                    None if rust is None else float(rust["rust_exact_accuracy"])
                ),
                "rust_exact_match_rate": (
                    None if rust is None else float(rust["rust_exact_match_rate"])
                ),
            }
        )
    return rows


def flatten_results_csv(results: list[dict[str, Any]]) -> str:
    keys = [
        "algorithm",
        "dataset",
        "subject",
        "fold_index",
        "folds",
        "classes",
        "channels",
        "profile",
        "target_fs",
        "band_low",
        "band_high",
        "notch_hz",
        "train_window_seconds",
        "requested_window_seconds",
        "window_seconds",
        "effective_window_seconds",
        "leading_trim_seconds",
        "window",
        "train_trials",
        "test_trials",
        "pyntbci_accuracy",
        "rust_exact_accuracy",
        "rust_exact_match_rate",
    ]
    lines = [",".join(keys)]
    for row in results:
        fields = []
        for key in keys:
            value = row.get(key)
            fields.append("" if value is None else str(value))
        lines.append(",".join(fields))
    return "\n".join(lines) + "\n"


def mean_or_none(values: list[float | None]) -> float | None:
    filtered = [value for value in values if value is not None]
    if not filtered:
        return None
    return float(np.mean(filtered))


def fmt_metric(value: float | None) -> str:
    return "-" if value is None else f"{value:.4f}"


def grouped_summary_rows(results: list[dict[str, Any]]) -> list[dict[str, Any]]:
    grouped: dict[tuple[str, str, str, int, int], list[dict[str, Any]]] = {}
    for row in results:
        grouped.setdefault(
            (
                row["algorithm"],
                row["dataset"],
                row.get("profile", "legacy"),
                row["target_fs"],
                row["window"],
            ),
            [],
        ).append(row)

    summaries = []
    for (algorithm, dataset, profile, target_fs, window), rows in sorted(
        grouped.items()
    ):
        summaries.append(
            {
                "algorithm": algorithm,
                "dataset": dataset,
                "profile": profile,
                "target_fs": target_fs,
                "window": window,
                "window_seconds": rows[0]["window_seconds"],
                "effective_window_seconds": rows[0].get("effective_window_seconds"),
                "requested_window_seconds": rows[0]["requested_window_seconds"],
                "subjects": len({row["subject"] for row in rows}),
                "mean_pyntbci_accuracy": float(
                    np.mean([row["pyntbci_accuracy"] for row in rows])
                ),
                "mean_rust_exact_accuracy": mean_or_none(
                    [row.get("rust_exact_accuracy") for row in rows]
                ),
                "mean_rust_exact_match_rate": mean_or_none(
                    [row.get("rust_exact_match_rate") for row in rows]
                ),
            }
        )
    return summaries


def optional_mean(rows: list[dict[str, Any]], key: str) -> float | None:
    values = [row[key] for row in rows if row[key] is not None]
    if not values:
        return None
    return float(np.mean(values))


def render_html_report(
    output: Path,
    config: dict[str, Any],
    results: list[dict[str, Any]],
) -> None:
    summary = grouped_summary_rows(results)
    summary_rows = "\n".join(
        (
            "<tr>"
            f"<td>{html.escape(row['algorithm'])}</td>"
            f"<td>{html.escape(row['dataset'])}</td>"
            f"<td>{row['target_fs']}</td>"
            f"<td>{row['requested_window_seconds']:.3f}</td>"
            f"<td>{row['window_seconds']:.3f}</td>"
            f"<td>{row['subjects']}</td>"
            f"<td>{row['mean_pyntbci_accuracy']:.4f}</td>"
            f"<td>{fmt_metric(row['mean_rust_exact_accuracy'])}</td>"
            f"<td>{fmt_metric(row['mean_rust_exact_match_rate'])}</td>"
            "</tr>"
        )
        for row in summary
    )
    detail_rows = "\n".join(
        (
            "<tr>"
            f"<td>{html.escape(row['algorithm'])}</td>"
            f"<td>{html.escape(row['dataset'])}</td>"
            f"<td>{row['target_fs']}</td>"
            f"<td>{row['subject']}</td>"
            f"<td>{row['fold_index']}</td>"
            f"<td>{row['train_trials']}</td>"
            f"<td>{row['test_trials']}</td>"
            f"<td>{row['classes']}</td>"
            f"<td>{row['channels']}</td>"
            f"<td>{row['requested_window_seconds']:.3f}</td>"
            f"<td>{row['window_seconds']:.3f}</td>"
            f"<td>{row['window']}</td>"
            f"<td>{row['pyntbci_accuracy']:.4f}</td>"
            f"<td>{fmt_metric(row['rust_exact_accuracy'])}</td>"
            f"<td>{fmt_metric(row['rust_exact_match_rate'])}</td>"
            "</tr>"
        )
        for row in results
    )
    config_html = html.escape(json.dumps(config, indent=2))
    document = f"""<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>CVEP Benchmark Report</title>
  <style>
    :root {{
      color-scheme: light;
      --bg: #f5f1e8;
      --panel: #fffdf8;
      --ink: #1f2933;
      --muted: #5b6875;
      --line: #d9cfbf;
      --accent: #0f766e;
    }}
    body {{
      margin: 0;
      background: linear-gradient(180deg, #efe4cc 0%, var(--bg) 18%, #f8f4ec 100%);
      color: var(--ink);
      font-family: Georgia, "Iowan Old Style", "Palatino Linotype", serif;
    }}
    main {{
      max-width: 1200px;
      margin: 0 auto;
      padding: 32px 20px 48px;
    }}
    h1, h2 {{
      margin: 0 0 12px;
      font-weight: 600;
      letter-spacing: 0.01em;
    }}
    p {{
      color: var(--muted);
      margin: 0 0 16px;
    }}
    .card {{
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 18px;
      padding: 20px;
      box-shadow: 0 14px 40px rgba(55, 41, 20, 0.08);
      margin-bottom: 20px;
    }}
    .meta {{
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
      gap: 12px;
      margin-bottom: 8px;
    }}
    .meta div {{
      background: #f8f4eb;
      border: 1px solid #eadfcd;
      border-radius: 12px;
      padding: 12px 14px;
    }}
    .label {{
      display: block;
      font-size: 0.78rem;
      color: var(--muted);
      margin-bottom: 6px;
      text-transform: uppercase;
      letter-spacing: 0.08em;
    }}
    .value {{
      font-size: 1rem;
      color: var(--ink);
    }}
    table {{
      width: 100%;
      border-collapse: collapse;
      font-family: "SFMono-Regular", "Menlo", monospace;
      font-size: 0.9rem;
    }}
    th, td {{
      padding: 10px 12px;
      border-bottom: 1px solid var(--line);
      text-align: left;
      vertical-align: top;
    }}
    th {{
      color: var(--accent);
      font-weight: 700;
      background: #faf6ef;
      position: sticky;
      top: 0;
    }}
    .table-wrap {{
      overflow: auto;
      border: 1px solid var(--line);
      border-radius: 14px;
    }}
    pre {{
      margin: 0;
      overflow: auto;
      background: #fbf8f1;
      border: 1px solid var(--line);
      border-radius: 14px;
      padding: 16px;
      color: var(--ink);
    }}
  </style>
</head>
<body>
  <main>
    <section class="card">
      <h1>CVEP Benchmark Report</h1>
      <p>Comparison of PyntBCI reference decoding against the Rust projected-correlation runtime.</p>
      <div class="meta">
        <div><span class="label">Datasets</span><span class="value">{html.escape(", ".join(config["datasets"]))}</span></div>
        <div><span class="label">Algorithms</span><span class="value">{html.escape(", ".join(config["algorithms"]))}</span></div>
        <div><span class="label">Folds</span><span class="value">{config["folds"]}</span></div>
        <div><span class="label">Target fs grid</span><span class="value">{html.escape(", ".join(map(str, config["target_fs_grid"])))} Hz</span></div>
        <div><span class="label">Window step</span><span class="value">{html.escape(str(config["window_step_seconds"])) if config["window_step_seconds"] is not None else "full only"}</span></div>
        <div><span class="label">ADC bits</span><span class="value">{config["adc_bits"]}</span></div>
        <div><span class="label">Rows</span><span class="value">{len(results)}</span></div>
      </div>
    </section>
    <section class="card">
      <h2>Summary</h2>
      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Algorithm</th>
              <th>Dataset</th>
              <th>fs</th>
              <th>Requested s</th>
              <th>Actual s</th>
              <th>Subjects</th>
              <th>Mean PyntBCI</th>
              <th>Mean Rust exact</th>
              <th>Mean exact match</th>
            </tr>
          </thead>
          <tbody>
            {summary_rows}
          </tbody>
        </table>
      </div>
    </section>
    <section class="card">
      <h2>Per Subject Fold</h2>
      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Algorithm</th>
              <th>Dataset</th>
              <th>fs</th>
              <th>Subject</th>
              <th>Fold</th>
              <th>Train</th>
              <th>Test</th>
              <th>Classes</th>
              <th>Channels</th>
              <th>Requested s</th>
              <th>Actual s</th>
              <th>Window</th>
              <th>PyntBCI</th>
              <th>Rust exact</th>
              <th>Exact match</th>
            </tr>
          </thead>
          <tbody>
            {detail_rows}
          </tbody>
        </table>
      </div>
    </section>
    <section class="card">
      <h2>Config</h2>
      <pre>{config_html}</pre>
    </section>
  </main>
</body>
</html>
"""
    output.write_text(document, encoding="utf-8")


def format_optional_html_metric(value: float | None) -> str:
    if value is None:
        return "-"
    return f"{value:.4f}"


def render_summary(console: Console, results: list[dict[str, Any]]) -> None:
    table = Table(title="PyntBCI vs Rust benchmark summary")
    table.add_column("Algorithm")
    table.add_column("Dataset")
    table.add_column("fs")
    table.add_column("Req s")
    table.add_column("Actual s")
    table.add_column("Subjects")
    table.add_column("Mean PyntBCI")
    table.add_column("Mean Rust exact")
    table.add_column("Mean exact match")

    for row in grouped_summary_rows(results):
        table.add_row(
            row["algorithm"],
            row["dataset"],
            str(row["target_fs"]),
            f"{row['requested_window_seconds']:.3f}",
            f"{row['window_seconds']:.3f}",
            str(row["subjects"]),
            f"{row['mean_pyntbci_accuracy']:.4f}",
            fmt_metric(row["mean_rust_exact_accuracy"]),
            fmt_metric(row["mean_rust_exact_match_rate"]),
        )

    console.print(table)


def main() -> None:
    args = parse_args()
    console = Console()
    profile = resolve_benchmark_profile(args.profile)
    preprocessing = resolve_preprocessing_options(
        profile,
        band_low=args.band_low,
        band_high=args.band_high,
        notch_hz=args.notch_hz,
        drop_first_seconds=args.drop_first_seconds,
    )
    encoding_length = resolve_encoding_length(profile, args.encoding_length)
    event = resolve_event(profile, args.event)
    rust_binary = None if args.skip_rust else build_rust_binary()
    resolved_target_fs = resolve_target_fs(profile, args.target_fs)
    target_fs_grid = args.target_fs_grid or [resolved_target_fs]
    for target_fs in target_fs_grid:
        validate_target_fs(target_fs)

    output_json = args.output_json
    output_csv = args.output_csv
    output_html = args.output_html
    output_json.parent.mkdir(parents=True, exist_ok=True)
    output_csv.parent.mkdir(parents=True, exist_ok=True)
    output_html.parent.mkdir(parents=True, exist_ok=True)

    fold_indices = (
        args.fold_index if args.fold_index is not None else list(range(args.folds))
    )
    results: list[dict[str, Any]] = []
    resolved_window_grid = args.window_seconds_grid

    for dataset in args.datasets:
        subjects = args.subjects or subject_list_for_dataset(dataset)
        if args.max_subjects is not None:
            subjects = subjects[: args.max_subjects]

        for target_fs in target_fs_grid:
            for algorithm in args.algorithms:
                validate_dataset_algorithm_target_fs(dataset, algorithm, target_fs)

        for subject in subjects:
            for target_fs in target_fs_grid:
                full_trial_seconds = trial_seconds_for_dataset(dataset)
                resolved_window_grid = resolve_window_grid(
                    profile,
                    dataset,
                    args.window_seconds_grid,
                    args.window_step_seconds,
                )
                window_requests_seconds = decode_window_requests(
                    full_trial_seconds,
                    explicit=resolved_window_grid,
                    step_seconds=args.window_step_seconds,
                )
                data_cache: dict[tuple[str, float | None], SubjectData] = {}

                for algorithm in args.algorithms:
                    grouped_windows = [window_requests_seconds]
                    use_direct_window_trials = (
                        loader_trial_seconds_for_algorithm(
                            dataset,
                            algorithm,
                            requested_window_seconds=full_trial_seconds,
                        )
                        is not None
                    )
                    if use_direct_window_trials:
                        grouped_windows = [
                            [window] for window in window_requests_seconds
                        ]

                    for windows in grouped_windows:
                        load_seconds = (
                            loader_trial_seconds_for_algorithm(
                                dataset,
                                algorithm,
                                requested_window_seconds=windows[0],
                            )
                            if use_direct_window_trials
                            else None
                        )
                        cache_key = (algorithm, load_seconds)
                        if cache_key not in data_cache:
                            label_seconds = (
                                load_seconds
                                if load_seconds is not None
                                else full_trial_seconds
                            )
                            console.print(
                                f"[cyan]loading[/cyan] dataset={dataset} subject={subject} "
                                f"target_fs={target_fs} algorithm={algorithm} "
                                f"trial_seconds={label_seconds:.3f}"
                            )
                            data_cache[cache_key] = load_subject(
                                dataset,
                                subject,
                                args.data_dir,
                                target_fs,
                                trial_seconds=load_seconds,
                                preprocessing=preprocessing,
                                thielen2021_source=args.thielen2021_source,
                            )
                            loaded = data_cache[cache_key]
                            console.print(
                                f"[green]loaded[/green] dataset={dataset} subject={subject} "
                                f"target_fs={target_fs} algorithm={algorithm} "
                                f"shape={tuple(loaded.x.shape)} classes={len(np.unique(loaded.y))} "
                                f"windows={len(windows)}"
                            )

                        data = data_cache[cache_key]
                        for fold_idx in fold_indices:
                            console.print(
                                f"[blue]benchmarking[/blue] algorithm={algorithm} "
                                f"dataset={dataset} subject={subject} target_fs={target_fs} "
                                f"fold={fold_idx}/{args.folds - 1} "
                                f"windows={','.join(f'{value:.3f}' for value in windows)}"
                            )
                            results.extend(
                                benchmark_subject_fold_windows(
                                    algorithm,
                                    data,
                                    rust_binary=rust_binary,
                                    fold_idx=fold_idx,
                                    folds=args.folds,
                                    window_requests_seconds=windows,
                                    adc_bits=args.adc_bits,
                                    adc_headroom=args.adc_headroom,
                                    encoding_length=encoding_length,
                                    event=event,
                                    preprocessing=preprocessing,
                                    profile_name=profile.name,
                                )
                            )

    payload = {
        "config": {
            "datasets": args.datasets,
            "profile": profile.name,
            "profile_description": profile.description,
            "algorithms": args.algorithms,
            "folds": args.folds,
            "fold_indices": fold_indices,
            "target_fs_grid": target_fs_grid,
            "window_seconds_grid": resolved_window_grid,
            "window_step_seconds": args.window_step_seconds,
            "adc_bits": args.adc_bits,
            "adc_headroom": args.adc_headroom,
            "encoding_length": encoding_length,
            "event": event,
            "preprocessing": {
                "band_low": preprocessing.band_low,
                "band_high": preprocessing.band_high,
                "notch_hz": preprocessing.notch_hz,
                "drop_first_seconds": preprocessing.drop_first_seconds,
            },
            "skip_rust": args.skip_rust,
            "thielen2021_source": args.thielen2021_source,
        },
        "results": results,
    }
    output_json.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    output_csv.write_text(flatten_results_csv(results), encoding="utf-8")
    render_html_report(output_html, payload["config"], results)
    render_summary(console, results)
    console.print(f"[green]wrote[/green] {output_json}")
    console.print(f"[green]wrote[/green] {output_csv}")
    console.print(f"[green]wrote[/green] {output_html}")


if __name__ == "__main__":
    main()
