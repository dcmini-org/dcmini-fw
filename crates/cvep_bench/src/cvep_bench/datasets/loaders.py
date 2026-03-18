from __future__ import annotations

import math
from pathlib import Path

import h5py
import mne
import numpy as np
from scipy.io import loadmat

from cvep_bench.datasets.models import PreprocessingOptions, SubjectData
from cvep_bench.datasets.preprocessing import (
    epoch_and_resample,
    extract_trials_from_raw,
    resample_trials,
)
from cvep_bench.datasets.windows import seconds_to_samples


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


def effective_etrca_cycle_size(cycle_size: float | None, fs: int) -> float | None:
    if cycle_size is None:
        return None
    cycle_samples = cycle_size * fs
    if math.isclose(cycle_samples, round(cycle_samples), abs_tol=1e-9):
        return cycle_size
    return None


def validate_dataset_algorithm_target_fs(
    dataset: str, algorithm: str, target_fs: int
) -> None:
    _ = dataset
    _ = algorithm
    _ = target_fs


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
            gdf_path, stim_channel="status", preload=True, verbose=False
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
        trials = extract_trials_from_raw(
            raw, events[:, 0], trial_seconds, target_fs, preprocessing=preprocessing
        )
        runs_x.append(trials)
        runs_y.append(labels)
    assert stimulus is not None
    x = np.concatenate(runs_x, axis=0)
    y = np.concatenate(runs_y, axis=0)
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
    codes = loadmat(root / "resources" / "mgold_61_6521_flip_balanced_20.mat")["codes"]
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
            gdf_path, stim_channel="status", preload=True, verbose=False
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
        onsets = events[np.concatenate(([0], 1 + np.where(cond)[0])), 0]
        trials = extract_trials_from_raw(
            raw, onsets, trial_seconds, target_fs, preprocessing=preprocessing
        )
        runs_x.append(trials)
        runs_y.append(labels)
    return SubjectData(
        dataset="Thielen2021",
        subject=subject,
        x=np.concatenate(runs_x, axis=0),
        y=np.concatenate(runs_y, axis=0),
        fs=target_fs,
        stimulus=stimulus,
        cycle_size=codes.shape[0] / THIELEN2021_PRESENTATION_RATE,
        trial_seconds=trial_seconds,
        presentation_rate=THIELEN2021_PRESENTATION_RATE,
    )


def load_thielen2021_packaged_subject(
    subject: int, target_fs: int, trial_seconds: float | None = None
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
    x = np.asarray(raw["X"], dtype=np.float64)[
        :, :, : seconds_to_samples(trial_seconds, fs_raw)
    ]
    if target_fs != fs_raw:
        x = resample_trials(x, fs_raw, target_fs)
    return SubjectData(
        dataset="Thielen2021",
        subject=subject,
        x=x,
        y=np.asarray(raw["y"], dtype=np.int64),
        fs=target_fs,
        stimulus=np.asarray(raw["V"], dtype=np.float64),
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
        code, label = description.split("_")[0], description.split("_")[1]
        code = code.replace("\n", "").replace("[", "").replace("]", "").replace(" ", "")
        raw.annotations.description[idx] = f"{code}_{label}"
    if to_remove:
        raw.annotations.delete(np.asarray(to_remove))
    events, event_id = mne.events_from_annotations(raw, event_id="auto", verbose=False)
    labels = events[:, -1] - np.min(events[:, -1])
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
