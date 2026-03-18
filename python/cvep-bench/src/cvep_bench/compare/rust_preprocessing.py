from __future__ import annotations

import argparse
import json
import subprocess
from pathlib import Path

import numpy as np
from scipy import signal

from cvep_bench.benchmarks.reporting import render_tabular_html, write_json_payload
from cvep_bench.compare.metrics import compare_outputs
from cvep_bench.runtime.binaries import WORKSPACE_ROOT
from cvep_bench.runtime.json_fixtures import temporary_fixture_path
from cvep_bench.runtime.runner import run_fixture_payload


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fs", type=float, default=250.0)
    parser.add_argument("--channels", type=int, default=8, choices=[8, 32, 64])
    parser.add_argument("--seconds", type=float, default=8.0)
    parser.add_argument("--seed", type=int, default=42)
    parser.add_argument("--band-low", type=float, default=1.0)
    parser.add_argument("--band-high", type=float, default=65.0)
    parser.add_argument("--band-order", type=int, default=4)
    parser.add_argument("--notch-q", type=float, default=30.0)
    parser.add_argument(
        "--output-json",
        type=Path,
        default=Path("crates/cvep-decoder/data/preprocessing_compare.json"),
    )
    parser.add_argument(
        "--output-html",
        type=Path,
        default=Path("crates/cvep-decoder/data/preprocessing_compare.html"),
    )
    return parser.parse_args()


def design_sos(
    fs: float, band_low: float, band_high: float, band_order: int, notch_q: float
) -> np.ndarray:
    if not 0.0 < band_low < band_high < fs / 2.0:
        raise ValueError(
            f"Expected 0 < band_low < band_high < Nyquist, got {band_low=}, {band_high=}, {fs=}"
        )
    sos_parts = [
        signal.butter(
            band_order, [band_low, band_high], btype="bandpass", output="sos", fs=fs
        )
    ]
    harmonic = 50.0
    while harmonic < fs / 2.0:
        b, a = signal.iirnotch(harmonic, notch_q, fs)
        sos_parts.append(signal.tf2sos(b, a))
        harmonic += 50.0
    return np.concatenate(sos_parts, axis=0).astype(np.float32)


def synthesize_signal(samples: int, channels: int, fs: float, seed: int) -> np.ndarray:
    rng = np.random.default_rng(seed)
    t = np.arange(samples, dtype=np.float64) / fs
    out = np.zeros((samples, channels), dtype=np.float32)
    for ch in range(channels):
        phase = 0.13 * (ch + 1)
        base = (
            40.0 * np.sin(2.0 * np.pi * 12.0 * t + phase)
            + 25.0 * np.sin(2.0 * np.pi * 21.0 * t + 0.7 * phase)
            + 10.0 * np.sin(2.0 * np.pi * 50.0 * t)
            + 6.0 * np.sin(2.0 * np.pi * 100.0 * t)
        )
        drift = 60.0 + 8.0 * ch + 5.0 * np.sin(2.0 * np.pi * 0.2 * t)
        noise = rng.normal(scale=3.0 + 0.25 * ch, size=samples)
        out[:, ch] = (base + drift + noise).astype(np.float32)
    return out


def run_rust_fixture(
    samples: np.ndarray, sos_rows: np.ndarray, channels: int
) -> np.ndarray:
    fixture = {
        "channels": channels,
        "sections": int(sos_rows.shape[0]),
        "sos_rows": sos_rows.astype(np.float32).tolist(),
        "samples": samples.astype(np.float32).tolist(),
    }
    binary = WORKSPACE_ROOT / "target" / "debug" / "preprocessing_fixture"
    if not binary.exists():
        subprocess.run(
            [
                "cargo",
                "build",
                "--quiet",
                "-p",
                "cvep-decoder",
                "--bin",
                "preprocessing_fixture",
            ],
            cwd=WORKSPACE_ROOT,
            check=True,
        )
    with temporary_fixture_path(prefix="cvep-preproc-") as fixture_path:
        payload = run_fixture_payload(binary, fixture, fixture_path=fixture_path)
    return np.asarray(payload["filtered"], dtype=np.float64)


def main() -> None:
    args = parse_args()
    samples = synthesize_signal(
        int(round(args.seconds * args.fs)), args.channels, args.fs, args.seed
    )
    sos = design_sos(
        args.fs, args.band_low, args.band_high, args.band_order, args.notch_q
    )
    python = signal.sosfilt(sos, samples, axis=0).astype(np.float64)
    rust = run_rust_fixture(samples, sos, args.channels)
    payload = {"config": vars(args), "comparison": compare_outputs(python, rust)}
    write_json_payload(args.output_json, payload)
    render_tabular_html(
        args.output_html,
        title="Rust Preprocessing Comparison",
        subtitle="Python vs Rust SOS filter output comparison.",
        config=vars(args),
        summary_columns=[
            ("Channel", "channel"),
            ("MAE", "mae"),
            ("RMSE", "rmse"),
            ("Max Abs Error", "max_abs_error"),
            ("Python Std", "python_std"),
            ("Rust Std", "rust_std"),
        ],
        summary_rows=payload["comparison"]["channel_metrics"],
    )
