#!/usr/bin/env -S uv run --script
# /// script
# dependencies = [
#   "numpy>=2.2.6",
#   "rich>=14.3.3",
# ]
# ///
"""Run the 125 Hz profile matrix across eTRCA and zero-training CCA."""

from __future__ import annotations

import argparse
import html
import json
from pathlib import Path
import subprocess
import sys
from typing import Any

import numpy as np
from rich.console import Console
from rich.table import Table


CRATE_ROOT = Path(__file__).resolve().parents[1]
WORKSPACE_ROOT = Path(__file__).resolve().parents[3]
ETRCA_SCRIPT = CRATE_ROOT / "scripts/benchmark_pyntbci_vs_rust.py"
CCA_SCRIPT = CRATE_ROOT / "scripts/benchmark_cca_vs_rust.py"
DEFAULT_WINDOWS = [1.05, 2.1, 4.2, 5.25, 10.5, 31.5]
DEFAULT_PROFILES = [
    "matched_embedded_125",
    "matched_diagnostic_125",
    "matched_onset_aware_125",
    "literature_oriented_125",
]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data-dir", type=Path, default=CRATE_ROOT / "data")
    parser.add_argument(
        "--output-json",
        type=Path,
        default=CRATE_ROOT / "data/benchmark_125hz_profile_matrix.json",
    )
    parser.add_argument(
        "--output-csv",
        type=Path,
        default=CRATE_ROOT / "data/benchmark_125hz_profile_matrix.csv",
    )
    parser.add_argument(
        "--output-html",
        type=Path,
        default=CRATE_ROOT / "data/benchmark_125hz_profile_matrix.html",
    )
    parser.add_argument("--datasets", nargs="+", default=["Thielen2021"])
    parser.add_argument("--subjects", type=int, nargs="+", default=None)
    parser.add_argument("--max-subjects", type=int, default=None)
    parser.add_argument("--folds", type=int, default=5)
    parser.add_argument("--fold-index", type=int, nargs="+", default=None)
    parser.add_argument("--profiles", nargs="+", default=DEFAULT_PROFILES)
    parser.add_argument(
        "--window-seconds-grid",
        type=float,
        nargs="+",
        default=DEFAULT_WINDOWS,
    )
    parser.add_argument("--skip-rust", action="store_true", default=True)
    parser.add_argument("--include-legacy", action="store_true")
    return parser.parse_args()


def run_script(command: list[str]) -> dict[str, Any]:
    result = subprocess.run(
        command,
        cwd=WORKSPACE_ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    output_json = Path(command[command.index("--output-json") + 1])
    payload = json.loads(output_json.read_text(encoding="utf-8"))
    payload["stdout"] = result.stdout
    payload["stderr"] = result.stderr
    return payload


def base_command(
    script: Path,
    profile: str,
    output_prefix: Path,
    args: argparse.Namespace,
) -> list[str]:
    command = [
        "uv",
        "run",
        str(script),
        "--profile",
        profile,
        "--data-dir",
        str(args.data_dir),
        "--output-json",
        str(output_prefix.with_suffix(".json")),
        "--output-csv",
        str(output_prefix.with_suffix(".csv")),
        "--output-html",
        str(output_prefix.with_suffix(".html")),
        "--datasets",
        *args.datasets,
        "--folds",
        str(args.folds),
        "--window-seconds-grid",
        *[str(value) for value in args.window_seconds_grid],
    ]
    if args.subjects is not None:
        command.extend(["--subjects", *[str(value) for value in args.subjects]])
    if args.max_subjects is not None:
        command.extend(["--max-subjects", str(args.max_subjects)])
    if args.fold_index is not None:
        command.extend(["--fold-index", *[str(value) for value in args.fold_index]])
    if args.skip_rust:
        command.append("--skip-rust")
    return command


def rows_to_csv(rows: list[dict[str, Any]]) -> str:
    keys = [
        "script",
        "algorithm",
        "dataset",
        "profile",
        "subject",
        "fold_index",
        "target_fs",
        "requested_window_seconds",
        "window_seconds",
        "effective_window_seconds",
        "leading_trim_seconds",
        "pyntbci_accuracy",
        "python_reference_accuracy",
    ]
    lines = [",".join(keys)]
    for row in rows:
        lines.append(
            ",".join("" if row.get(key) is None else str(row[key]) for key in keys)
        )
    return "\n".join(lines) + "\n"


def grouped_summary(rows: list[dict[str, Any]]) -> list[dict[str, Any]]:
    grouped: dict[tuple[str, str, str, float], list[dict[str, Any]]] = {}
    for row in rows:
        key = (
            row["algorithm"],
            row["dataset"],
            row["profile"],
            float(row["requested_window_seconds"]),
        )
        grouped.setdefault(key, []).append(row)
    out = []
    for key, members in sorted(grouped.items()):
        algorithm, dataset, profile, requested_window = key
        accuracy_key = (
            "python_reference_accuracy"
            if members[0].get("python_reference_accuracy") is not None
            else "pyntbci_accuracy"
        )
        out.append(
            {
                "algorithm": algorithm,
                "dataset": dataset,
                "profile": profile,
                "requested_window_seconds": requested_window,
                "effective_window_seconds": float(
                    np.mean([row["effective_window_seconds"] for row in members])
                ),
                "subjects": len({row["subject"] for row in members}),
                "mean_accuracy": float(np.mean([row[accuracy_key] for row in members])),
            }
        )
    return out


def render_html(
    output: Path,
    config: dict[str, Any],
    rows: list[dict[str, Any]],
    summary: list[dict[str, Any]],
) -> None:
    summary_rows = "\n".join(
        (
            "<tr>"
            f"<td>{html.escape(row['algorithm'])}</td>"
            f"<td>{html.escape(row['dataset'])}</td>"
            f"<td>{html.escape(row['profile'])}</td>"
            f"<td>{row['requested_window_seconds']:.3f}</td>"
            f"<td>{row['effective_window_seconds']:.3f}</td>"
            f"<td>{row['subjects']}</td>"
            f"<td>{row['mean_accuracy']:.4f}</td>"
            "</tr>"
        )
        for row in summary
    )
    detail_rows = "\n".join(
        (
            "<tr>"
            f"<td>{html.escape(row['algorithm'])}</td>"
            f"<td>{html.escape(row['dataset'])}</td>"
            f"<td>{html.escape(row['profile'])}</td>"
            f"<td>{row['subject']}</td>"
            f"<td>{row['fold_index']}</td>"
            f"<td>{row['requested_window_seconds']:.3f}</td>"
            f"<td>{row['effective_window_seconds']:.3f}</td>"
            f"{(row.get('python_reference_accuracy') if row.get('python_reference_accuracy') is not None else row['pyntbci_accuracy']):.4f}</td>"
            "</tr>"
        )
        for row in rows
    )
    document = f"""<!doctype html>
<html lang=\"en\">
<head>
  <meta charset=\"utf-8\">
  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">
  <title>125 Hz Profile Matrix</title>
  <style>
    :root {{ --bg: #f6f2ea; --panel: #fffdf8; --ink: #1d2935; --muted: #5b6875; --line: #d8cfbf; }}
    body {{ margin: 0; background: var(--bg); color: var(--ink); font-family: Georgia, serif; }}
    main {{ max-width: 1280px; margin: 0 auto; padding: 28px 18px 40px; }}
    .card {{ background: var(--panel); border: 1px solid var(--line); border-radius: 16px; padding: 18px; margin-bottom: 18px; }}
    table {{ width: 100%; border-collapse: collapse; font-size: 0.95rem; }}
    th, td {{ padding: 10px 8px; border-bottom: 1px solid var(--line); text-align: left; }}
    th {{ color: var(--muted); text-transform: uppercase; letter-spacing: 0.06em; font-size: 0.75rem; }}
    pre {{ overflow-x: auto; background: #f6f2ea; padding: 12px; border-radius: 12px; }}
  </style>
</head>
<body>
  <main>
    <div class=\"card\">
      <h1>125 Hz Method/Profile Matrix</h1>
      <pre>{html.escape(json.dumps(config, indent=2))}</pre>
    </div>
    <div class=\"card\">
      <h2>Summary</h2>
      <table>
        <thead><tr><th>Algorithm</th><th>Dataset</th><th>Profile</th><th>Requested</th><th>Effective</th><th>Subjects</th><th>Mean accuracy</th></tr></thead>
        <tbody>{summary_rows}</tbody>
      </table>
    </div>
    <div class=\"card\">
      <h2>Details</h2>
      <table>
        <thead><tr><th>Algorithm</th><th>Dataset</th><th>Profile</th><th>Subject</th><th>Fold</th><th>Requested</th><th>Effective</th><th>Accuracy</th></tr></thead>
        <tbody>{detail_rows}</tbody>
      </table>
    </div>
  </main>
</body>
</html>
"""
    output.write_text(document, encoding="utf-8")


def main() -> None:
    args = parse_args()
    args.output_json.parent.mkdir(parents=True, exist_ok=True)
    all_rows: list[dict[str, Any]] = []
    run_log: list[dict[str, Any]] = []
    console = Console()
    profiles = list(args.profiles)
    if args.include_legacy and "legacy" not in profiles:
        profiles.append("legacy")

    for profile in profiles:
        etrca_prefix = args.output_json.parent / f"matrix_etrca_{profile}"
        etrca_command = base_command(ETRCA_SCRIPT, profile, etrca_prefix, args)
        etrca_command.extend(["--algorithms", "etrca"])
        console.print(f"[cyan]matrix[/cyan] running {' '.join(etrca_command[2:8])} ...")
        etrca_payload = run_script(etrca_command)
        for row in etrca_payload["results"]:
            row["script"] = "benchmark_pyntbci_vs_rust.py"
            all_rows.append(row)
        run_log.append(
            {"profile": profile, "script": "etrca", "config": etrca_payload["config"]}
        )

        cca_prefix = args.output_json.parent / f"matrix_cca_{profile}"
        cca_command = base_command(CCA_SCRIPT, profile, cca_prefix, args)
        cca_command.extend(["--algorithms", "instantaneous_cca", "cumulative_cca"])
        console.print(f"[cyan]matrix[/cyan] running {' '.join(cca_command[2:8])} ...")
        cca_payload = run_script(cca_command)
        for row in cca_payload["results"]:
            row["script"] = "benchmark_cca_vs_rust.py"
            all_rows.append(row)
        run_log.append(
            {"profile": profile, "script": "cca", "config": cca_payload["config"]}
        )

    summary = grouped_summary(all_rows)
    payload = {
        "config": {
            "datasets": args.datasets,
            "subjects": args.subjects,
            "max_subjects": args.max_subjects,
            "folds": args.folds,
            "fold_index": args.fold_index,
            "profiles": profiles,
            "window_seconds_grid": args.window_seconds_grid,
            "skip_rust": args.skip_rust,
        },
        "runs": run_log,
        "summary": summary,
        "results": all_rows,
    }
    args.output_json.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    args.output_csv.write_text(rows_to_csv(all_rows), encoding="utf-8")
    render_html(args.output_html, payload["config"], all_rows, summary)

    table = Table(title="125 Hz Profile Matrix")
    for column in ["Algorithm", "Dataset", "Profile", "Req", "Eff", "Subjects", "Mean"]:
        table.add_column(column)
    for row in summary:
        table.add_row(
            row["algorithm"],
            row["dataset"],
            row["profile"],
            f"{row['requested_window_seconds']:.3f}",
            f"{row['effective_window_seconds']:.3f}",
            str(row["subjects"]),
            f"{row['mean_accuracy']:.4f}",
        )
    console.print(table)


if __name__ == "__main__":
    main()
