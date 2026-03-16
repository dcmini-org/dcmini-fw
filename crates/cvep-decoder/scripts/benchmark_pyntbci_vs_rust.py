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


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
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
        default=250,
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
        default=0.3,
        help="Encoding length in seconds for rCCA.",
    )
    parser.add_argument(
        "--event",
        type=str,
        default="refe",
        help="Stimulus event string for rCCA.",
    )
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
    if DC_MINI_BASE_FS % target_fs != 0:
        raise ValueError(
            f"target_fs={target_fs} is not an integer divisor of the DC-mini ADS1299 base "
            f"rate {DC_MINI_BASE_FS} Hz"
        )


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
            raise ValueError(f"window_step_seconds must be positive, got {step_seconds}")
        values = []
        steps = int(math.floor(full_trial_seconds / step_seconds))
        for idx in range(1, steps + 1):
            values.append(round(idx * step_seconds, 6))
    else:
        values = [full_trial_seconds]

    filtered = [value for value in values if 0.0 < value <= full_trial_seconds]
    if not filtered:
        raise ValueError("No valid window lengths remain after filtering")

    if not any(math.isclose(value, full_trial_seconds, abs_tol=1e-9) for value in filtered):
        filtered.append(full_trial_seconds)
    return sorted(filtered)


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
) -> SubjectData:
    if dataset == "Thielen2015":
        return load_thielen2015_subject(subject, data_dir, target_fs)
    if dataset == "Thielen2021":
        return load_thielen2021_subject(subject, data_dir, target_fs)
    if dataset in CASTILLOS_PARADIGMS:
        return load_castillos_subject(dataset, subject, data_dir, target_fs)
    raise ValueError(f"Unsupported dataset {dataset}")


def load_thielen2015_subject(
    subject: int,
    data_dir: Path,
    target_fs: int,
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
) -> SubjectData:
    root = data_dir / "MNE-thielen2021-data" / "dcc" / "DSC_2018.00122_448_v3"
    trial_seconds = trial_seconds_for_dataset("Thielen2021")
    session = THIELEN2021_SESSIONS[subject - 1]
    runs_x = []
    runs_y = []

    codes_path = root / "resources" / "mgold_61_6521_flip_balanced_20.mat"
    codes = loadmat(codes_path)["codes"]
    presentation_samples = int(round(trial_seconds * THIELEN2021_PRESENTATION_RATE))
    stimulus = np.tile(codes, (math.ceil(presentation_samples / codes.shape[0]), 1))[
        :presentation_samples
    ].T.astype(np.float64)

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


def load_castillos_subject(
    dataset: str,
    subject: int,
    data_dir: Path,
    target_fs: int,
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
) -> None:
    freqs = notch_frequencies(raw.info["sfreq"])
    if freqs.size:
        raw.notch_filter(freqs=freqs, picks="eeg", verbose=False)
    raw.filter(
        l_freq=BANDPASS_HZ[0],
        h_freq=BANDPASS_HZ[1],
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
) -> np.ndarray:
    preprocess_raw(raw)
    epochs = mne.Epochs(
        raw,
        events=events,
        event_id=event_id,
        tmin=tmin - PRETRIAL_BUFFER_SECONDS,
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
) -> np.ndarray:
    raw_samples = int(
        round((trial_seconds + PRETRIAL_BUFFER_SECONDS) * raw.info["sfreq"])
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
    rust_binary: Path,
    fold_idx: int,
    folds: int,
    window_requests_seconds: list[float],
    adc_bits: int,
    adc_headroom: float,
    encoding_length: float,
    event: str,
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

    if algorithm == "etrca":
        model = fit_etrca(
            x_train,
            y_train,
            data.fs,
            effective_etrca_cycle_size(data.cycle_size, data.fs),
        )
    elif algorithm == "rcca":
        model = fit_rcca(
            x_train,
            y_train,
            data.stimulus,
            data.fs,
            event=event,
            encoding_length=encoding_length,
        )
    else:
        raise ValueError(f"Unsupported algorithm {algorithm}")

    rows: list[dict[str, Any]] = []
    for requested_window_seconds in window_requests_seconds:
        requested_samples = seconds_to_samples(requested_window_seconds, data.fs)
        window_samples = min(requested_samples, data.x.shape[2])
        actual_window_seconds = window_samples / data.fs
        x_test_window = x_test[:, :, :window_samples]

        if algorithm == "etrca":
            spatial_filters, templates = build_etrca_bank(model, window_samples, classes)
        elif algorithm == "rcca":
            spatial_filters, templates = build_rcca_bank(
                model,
                n_classes=data.stimulus.shape[0],
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
                "cycle_size_seconds": effective_etrca_cycle_size(data.cycle_size, data.fs),
                "train_window_seconds": data.trial_seconds,
                "requested_window_seconds": requested_window_seconds,
                "window_seconds": actual_window_seconds,
                "window": fixture["window"],
                "train_trials": int(x_train.shape[0]),
                "test_trials": int(x_test.shape[0]),
                "pyntbci_accuracy": float(np.mean(benchmark_predictions == y_test)),
                "rust_exact_accuracy": float(rust["rust_exact_accuracy"]),
                "rust_exact_match_rate": float(rust["rust_exact_match_rate"]),
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
        "target_fs",
        "train_window_seconds",
        "requested_window_seconds",
        "window_seconds",
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


def grouped_summary_rows(results: list[dict[str, Any]]) -> list[dict[str, Any]]:
    grouped: dict[tuple[str, str, int, int], list[dict[str, Any]]] = {}
    for row in results:
        grouped.setdefault(
            (row["algorithm"], row["dataset"], row["target_fs"], row["window"]),
            [],
        ).append(row)

    summaries = []
    for (algorithm, dataset, target_fs, window), rows in sorted(grouped.items()):
        summaries.append(
            {
                "algorithm": algorithm,
                "dataset": dataset,
                "target_fs": target_fs,
                "window": window,
                "window_seconds": rows[0]["window_seconds"],
                "requested_window_seconds": rows[0]["requested_window_seconds"],
                "subjects": len({row["subject"] for row in rows}),
                "mean_pyntbci_accuracy": float(
                    np.mean([row["pyntbci_accuracy"] for row in rows])
                ),
                "mean_rust_exact_accuracy": float(
                    np.mean([row["rust_exact_accuracy"] for row in rows])
                ),
                "mean_rust_exact_match_rate": float(
                    np.mean([row["rust_exact_match_rate"] for row in rows])
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
            f"<td>{row['mean_rust_exact_accuracy']:.4f}</td>"
            f"<td>{row['mean_rust_exact_match_rate']:.4f}</td>"
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
            f"<td>{row['rust_exact_accuracy']:.4f}</td>"
            f"<td>{row['rust_exact_match_rate']:.4f}</td>"
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
            f"{row['mean_rust_exact_accuracy']:.4f}",
            f"{row['mean_rust_exact_match_rate']:.4f}",
        )

    console.print(table)


def main() -> None:
    args = parse_args()
    console = Console()
    rust_binary = build_rust_binary()
    target_fs_grid = args.target_fs_grid or [args.target_fs]
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

    for dataset in args.datasets:
        subjects = args.subjects or subject_list_for_dataset(dataset)
        if args.max_subjects is not None:
            subjects = subjects[: args.max_subjects]

        for target_fs in target_fs_grid:
            for algorithm in args.algorithms:
                validate_dataset_algorithm_target_fs(dataset, algorithm, target_fs)

        for subject in subjects:
            for target_fs in target_fs_grid:
                console.print(
                    f"[cyan]loading[/cyan] dataset={dataset} subject={subject} target_fs={target_fs}"
                )
                data = load_subject(dataset, subject, args.data_dir, target_fs)
                window_requests_seconds = decode_window_requests(
                    data.trial_seconds,
                    explicit=args.window_seconds_grid,
                    step_seconds=args.window_step_seconds,
                )
                console.print(
                    f"[green]loaded[/green] dataset={dataset} subject={subject} "
                    f"target_fs={target_fs} shape={tuple(data.x.shape)} "
                    f"classes={len(np.unique(data.y))} windows={len(window_requests_seconds)}"
                )

                for algorithm in args.algorithms:
                    for fold_idx in fold_indices:
                        console.print(
                            f"[blue]benchmarking[/blue] algorithm={algorithm} "
                            f"dataset={dataset} subject={subject} target_fs={target_fs} "
                            f"fold={fold_idx}/{args.folds - 1}"
                        )
                        results.extend(
                            benchmark_subject_fold_windows(
                                algorithm,
                                data,
                                rust_binary=rust_binary,
                                fold_idx=fold_idx,
                                folds=args.folds,
                                window_requests_seconds=window_requests_seconds,
                                adc_bits=args.adc_bits,
                                adc_headroom=args.adc_headroom,
                                encoding_length=args.encoding_length,
                                event=args.event,
                            )
                        )

    payload = {
        "config": {
            "datasets": args.datasets,
            "algorithms": args.algorithms,
            "folds": args.folds,
            "fold_indices": fold_indices,
            "target_fs_grid": target_fs_grid,
            "window_seconds_grid": args.window_seconds_grid,
            "window_step_seconds": args.window_step_seconds,
            "adc_bits": args.adc_bits,
            "adc_headroom": args.adc_headroom,
            "encoding_length": args.encoding_length,
            "event": args.event,
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
