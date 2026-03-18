from __future__ import annotations

import argparse
import base64
import html
import io
import json
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np

from cvep_bench.benchmarks import pyntbci_vs_rust as benchmark
from cvep_bench.benchmarks.causal_preprocessing import causal_loader_patch
from cvep_bench.benchmarks.pyntbci_vs_rust import DEFAULT_DATA_DIR


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dataset", type=str, default="Thielen2021")
    parser.add_argument("--subject", type=int, default=1)
    parser.add_argument("--data-dir", type=Path, default=DEFAULT_DATA_DIR)
    parser.add_argument("--target-fs", type=int, default=250)
    parser.add_argument("--trial-seconds", type=float, default=4.2)
    parser.add_argument("--plot-seconds", type=float, default=2.0)
    parser.add_argument("--band-low", type=float, default=1.0)
    parser.add_argument("--band-high", type=float, default=65.0)
    parser.add_argument("--band-order", type=int, default=4)
    parser.add_argument("--notch-q", type=float, default=30.0)
    parser.add_argument(
        "--output-json",
        type=Path,
        default=DEFAULT_DATA_DIR / "reference_vs_causal_preprocessing.json",
    )
    parser.add_argument(
        "--output-html",
        type=Path,
        default=DEFAULT_DATA_DIR / "reference_vs_causal_preprocessing.html",
    )
    return parser.parse_args()


def mean_trial_correlation(
    lhs: np.ndarray, rhs: np.ndarray
) -> dict[str, float | list[float]]:
    corrs = []
    for idx in range(lhs.shape[0]):
        a = lhs[idx].reshape(-1) - lhs[idx].mean()
        b = rhs[idx].reshape(-1) - rhs[idx].mean()
        denom = np.linalg.norm(a) * np.linalg.norm(b)
        corrs.append(float(a.dot(b) / denom) if denom else 0.0)
    return {
        "mean": float(np.mean(corrs)),
        "min": float(np.min(corrs)),
        "max": float(np.max(corrs)),
        "first10": [float(v) for v in corrs[:10]],
    }


def plot_overlay(
    reference: np.ndarray, causal: np.ndarray, fs: int, seconds: float
) -> str:
    samples = min(reference.shape[2], int(round(seconds * fs)))
    time = np.arange(samples) / fs
    fig, axes = plt.subplots(
        reference.shape[1], 1, figsize=(10, 1.5 * reference.shape[1]), sharex=True
    )
    axes = np.atleast_1d(axes)
    for ch, ax in enumerate(axes):
        ax.plot(time, reference[0, ch, :samples], label="reference", linewidth=1.0)
        ax.plot(time, causal[0, ch, :samples], label="causal", linewidth=1.0, alpha=0.8)
        ax.set_ylabel(f"ch{ch}")
    axes[0].legend(loc="upper right")
    axes[-1].set_xlabel("seconds")
    buf = io.BytesIO()
    fig.tight_layout()
    fig.savefig(buf, format="png", dpi=150)
    plt.close(fig)
    return base64.b64encode(buf.getvalue()).decode("ascii")


def render_html(output: Path, payload: dict) -> None:
    output.write_text(
        f"<!doctype html><html lang='en'><body><pre>{html.escape(json.dumps(payload, indent=2))}</pre><img src='data:image/png;base64,{payload['overlay_png_base64']}' /></body></html>",
        encoding="utf-8",
    )


def main() -> None:
    args = parse_args()
    original = benchmark.trial_seconds_for_dataset
    benchmark.trial_seconds_for_dataset = lambda ds: (
        args.trial_seconds if ds == args.dataset else original(ds)
    )
    try:
        reference = benchmark.load_subject(
            args.dataset, args.subject, args.data_dir, args.target_fs
        )
        with causal_loader_patch(
            args.band_low, args.band_high, args.band_order, args.notch_q
        ):
            causal = benchmark.load_subject(
                args.dataset, args.subject, args.data_dir, args.target_fs
            )
    finally:
        benchmark.trial_seconds_for_dataset = original
    payload = {
        "dataset": args.dataset,
        "subject": args.subject,
        "target_fs": args.target_fs,
        "trial_seconds": args.trial_seconds,
        "correlation": mean_trial_correlation(reference.x, causal.x),
        "overlay_png_base64": plot_overlay(
            reference.x, causal.x, args.target_fs, args.plot_seconds
        ),
    }
    args.output_json.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    render_html(args.output_html, payload)
