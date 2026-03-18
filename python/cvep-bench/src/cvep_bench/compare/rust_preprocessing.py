from __future__ import annotations

import argparse
import html
import json
import subprocess
import tempfile
from pathlib import Path
from typing import Any

import numpy as np
from scipy import signal

from cvep_bench.runtime.cargo import WORKSPACE_ROOT


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
    with tempfile.TemporaryDirectory(prefix="cvep-preproc-") as tmpdir:
        fixture_path = Path(tmpdir) / "fixture.json"
        fixture_path.write_text(json.dumps(fixture), encoding="utf-8")
        result = subprocess.run(
            [
                "cargo",
                "run",
                "-q",
                "-p",
                "cvep-decoder",
                "--bin",
                "preprocessing_fixture",
                "--",
                str(fixture_path),
            ],
            cwd=WORKSPACE_ROOT,
            check=True,
            capture_output=True,
            text=True,
        )
    return np.asarray(json.loads(result.stdout)["filtered"], dtype=np.float64)


def compare_outputs(python: np.ndarray, rust: np.ndarray) -> dict[str, Any]:
    delta = rust - python
    abs_delta = np.abs(delta)
    return {
        "mae": float(np.mean(abs_delta)),
        "rmse": float(np.sqrt(np.mean(delta**2))),
        "max_abs_error": float(np.max(abs_delta)),
        "channel_metrics": [
            {
                "channel": ch,
                "mae": float(np.mean(abs_delta[:, ch])),
                "rmse": float(np.sqrt(np.mean(delta[:, ch] ** 2))),
                "max_abs_error": float(np.max(abs_delta[:, ch])),
                "python_std": float(np.std(python[:, ch])),
                "rust_std": float(np.std(rust[:, ch])),
            }
            for ch in range(python.shape[1])
        ],
        "first_frame_python": python[0].tolist(),
        "first_frame_rust": rust[0].tolist(),
        "last_frame_python": python[-1].tolist(),
        "last_frame_rust": rust[-1].tolist(),
    }


def render_html(output: Path, payload: dict[str, Any]) -> None:
    channel_rows = "\n".join(
        (
            "<tr>"
            f"<td>{row['channel']}</td><td>{row['mae']:.8f}</td><td>{row['rmse']:.8f}</td><td>{row['max_abs_error']:.8f}</td><td>{row['python_std']:.8f}</td><td>{row['rust_std']:.8f}</td>"
            "</tr>"
        )
        for row in payload["comparison"]["channel_metrics"]
    )
    output.write_text(
        f"<!doctype html><html lang='en'><body><pre>{html.escape(json.dumps(payload, indent=2))}</pre><table><tbody>{channel_rows}</tbody></table></body></html>",
        encoding="utf-8",
    )


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
    args.output_json.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    render_html(args.output_html, payload)
