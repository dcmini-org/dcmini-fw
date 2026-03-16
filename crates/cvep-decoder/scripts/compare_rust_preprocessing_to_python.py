#!/usr/bin/env python3
# /// script
# dependencies = [
#   "numpy",
#   "scipy",
# ]
# ///
"""Compare Rust causal SOS preprocessing against SciPy on the same signal."""

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


WORKSPACE_ROOT = Path(__file__).resolve().parents[3]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--fs",
        type=float,
        default=250.0,
        help="Sampling frequency used for the synthetic signal and filter design.",
    )
    parser.add_argument(
        "--channels",
        type=int,
        default=8,
        choices=[8, 32, 64],
        help="Channel count to compare. Must match a compiled Rust fixture shape.",
    )
    parser.add_argument(
        "--seconds",
        type=float,
        default=8.0,
        help="Duration of the synthetic signal in seconds.",
    )
    parser.add_argument(
        "--seed",
        type=int,
        default=42,
        help="Random seed for reproducible noise.",
    )
    parser.add_argument(
        "--band-low",
        type=float,
        default=1.0,
        help="Band-pass lower cutoff in Hz.",
    )
    parser.add_argument(
        "--band-high",
        type=float,
        default=65.0,
        help="Band-pass upper cutoff in Hz.",
    )
    parser.add_argument(
        "--band-order",
        type=int,
        default=4,
        help="Butterworth band-pass order.",
    )
    parser.add_argument(
        "--notch-q",
        type=float,
        default=30.0,
        help="Q factor for notch sections.",
    )
    parser.add_argument(
        "--output-json",
        type=Path,
        default=Path("crates/cvep-decoder/data/preprocessing_compare.json"),
        help="Path for raw JSON output.",
    )
    parser.add_argument(
        "--output-html",
        type=Path,
        default=Path("crates/cvep-decoder/data/preprocessing_compare.html"),
        help="Path for HTML output.",
    )
    return parser.parse_args()


def design_sos(
    fs: float,
    band_low: float,
    band_high: float,
    band_order: int,
    notch_q: float,
) -> np.ndarray:
    if not 0.0 < band_low < band_high < fs / 2.0:
        raise ValueError(
            f"Expected 0 < band_low < band_high < Nyquist, got {band_low=}, {band_high=}, {fs=}"
        )

    sos_parts = [
        signal.butter(
            band_order,
            [band_low, band_high],
            btype="bandpass",
            output="sos",
            fs=fs,
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
    samples: np.ndarray,
    sos_rows: np.ndarray,
    channels: int,
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
    payload = json.loads(result.stdout)
    return np.asarray(payload["filtered"], dtype=np.float64)


def compare_outputs(python: np.ndarray, rust: np.ndarray) -> dict[str, Any]:
    delta = rust - python
    abs_delta = np.abs(delta)
    channel_metrics = []
    for ch in range(python.shape[1]):
        channel_metrics.append(
            {
                "channel": ch,
                "mae": float(np.mean(abs_delta[:, ch])),
                "rmse": float(np.sqrt(np.mean(delta[:, ch] ** 2))),
                "max_abs_error": float(np.max(abs_delta[:, ch])),
                "python_std": float(np.std(python[:, ch])),
                "rust_std": float(np.std(rust[:, ch])),
            }
        )
    return {
        "mae": float(np.mean(abs_delta)),
        "rmse": float(np.sqrt(np.mean(delta**2))),
        "max_abs_error": float(np.max(abs_delta)),
        "channel_metrics": channel_metrics,
        "first_frame_python": python[0].tolist(),
        "first_frame_rust": rust[0].tolist(),
        "last_frame_python": python[-1].tolist(),
        "last_frame_rust": rust[-1].tolist(),
    }


def render_html(output: Path, payload: dict[str, Any]) -> None:
    channel_rows = "\n".join(
        (
            "<tr>"
            f"<td>{row['channel']}</td>"
            f"<td>{row['mae']:.8f}</td>"
            f"<td>{row['rmse']:.8f}</td>"
            f"<td>{row['max_abs_error']:.8f}</td>"
            f"<td>{row['python_std']:.8f}</td>"
            f"<td>{row['rust_std']:.8f}</td>"
            "</tr>"
        )
        for row in payload["comparison"]["channel_metrics"]
    )
    config_html = html.escape(json.dumps(payload["config"], indent=2))
    document = f"""<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Preprocessing Comparison</title>
  <style>
    :root {{
      color-scheme: light;
      --bg: #f6f1e8;
      --panel: #fffdf9;
      --ink: #24323f;
      --muted: #617282;
      --line: #d9cfbd;
      --accent: #8c4c12;
    }}
    body {{
      margin: 0;
      background: linear-gradient(180deg, #efe5d0 0%, var(--bg) 34%, #faf8f3 100%);
      color: var(--ink);
      font-family: Cambria, Georgia, serif;
    }}
    main {{
      max-width: 1040px;
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
      <h1>Rust vs SciPy preprocessing</h1>
      <p>Comparison between the local causal SOS runtime and <code>scipy.signal.sosfilt</code> on the same multichannel stream.</p>
    </section>
    <section class="card">
      <h2>Aggregate error</h2>
      <table>
        <tbody>
          <tr><th>MAE</th><td>{payload["comparison"]["mae"]:.10f}</td></tr>
          <tr><th>RMSE</th><td>{payload["comparison"]["rmse"]:.10f}</td></tr>
          <tr><th>Max abs error</th><td>{payload["comparison"]["max_abs_error"]:.10f}</td></tr>
        </tbody>
      </table>
    </section>
    <section class="card">
      <h2>Per-channel error</h2>
      <table>
        <thead>
          <tr>
            <th>Channel</th>
            <th>MAE</th>
            <th>RMSE</th>
            <th>Max abs error</th>
            <th>Python std</th>
            <th>Rust std</th>
          </tr>
        </thead>
        <tbody>{channel_rows}</tbody>
      </table>
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


def main() -> None:
    args = parse_args()
    samples = int(round(args.seconds * args.fs))
    sos = design_sos(args.fs, args.band_low, args.band_high, args.band_order, args.notch_q)
    if sos.shape[0] > 8:
        raise ValueError(
            f"Rust fixture binary currently supports at most 8 SOS sections, got {sos.shape[0]}"
        )

    x = synthesize_signal(samples, args.channels, args.fs, args.seed)
    python_filtered = signal.sosfilt(sos.astype(np.float64), x.astype(np.float64), axis=0)
    rust_filtered = run_rust_fixture(x, sos, args.channels)
    comparison = compare_outputs(python_filtered, rust_filtered)

    payload = {
        "config": {
            "fs": args.fs,
            "channels": args.channels,
            "seconds": args.seconds,
            "samples": samples,
            "band_low": args.band_low,
            "band_high": args.band_high,
            "band_order": args.band_order,
            "notch_q": args.notch_q,
            "sections": int(sos.shape[0]),
        },
        "comparison": comparison,
    }

    args.output_json.parent.mkdir(parents=True, exist_ok=True)
    args.output_html.parent.mkdir(parents=True, exist_ok=True)
    args.output_json.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    render_html(args.output_html, payload)

    print(
        json.dumps(
            {
                "mae": comparison["mae"],
                "rmse": comparison["rmse"],
                "max_abs_error": comparison["max_abs_error"],
                "sections": int(sos.shape[0]),
            },
            indent=2,
        )
    )
    print(f"wrote {args.output_json}")
    print(f"wrote {args.output_html}")


if __name__ == "__main__":
    main()
