from __future__ import annotations

import argparse
import base64
import io
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np

from cvep_bench.benchmarks.causal_preprocessing import causal_loader_patch
from cvep_bench.benchmarks.pyntbci_vs_rust import DEFAULT_DATA_DIR
from cvep_bench.benchmarks.reporting import render_tabular_html, write_json_payload
from cvep_bench.compare.metrics import mean_trial_correlation
from cvep_bench.datasets.loaders import load_subject


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


def main() -> None:
    args = parse_args()
    import cvep_bench.datasets.loaders as loaders

    original_fn = loaders.trial_seconds_for_dataset
    loaders.trial_seconds_for_dataset = lambda ds: (
        args.trial_seconds if ds == args.dataset else original_fn(ds)
    )
    try:
        reference = load_subject(
            args.dataset, args.subject, args.data_dir, args.target_fs
        )
        with causal_loader_patch(
            args.band_low, args.band_high, args.band_order, args.notch_q
        ):
            causal = load_subject(
                args.dataset, args.subject, args.data_dir, args.target_fs
            )
    finally:
        loaders.trial_seconds_for_dataset = original_fn
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
    write_json_payload(args.output_json, payload)
    render_tabular_html(
        args.output_html,
        title="Reference vs Causal Preprocessing",
        subtitle="Waveform-level comparison between reference and causal preprocessing paths.",
        config={
            "dataset": args.dataset,
            "subject": args.subject,
            "target_fs": args.target_fs,
            "trial_seconds": args.trial_seconds,
        },
        summary_columns=[
            ("Mean Corr", "mean"),
            ("Min Corr", "min"),
            ("Max Corr", "max"),
        ],
        summary_rows=[payload["correlation"]],
    )
    html_path = args.output_html
    html_path.write_text(
        html_path.read_text(encoding="utf-8").replace(
            "</main>",
            f"<div class='card'><h2>Overlay</h2><img src='data:image/png;base64,{payload['overlay_png_base64']}' /></div></main>",
        ),
        encoding="utf-8",
    )
