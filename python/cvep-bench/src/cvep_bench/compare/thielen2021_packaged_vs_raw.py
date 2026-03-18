from __future__ import annotations

import argparse
from pathlib import Path
from typing import Any

import numpy as np

from cvep_bench.algorithms.pyntbci_models import fit_etrca
from cvep_bench.benchmarks.reporting import render_tabular_html, write_json_payload
from cvep_bench.compare.metrics import mean_trial_correlation
from cvep_bench.datasets.loaders import load_thielen2021_subject


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--subject", type=int, default=1)
    parser.add_argument(
        "--data-dir", type=Path, default=Path("crates/cvep-decoder/data")
    )
    parser.add_argument("--target-fs", type=int, default=240)
    parser.add_argument("--trialtime", type=float, default=4.2)
    parser.add_argument(
        "--output-json",
        type=Path,
        default=Path("crates/cvep-decoder/data/thielen2021_packaged_vs_raw.json"),
    )
    parser.add_argument(
        "--output-html",
        type=Path,
        default=Path("crates/cvep-decoder/data/thielen2021_packaged_vs_raw.html"),
    )
    return parser.parse_args()


def etrca_cv_accuracy(
    x: np.ndarray, y: np.ndarray, fs: int, cycle_size: float, folds: int = 5
) -> float:
    fold_ids = np.repeat(np.arange(folds), x.shape[0] // folds)
    accuracy = []
    for fold in range(folds):
        train = fold_ids != fold
        test = ~train
        model = fit_etrca(x[train], y[train], fs=fs, cycle_size=cycle_size)
        prediction = np.asarray(model.predict(x[test]), dtype=np.int64)
        accuracy.append(float(np.mean(prediction == y[test])))
    return float(np.mean(accuracy))


def trial_demean(x: np.ndarray) -> np.ndarray:
    return x - x.mean(axis=2, keepdims=True)


def summarize_variant(
    name: str, x: np.ndarray, packaged_x: np.ndarray, y: np.ndarray, fs: int
) -> dict[str, Any]:
    stats = mean_trial_correlation(x, packaged_x)
    return {
        "name": name,
        "overall_std": float(x.std()),
        "overall_mean": float(x.mean()),
        "mean_trial_corr": stats["mean"],
        "min_trial_corr": stats["min"],
        "max_trial_corr": stats["max"],
        "first10_trial_corr": stats["first10"],
        "etrca_accuracy": etrca_cv_accuracy(x, y, fs=fs, cycle_size=2.1),
    }


def main() -> None:
    args = parse_args()
    raw = load_thielen2021_subject(
        args.subject, args.data_dir, args.target_fs, trial_seconds=args.trialtime
    )
    import pyntbci

    if pyntbci.__file__ is None:
        raise RuntimeError("pyntbci package path is unavailable")

    packaged_path = (
        Path(pyntbci.__file__).resolve().parent
        / "data"
        / f"thielen2021_sub-{args.subject:02d}.npz"
    )
    packaged = np.load(packaged_path)
    packaged_x = np.asarray(packaged["X"], dtype=np.float64)[
        :, :, : int(round(args.trialtime * args.target_fs))
    ]
    packaged_y = np.asarray(packaged["y"], dtype=np.int64)
    payload = {
        "subject": args.subject,
        "fs": args.target_fs,
        "trialtime": args.trialtime,
        "variants": [
            summarize_variant(
                "packaged", packaged_x, packaged_x, packaged_y, args.target_fs
            ),
            summarize_variant(
                "raw",
                np.asarray(raw.x, dtype=np.float64),
                packaged_x,
                np.asarray(raw.y, dtype=np.int64),
                args.target_fs,
            ),
            summarize_variant(
                "raw_trial_demean",
                trial_demean(np.asarray(raw.x, dtype=np.float64)),
                packaged_x,
                np.asarray(raw.y, dtype=np.int64),
                args.target_fs,
            ),
        ],
        "label_exact_match": float(
            np.mean(np.asarray(raw.y, dtype=np.int64) == packaged_y)
        ),
    }
    write_json_payload(args.output_json, payload)
    render_tabular_html(
        args.output_html,
        title="Thielen2021 Packaged vs Raw",
        subtitle="Packaged-vs-raw parity diagnostics.",
        config={
            "subject": args.subject,
            "fs": args.target_fs,
            "trialtime": args.trialtime,
        },
        summary_columns=[
            ("Variant", "name"),
            ("Mean", "overall_mean"),
            ("Std", "overall_std"),
            ("Mean Trial Corr", "mean_trial_corr"),
            ("eTRCA", "etrca_accuracy"),
        ],
        summary_rows=payload["variants"],
    )
