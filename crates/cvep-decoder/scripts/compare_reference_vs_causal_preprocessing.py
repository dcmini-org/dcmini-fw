#!/usr/bin/env python3
# /// script
# dependencies = [
#   "h5py",
#   "matplotlib",
#   "mne",
#   "numpy",
#   "pyntbci",
#   "rich",
#   "scipy",
# ]
# ///
"""Plot and compare reference vs causal preprocessing on the same dataset slice."""

from __future__ import annotations

import argparse
import base64
import html
import importlib.util
import io
import json
import os
import sys
import tempfile
from pathlib import Path
from typing import Any, Callable

os.environ.setdefault("MNE_DONTWRITE_HOME", "true")
os.environ.setdefault("MNE_HOME", str(Path(tempfile.gettempdir()) / "mne-home"))
os.environ.setdefault(
    "MPLCONFIGDIR",
    str(Path(tempfile.gettempdir()) / "matplotlib-cache"),
)

import matplotlib

matplotlib.use("Agg")

import matplotlib.pyplot as plt
import numpy as np


WORKSPACE_ROOT = Path(__file__).resolve().parents[3]
RAW_BENCHMARK_SCRIPT = (
    WORKSPACE_ROOT / "crates/cvep-decoder/scripts/benchmark_pyntbci_vs_rust.py"
)
CAUSAL_BENCHMARK_SCRIPT = (
    WORKSPACE_ROOT
    / "crates/cvep-decoder/scripts/benchmark_causal_preprocessing_vs_reference.py"
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--dataset",
        type=str,
        default="Thielen2021",
        help="Dataset name to compare.",
    )
    parser.add_argument(
        "--subject",
        type=int,
        default=1,
        help="Subject index to compare.",
    )
    parser.add_argument(
        "--data-dir",
        type=Path,
        default=WORKSPACE_ROOT / "crates/cvep-decoder/data",
        help="Root containing the downloaded datasets.",
    )
    parser.add_argument(
        "--target-fs",
        type=int,
        default=250,
        help="Target sample rate used by both preprocessing paths.",
    )
    parser.add_argument(
        "--trial-seconds",
        type=float,
        default=4.2,
        help="Optional trial length override for comparison plots.",
    )
    parser.add_argument(
        "--plot-seconds",
        type=float,
        default=2.0,
        help="How much of the trial to display in overlay plots.",
    )
    parser.add_argument(
        "--band-low",
        type=float,
        default=1.0,
        help="Causal band-pass lower cutoff in Hz.",
    )
    parser.add_argument(
        "--band-high",
        type=float,
        default=65.0,
        help="Causal band-pass upper cutoff in Hz.",
    )
    parser.add_argument(
        "--band-order",
        type=int,
        default=4,
        help="Causal Butterworth band-pass order.",
    )
    parser.add_argument(
        "--notch-q",
        type=float,
        default=30.0,
        help="Q factor for each causal notch section.",
    )
    parser.add_argument(
        "--output-json",
        type=Path,
        default=WORKSPACE_ROOT / "crates/cvep-decoder/data/reference_vs_causal_preprocessing.json",
        help="Path for raw JSON output.",
    )
    parser.add_argument(
        "--output-html",
        type=Path,
        default=WORKSPACE_ROOT / "crates/cvep-decoder/data/reference_vs_causal_preprocessing.html",
        help="Path for HTML output.",
    )
    return parser.parse_args()


def load_module(path: Path, name: str) -> Any:
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"Failed to load module from {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def patch_trial_seconds(module: Any, dataset: str, trial_seconds: float) -> Callable[[], None]:
    original = module.trial_seconds_for_dataset
    module.trial_seconds_for_dataset = (
        lambda ds: trial_seconds if ds == dataset else original(ds)
    )

    def restore() -> None:
        module.trial_seconds_for_dataset = original

    return restore


def load_reference_and_causal(args: argparse.Namespace) -> tuple[Any, Any]:
    benchmark = load_module(RAW_BENCHMARK_SCRIPT, "cvep_raw_benchmark_compare")
    causal_mod = load_module(CAUSAL_BENCHMARK_SCRIPT, "cvep_causal_benchmark_compare")

    restore = patch_trial_seconds(benchmark, args.dataset, args.trial_seconds)
    try:
        reference = benchmark.load_subject(
            args.dataset,
            args.subject,
            args.data_dir,
            args.target_fs,
        )
        with causal_mod.causal_loader_patch(
            benchmark,
            band_low=args.band_low,
            band_high=args.band_high,
            band_order=args.band_order,
            notch_q=args.notch_q,
        ):
            causal = benchmark.load_subject(
                args.dataset,
                args.subject,
                args.data_dir,
                args.target_fs,
            )
    finally:
        restore()

    return reference, causal


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


def etrca_cv_accuracy(x: np.ndarray, y: np.ndarray, fs: int, cycle_size: float | None, folds: int = 5) -> float:
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


def encode_plot(fig: plt.Figure) -> str:
    buf = io.BytesIO()
    fig.savefig(buf, format="png", dpi=160, bbox_inches="tight")
    plt.close(fig)
    return base64.b64encode(buf.getvalue()).decode("ascii")


def overlay_plot(reference_x: np.ndarray, causal_x: np.ndarray, fs: int, seconds: float) -> str:
    samples = min(reference_x.shape[2], int(round(seconds * fs)))
    time = np.arange(samples) / fs
    fig, axes = plt.subplots(4, 1, figsize=(10, 7), sharex=True)
    trial_idx = 0
    for ch, ax in enumerate(axes):
        ax.plot(time, reference_x[trial_idx, ch, :samples], label="reference", linewidth=1.4)
        ax.plot(time, causal_x[trial_idx, ch, :samples], label="causal", linewidth=1.1, alpha=0.85)
        ax.set_ylabel(f"Ch {ch}")
        ax.grid(True, alpha=0.2)
    axes[0].legend(loc="upper right")
    axes[-1].set_xlabel("Time (s)")
    fig.suptitle("First trial overlay")
    return encode_plot(fig)


def average_trace_plot(reference_x: np.ndarray, causal_x: np.ndarray, fs: int, seconds: float) -> str:
    samples = min(reference_x.shape[2], int(round(seconds * fs)))
    time = np.arange(samples) / fs
    fig, axes = plt.subplots(2, 1, figsize=(10, 6), sharex=True)
    ref = reference_x[:, 0, :samples].mean(axis=0)
    cau = causal_x[:, 0, :samples].mean(axis=0)
    axes[0].plot(time, ref, label="reference", linewidth=1.5)
    axes[0].plot(time, cau, label="causal", linewidth=1.2)
    axes[0].set_ylabel("Mean trace")
    axes[0].legend(loc="upper right")
    axes[0].grid(True, alpha=0.2)
    axes[1].plot(time, cau - ref, color="#8c4c12", linewidth=1.2)
    axes[1].set_ylabel("Delta")
    axes[1].set_xlabel("Time (s)")
    axes[1].grid(True, alpha=0.2)
    fig.suptitle("Channel 0 average trace and delta")
    return encode_plot(fig)


def lag_summary_plot(lag_rows: list[dict[str, float | int]]) -> str:
    channels = [row["channel"] for row in lag_rows]
    lags = [row["lag_samples"] for row in lag_rows]
    corrs = [row["corr"] for row in lag_rows]
    fig, ax1 = plt.subplots(figsize=(10, 4.8))
    ax1.bar(channels, lags, color="#c08457", alpha=0.8)
    ax1.set_xlabel("Channel")
    ax1.set_ylabel("Best lag (samples)")
    ax1.grid(True, axis="y", alpha=0.2)
    ax2 = ax1.twinx()
    ax2.plot(channels, corrs, color="#0f766e", marker="o", linewidth=1.4)
    ax2.set_ylabel("Correlation at best lag")
    fig.suptitle("Average-trace lag summary")
    return encode_plot(fig)


def summarize_variant(
    name: str,
    x: np.ndarray,
    other_x: np.ndarray,
    y: np.ndarray,
    fs: int,
    cycle_size: float | None,
) -> dict[str, Any]:
    stats = mean_trial_correlation(x, other_x)
    return {
        "name": name,
        "overall_mean": float(x.mean()),
        "overall_std": float(x.std()),
        "mean_trial_corr_to_other": stats["mean"],
        "min_trial_corr_to_other": stats["min"],
        "max_trial_corr_to_other": stats["max"],
        "first10_trial_corr_to_other": stats["first10"],
        "etrca_accuracy": etrca_cv_accuracy(x, y, fs=fs, cycle_size=cycle_size),
    }


def render_html(output: Path, payload: dict[str, Any]) -> None:
    summary_rows = "\n".join(
        (
            "<tr>"
            f"<td>{html.escape(row['name'])}</td>"
            f"<td>{row['overall_mean']:.8f}</td>"
            f"<td>{row['overall_std']:.8f}</td>"
            f"<td>{row['mean_trial_corr_to_other']:.4f}</td>"
            f"<td>{row['etrca_accuracy']:.4f}</td>"
            "</tr>"
        )
        for row in payload["variants"]
    )
    lag_rows = "\n".join(
        (
            "<tr>"
            f"<td>{row['channel']}</td>"
            f"<td>{row['lag_samples']}</td>"
            f"<td>{row['corr']:.4f}</td>"
            "</tr>"
        )
        for row in payload["average_trace_best_lags"]
    )
    pre = html.escape(json.dumps(payload["config"], indent=2))
    document = f"""<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Reference vs causal preprocessing</title>
  <style>
    :root {{
      color-scheme: light;
      --bg: #f6f2e8;
      --panel: #fffdf9;
      --ink: #1f2933;
      --line: #d8cfbf;
      --accent: #0d5c63;
    }}
    body {{
      margin: 0;
      background: radial-gradient(circle at top, #efe0c2 0%, var(--bg) 42%, #faf8f3 100%);
      color: var(--ink);
      font-family: Cambria, Georgia, serif;
    }}
    main {{
      max-width: 1180px;
      margin: 0 auto;
      padding: 28px 18px 56px;
    }}
    .card {{
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 18px;
      padding: 20px;
      box-shadow: 0 12px 36px rgba(52, 39, 18, 0.08);
      margin-bottom: 18px;
    }}
    .plot {{
      width: 100%;
      border: 1px solid var(--line);
      border-radius: 12px;
      background: #fff;
    }}
    .plot-grid {{
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
      gap: 16px;
    }}
    table {{
      width: 100%;
      border-collapse: collapse;
      font-family: "SFMono-Regular", Menlo, monospace;
      font-size: 0.88rem;
    }}
    th, td {{
      padding: 10px 12px;
      border-bottom: 1px solid var(--line);
      text-align: left;
    }}
    th {{
      background: #faf5ec;
      color: var(--accent);
    }}
    pre {{
      margin: 0;
      overflow: auto;
      background: #fbf8f1;
      border: 1px solid var(--line);
      border-radius: 12px;
      padding: 14px;
    }}
  </style>
</head>
<body>
  <main>
    <section class="card">
      <h1>Reference vs causal preprocessing</h1>
      <p>Direct comparison of the current benchmark preprocessing path against the causal SOS path on the same subject and trial extraction.</p>
    </section>
    <section class="card">
      <h2>Summary</h2>
      <table>
        <thead>
          <tr>
            <th>Variant</th>
            <th>Mean</th>
            <th>Std</th>
            <th>Mean trial corr to other</th>
            <th>eTRCA accuracy</th>
          </tr>
        </thead>
        <tbody>{summary_rows}</tbody>
      </table>
    </section>
    <section class="card">
      <h2>Plots</h2>
      <div class="plot-grid">
        <img class="plot" src="data:image/png;base64,{payload['plots']['overlay_png']}" alt="overlay plot">
        <img class="plot" src="data:image/png;base64,{payload['plots']['average_trace_png']}" alt="average trace plot">
        <img class="plot" src="data:image/png;base64,{payload['plots']['lag_summary_png']}" alt="lag summary plot">
      </div>
    </section>
    <section class="card">
      <h2>Average trace lag summary</h2>
      <table>
        <thead>
          <tr>
            <th>Channel</th>
            <th>Best lag (samples)</th>
            <th>Correlation</th>
          </tr>
        </thead>
        <tbody>{lag_rows}</tbody>
      </table>
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
    reference, causal = load_reference_and_causal(args)
    if not np.array_equal(reference.y, causal.y):
        raise ValueError("Label mismatch between reference and causal preprocessing")
    if reference.x.shape != causal.x.shape:
        raise ValueError(
            f"Shape mismatch between reference and causal preprocessing: "
            f"{reference.x.shape} vs {causal.x.shape}"
        )

    max_lag = min(64, reference.x.shape[2] // 8)
    payload = {
        "config": {
            "dataset": args.dataset,
            "subject": args.subject,
            "target_fs": args.target_fs,
            "trial_seconds": args.trial_seconds,
            "plot_seconds": args.plot_seconds,
            "band_low": args.band_low,
            "band_high": args.band_high,
            "band_order": args.band_order,
            "notch_q": args.notch_q,
            "shape": list(reference.x.shape),
            "cycle_size_reference": reference.cycle_size,
            "cycle_size_causal": causal.cycle_size,
        },
        "variants": [
            summarize_variant(
                "reference",
                np.asarray(reference.x, dtype=np.float64),
                np.asarray(causal.x, dtype=np.float64),
                np.asarray(reference.y, dtype=np.int64),
                fs=reference.fs,
                cycle_size=reference.cycle_size,
            ),
            summarize_variant(
                "causal",
                np.asarray(causal.x, dtype=np.float64),
                np.asarray(reference.x, dtype=np.float64),
                np.asarray(causal.y, dtype=np.int64),
                fs=causal.fs,
                cycle_size=causal.cycle_size,
            ),
        ],
        "mean_trial_correlation": mean_trial_correlation(
            np.asarray(reference.x, dtype=np.float64),
            np.asarray(causal.x, dtype=np.float64),
        ),
        "channel_correlation_matrix": channel_correlation_matrix(
            np.asarray(reference.x, dtype=np.float64),
            np.asarray(causal.x, dtype=np.float64),
        ).tolist(),
        "average_trace_best_lags": average_trace_best_lags(
            np.asarray(reference.x, dtype=np.float64),
            np.asarray(causal.x, dtype=np.float64),
            max_lag=max_lag,
        ),
        "plots": {
            "overlay_png": overlay_plot(reference.x, causal.x, reference.fs, args.plot_seconds),
            "average_trace_png": average_trace_plot(reference.x, causal.x, reference.fs, args.plot_seconds),
            "lag_summary_png": lag_summary_plot(
                average_trace_best_lags(
                    np.asarray(reference.x, dtype=np.float64),
                    np.asarray(causal.x, dtype=np.float64),
                    max_lag=max_lag,
                )
            ),
        },
    }

    args.output_json.parent.mkdir(parents=True, exist_ok=True)
    args.output_html.parent.mkdir(parents=True, exist_ok=True)
    args.output_json.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    render_html(args.output_html, payload)
    print(json.dumps({
        "mean_trial_corr": payload["mean_trial_correlation"]["mean"],
        "reference_etrca": payload["variants"][0]["etrca_accuracy"],
        "causal_etrca": payload["variants"][1]["etrca_accuracy"],
    }, indent=2))
    print(f"wrote {args.output_json}")
    print(f"wrote {args.output_html}")


if __name__ == "__main__":
    main()
