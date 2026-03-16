#!/usr/bin/env python3
# /// script
# dependencies = [
#   "h5py",
#   "mne",
#   "numpy",
#   "pyntbci",
#   "scipy",
# ]
# ///
"""Compare packaged PyNTBCI Thielen2021 data against the local raw reconstruction."""

from __future__ import annotations

import argparse
import html
import importlib.util
import json
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable

import numpy as np


WORKSPACE_ROOT = Path(__file__).resolve().parents[3]
RAW_BENCHMARK_SCRIPT = WORKSPACE_ROOT / "crates/cvep-decoder/scripts/benchmark_pyntbci_vs_rust.py"


@dataclass
class SubjectPair:
    subject: int
    raw_x: np.ndarray
    raw_y: np.ndarray
    packaged_x: np.ndarray
    packaged_y: np.ndarray
    fs: int


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--subject",
        type=int,
        default=1,
        help="Thielen2021 subject number to compare.",
    )
    parser.add_argument(
        "--data-dir",
        type=Path,
        default=Path("crates/cvep-decoder/data"),
        help="Root containing the downloaded raw Thielen2021 dataset.",
    )
    parser.add_argument(
        "--target-fs",
        type=int,
        default=240,
        help="Resample rate for the raw reconstruction. Use 240 Hz to match the packaged data.",
    )
    parser.add_argument(
        "--trialtime",
        type=float,
        default=4.2,
        help="Trial window in seconds. Use 4.2 s to match the packaged example.",
    )
    parser.add_argument(
        "--output-json",
        type=Path,
        default=Path("crates/cvep-decoder/data/thielen2021_packaged_vs_raw.json"),
        help="Path for raw JSON output.",
    )
    parser.add_argument(
        "--output-html",
        type=Path,
        default=Path("crates/cvep-decoder/data/thielen2021_packaged_vs_raw.html"),
        help="Path for HTML output.",
    )
    return parser.parse_args()


def load_raw_benchmark_module() -> Any:
    spec = importlib.util.spec_from_file_location("cvep_raw_benchmark", RAW_BENCHMARK_SCRIPT)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"Failed to load benchmark module from {RAW_BENCHMARK_SCRIPT}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def load_subject_pair(subject: int, data_dir: Path, target_fs: int, trialtime: float) -> SubjectPair:
    benchmark = load_raw_benchmark_module()
    original_trial_seconds_for_dataset: Callable[[str], float] = benchmark.trial_seconds_for_dataset
    benchmark.trial_seconds_for_dataset = (
        lambda dataset: trialtime if dataset == "Thielen2021" else original_trial_seconds_for_dataset(dataset)
    )
    raw = benchmark.load_thielen2021_subject(subject, data_dir, target_fs)

    import pyntbci

    packaged_path = (
        Path(pyntbci.__file__).resolve().parent
        / "data"
        / f"thielen2021_sub-{subject:02d}.npz"
    )
    packaged = np.load(packaged_path)
    n_samples = int(round(trialtime * target_fs))
    packaged_x = np.asarray(packaged["X"], dtype=np.float64)[:, :, :n_samples]
    packaged_y = np.asarray(packaged["y"], dtype=np.int64)
    return SubjectPair(
        subject=subject,
        raw_x=np.asarray(raw.x, dtype=np.float64),
        raw_y=np.asarray(raw.y, dtype=np.int64),
        packaged_x=packaged_x,
        packaged_y=packaged_y,
        fs=target_fs,
    )


def mean_trial_correlation(lhs: np.ndarray, rhs: np.ndarray) -> dict[str, float | list[float]]:
    corrs = []
    for idx in range(lhs.shape[0]):
        a = lhs[idx].reshape(-1)
        b = rhs[idx].reshape(-1)
        a = a - a.mean()
        b = b - b.mean()
        denom = np.linalg.norm(a) * np.linalg.norm(b)
        corrs.append(float(a.dot(b) / denom) if denom else 0.0)
    return {
        "mean": float(np.mean(corrs)),
        "min": float(np.min(corrs)),
        "max": float(np.max(corrs)),
        "first10": [float(value) for value in corrs[:10]],
    }


def channel_correlation_matrix(lhs: np.ndarray, rhs: np.ndarray) -> np.ndarray:
    lhs_flat = lhs.transpose(1, 0, 2).reshape(lhs.shape[1], -1)
    rhs_flat = rhs.transpose(1, 0, 2).reshape(rhs.shape[1], -1)
    lhs_flat -= lhs_flat.mean(axis=1, keepdims=True)
    rhs_flat -= rhs_flat.mean(axis=1, keepdims=True)

    out = np.zeros((lhs.shape[1], rhs.shape[1]), dtype=np.float64)
    for i in range(lhs.shape[1]):
        for j in range(rhs.shape[1]):
            denom = np.linalg.norm(lhs_flat[i]) * np.linalg.norm(rhs_flat[j])
            out[i, j] = lhs_flat[i].dot(rhs_flat[j]) / denom if denom else 0.0
    return out


def average_trace_best_lags(lhs: np.ndarray, rhs: np.ndarray, max_lag: int) -> list[dict[str, float | int]]:
    out = []
    for ch in range(lhs.shape[1]):
        a = lhs[:, ch, :].mean(axis=0)
        b = rhs[:, ch, :].mean(axis=0)
        a = a - a.mean()
        b = b - b.mean()
        best_lag = 0
        best_corr = 0.0
        for lag in range(-max_lag, max_lag + 1):
            if lag < 0:
                aa = a[-lag:]
                bb = b[: len(aa)]
            elif lag > 0:
                aa = a[:-lag]
                bb = b[lag:]
            else:
                aa = a
                bb = b
            denom = np.linalg.norm(aa) * np.linalg.norm(bb)
            corr = float(aa.dot(bb) / denom) if denom else 0.0
            if abs(corr) > abs(best_corr):
                best_lag = lag
                best_corr = corr
        out.append({"channel": ch, "lag_samples": best_lag, "corr": best_corr})
    return out


def etrca_cv_accuracy(x: np.ndarray, y: np.ndarray, fs: int, cycle_size: float, folds: int = 5) -> float:
    import pyntbci

    fold_ids = np.repeat(np.arange(folds), x.shape[0] // folds)
    accuracy = []
    for fold in range(folds):
        train = fold_ids != fold
        test = ~train
        model = pyntbci.classifiers.eTRCA(
            lags=None,
            fs=fs,
            cycle_size=cycle_size,
            ensemble=True,
        )
        model.fit(x[train], y[train])
        prediction = np.asarray(model.predict(x[test]), dtype=np.int64)
        accuracy.append(float(np.mean(prediction == y[test])))
    return float(np.mean(accuracy))


def trial_demean(x: np.ndarray) -> np.ndarray:
    return x - x.mean(axis=2, keepdims=True)


def summarize_variant(name: str, x: np.ndarray, packaged_x: np.ndarray, y: np.ndarray, fs: int) -> dict[str, Any]:
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


def render_html(output: Path, payload: dict[str, Any]) -> None:
    summary_rows = "\n".join(
        (
            "<tr>"
            f"<td>{html.escape(row['name'])}</td>"
            f"<td>{row['overall_mean']:.8f}</td>"
            f"<td>{row['overall_std']:.8f}</td>"
            f"<td>{row['mean_trial_corr']:.4f}</td>"
            f"<td>{row['etrca_accuracy']:.4f}</td>"
            "</tr>"
        )
        for row in payload["variants"]
    )
    channel_rows = "\n".join(
        "<tr>" + "".join(f"<td>{value:.4f}</td>" for value in row) + "</tr>"
        for row in payload["channel_correlation_matrix"]
    )
    lag_rows = "\n".join(
        (
            "<tr>"
            f"<td>{entry['channel']}</td>"
            f"<td>{entry['lag_samples']}</td>"
            f"<td>{entry['corr']:.4f}</td>"
            "</tr>"
        )
        for entry in payload["average_trace_best_lags"]
    )
    pre = html.escape(json.dumps(payload["config"], indent=2))
    document = f"""<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Thielen2021 packaged vs raw</title>
  <style>
    :root {{
      color-scheme: light;
      --bg: #f6f2e8;
      --panel: #fffdf9;
      --ink: #1f2933;
      --line: #d8cfbf;
      --accent: #0d5c63;
    }}
    body {{ margin: 0; background: var(--bg); color: var(--ink); font-family: Georgia, serif; }}
    main {{ max-width: 1200px; margin: 0 auto; padding: 24px 16px 40px; }}
    .card {{ background: var(--panel); border: 1px solid var(--line); border-radius: 16px; padding: 18px; margin-bottom: 16px; }}
    table {{ width: 100%; border-collapse: collapse; font-family: Menlo, monospace; font-size: 0.88rem; }}
    th, td {{ padding: 10px 12px; border-bottom: 1px solid var(--line); text-align: left; }}
    th {{ background: #faf5ec; color: var(--accent); }}
    pre {{ margin: 0; overflow: auto; background: #fbf8f1; border: 1px solid var(--line); border-radius: 12px; padding: 14px; }}
    .table-wrap {{ overflow: auto; border: 1px solid var(--line); border-radius: 12px; }}
  </style>
</head>
<body>
  <main>
    <section class="card">
      <h1>Thielen2021 packaged vs raw</h1>
      <p>This report compares the packaged PyNTBCI example tensor against the local raw-data reconstruction for the same subject.</p>
    </section>
    <section class="card">
      <h2>Variants</h2>
      <div class="table-wrap">
        <table>
          <thead><tr><th>Variant</th><th>Mean</th><th>Std</th><th>Mean trial corr vs packaged</th><th>eTRCA accuracy</th></tr></thead>
          <tbody>{summary_rows}</tbody>
        </table>
      </div>
    </section>
    <section class="card">
      <h2>Channel Correlation Matrix</h2>
      <div class="table-wrap">
        <table><tbody>{channel_rows}</tbody></table>
      </div>
    </section>
    <section class="card">
      <h2>Average Trace Best Lags</h2>
      <div class="table-wrap">
        <table>
          <thead><tr><th>Channel</th><th>Lag [samples]</th><th>Peak corr</th></tr></thead>
          <tbody>{lag_rows}</tbody>
        </table>
      </div>
    </section>
    <section class="card">
      <h2>Config</h2>
      <pre>{pre}</pre>
    </section>
  </main>
</body>
</html>
"""
    output.write_text(document, encoding="utf-8")


def main() -> None:
    args = parse_args()
    pair = load_subject_pair(args.subject, args.data_dir, args.target_fs, args.trialtime)
    if pair.raw_x.shape != pair.packaged_x.shape:
        raise ValueError(f"Shape mismatch: raw={pair.raw_x.shape} packaged={pair.packaged_x.shape}")

    variants = [
        summarize_variant("packaged", pair.packaged_x, pair.packaged_x, pair.packaged_y, pair.fs),
        summarize_variant("raw", pair.raw_x, pair.packaged_x, pair.raw_y, pair.fs),
        summarize_variant("raw_trial_demean", trial_demean(pair.raw_x), pair.packaged_x, pair.raw_y, pair.fs),
    ]
    matrix = channel_correlation_matrix(pair.raw_x, pair.packaged_x)
    payload = {
        "config": {
            "subject": args.subject,
            "target_fs": args.target_fs,
            "trialtime": args.trialtime,
            "shape": list(pair.raw_x.shape),
            "label_exact_match": float(np.mean(pair.raw_y == pair.packaged_y)),
            "raw_labels_first20": pair.raw_y[:20].astype(int).tolist(),
            "packaged_labels_first20": pair.packaged_y[:20].astype(int).tolist(),
        },
        "variants": variants,
        "channel_correlation_matrix": matrix.tolist(),
        "channel_best_packaged_match": matrix.argmax(axis=1).astype(int).tolist(),
        "channel_diagonal_mean_corr": float(np.mean(np.diag(matrix))),
        "average_trace_best_lags": average_trace_best_lags(pair.raw_x, pair.packaged_x, max_lag=pair.fs // 2),
    }

    args.output_json.parent.mkdir(parents=True, exist_ok=True)
    args.output_html.parent.mkdir(parents=True, exist_ok=True)
    args.output_json.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    render_html(args.output_html, payload)
    print(f"wrote {args.output_json}")
    print(f"wrote {args.output_html}")


if __name__ == "__main__":
    main()
