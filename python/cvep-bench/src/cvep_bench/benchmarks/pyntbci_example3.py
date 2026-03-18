from __future__ import annotations

import argparse
import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import numpy as np
from rich.console import Console

from cvep_bench.algorithms.pyntbci_models import (
    build_etrca_bank,
    fit_etrca,
    quantize_trials_to_adc,
)
from cvep_bench.benchmarks.reporting import (
    render_rich_table,
    render_tabular_html,
    write_json_payload,
)
from cvep_bench.runtime.binaries import build_rust_binary
from cvep_bench.runtime.json_fixtures import temporary_fixture_path
from cvep_bench.runtime.runner import run_fixture_payload


@dataclass
class SubjectDataset:
    subject: str
    x: np.ndarray
    y: np.ndarray
    v: np.ndarray
    fs: int


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output-json",
        type=Path,
        default=Path("crates/cvep-decoder/data/example3_etrca_results.json"),
    )
    parser.add_argument(
        "--output-csv",
        type=Path,
        default=Path("crates/cvep-decoder/data/example3_etrca_results.csv"),
    )
    parser.add_argument(
        "--output-html",
        type=Path,
        default=Path("crates/cvep-decoder/data/example3_etrca_results.html"),
    )
    parser.add_argument("--n-subjects", type=int, default=5)
    parser.add_argument("--n-trials", type=int, default=100)
    parser.add_argument("--trialtime", type=float, default=4.2)
    parser.add_argument("--cycle-size", type=float, default=2.1)
    parser.add_argument("--folds", type=int, default=5)
    parser.add_argument("--segmenttime", type=float, default=0.1)
    parser.add_argument("--intertrialtime", type=float, default=1.0)
    parser.add_argument("--curve-subject", type=str, default="sub-01")
    parser.add_argument("--adc-bits", type=int, default=24)
    parser.add_argument("--adc-headroom", type=float, default=0.95)
    return parser.parse_args()


def packaged_pyntbci_path() -> Path:
    import pyntbci

    if pyntbci.__file__ is None:
        raise RuntimeError("pyntbci package path is unavailable")
    return Path(pyntbci.__file__).resolve().parent


def load_subject_dataset(
    subject: str, n_trials: int, trialtime: float
) -> SubjectDataset:
    root = packaged_pyntbci_path()
    raw = np.load(root / "data" / f"thielen2021_{subject}.npz")
    fs = int(np.asarray(raw["fs"]).item())
    n_samples = int(round(trialtime * fs))
    return SubjectDataset(
        subject=subject,
        x=np.asarray(raw["X"], dtype=np.float64)[:n_trials, :, :n_samples],
        y=np.asarray(raw["y"], dtype=np.int64)[:n_trials],
        v=np.asarray(raw["V"], dtype=np.float64),
        fs=fs,
    )


def chronological_folds(n_trials: int, n_folds: int) -> np.ndarray:
    if n_trials % n_folds != 0:
        raise ValueError(
            f"Expected n_trials divisible by n_folds, got {n_trials=} {n_folds=}"
        )
    return np.repeat(np.arange(n_folds), n_trials // n_folds)


def benchmark_subject_fold(
    dataset: SubjectDataset,
    fold_index: int,
    folds: np.ndarray,
    cycle_size: float,
    rust_binary: Path,
    adc_bits: int,
    adc_headroom: float,
) -> dict[str, Any]:
    train_mask = folds != fold_index
    test_mask = ~train_mask
    x_train = dataset.x[train_mask]
    y_train = dataset.y[train_mask]
    x_test = dataset.x[test_mask]
    y_test = dataset.y[test_mask]
    model = fit_etrca(x_train, y_train, dataset.fs, cycle_size)
    pyntbci_pred = np.asarray(model.predict(x_test), dtype=np.int64)
    classes = np.unique(dataset.y)
    spatial_filters, templates = build_etrca_bank(model, dataset.x.shape[2], classes)
    trials_i32, _scale = quantize_trials_to_adc(x_test, adc_bits, adc_headroom)
    fixture = {
        "algorithm": "etrca",
        "dataset": "pyntbci_example3",
        "subject": dataset.subject,
        "classes": int(classes.shape[0]),
        "channels": int(dataset.x.shape[1]),
        "window": int(dataset.x.shape[2]),
        "spatial_filters": spatial_filters.astype(np.float32).tolist(),
        "projected_templates": templates.astype(np.float32).tolist(),
        "benchmark_predictions": pyntbci_pred.astype(np.int64).tolist(),
        "benchmark_labels": y_test.astype(np.int64).tolist(),
        "trials_i32": trials_i32.tolist(),
    }
    with temporary_fixture_path(prefix="cvep-example3-") as fixture_path:
        rust = run_fixture_payload(rust_binary, fixture, fixture_path=fixture_path)
    return {
        "subject": dataset.subject,
        "fold_index": fold_index,
        "pyntbci_accuracy": float(np.mean(pyntbci_pred == y_test)),
        "rust_exact_accuracy": float(rust["rust_exact_accuracy"]),
        "rust_exact_match_rate": float(rust["rust_exact_match_rate"]),
    }


def main() -> None:
    args = parse_args()
    rust_binary = build_rust_binary("projected_correlation_benchmark")
    subjects = [f"sub-{idx:02d}" for idx in range(1, args.n_subjects + 1)]
    rows = []
    for subject in subjects:
        data = load_subject_dataset(subject, args.n_trials, args.trialtime)
        folds = chronological_folds(args.n_trials, args.folds)
        for fold_index in range(args.folds):
            rows.append(
                benchmark_subject_fold(
                    data,
                    fold_index,
                    folds,
                    args.cycle_size,
                    rust_binary,
                    args.adc_bits,
                    args.adc_headroom,
                )
            )
    payload = {"config": vars(args), "results": rows}
    write_json_payload(args.output_json, payload)
    args.output_csv.write_text(
        "subject,fold_index,pyntbci_accuracy,rust_exact_accuracy,rust_exact_match_rate\n"
        + "\n".join(
            f"{r['subject']},{r['fold_index']},{r['pyntbci_accuracy']},{r['rust_exact_accuracy']},{r['rust_exact_match_rate']}"
            for r in rows
        )
        + "\n",
        encoding="utf-8",
    )
    render_tabular_html(
        args.output_html,
        title="PyNTBCI Example 3 eTRCA",
        subtitle="Chronological fold parity between PyNTBCI and Rust on packaged example data.",
        config=payload["config"],
        summary_columns=[
            ("Subject", "subject"),
            ("Fold", "fold_index"),
            ("PyntBCI", "pyntbci_accuracy"),
            ("Rust exact", "rust_exact_accuracy"),
            ("Match", "rust_exact_match_rate"),
        ],
        summary_rows=rows,
    )
    render_rich_table(
        Console(),
        title="PyNTBCI Example 3 eTRCA",
        columns=[
            ("subject", "subject"),
            ("fold", "fold_index"),
            ("pyntbci", "pyntbci_accuracy"),
            ("rust_exact", "rust_exact_accuracy"),
            ("match", "rust_exact_match_rate"),
        ],
        rows=rows,
        formatters={
            "pyntbci_accuracy": lambda value: f"{value:.4f}",
            "rust_exact_accuracy": lambda value: f"{value:.4f}",
            "rust_exact_match_rate": lambda value: f"{value:.4f}",
        },
    )
