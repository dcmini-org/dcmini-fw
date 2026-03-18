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

from cvep_bench.benchmarks.pyntbci_vs_rust import DEFAULT_DATA_DIR


DEFAULT_WINDOWS = [1.05, 2.1, 4.2, 5.25, 10.5, 31.5]
DEFAULT_PROFILES = [
    "matched_embedded_125",
    "matched_diagnostic_125",
    "matched_onset_aware_125",
    "literature_oriented_125",
]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data-dir", type=Path, default=DEFAULT_DATA_DIR)
    parser.add_argument(
        "--output-json",
        type=Path,
        default=DEFAULT_DATA_DIR / "benchmark_125hz_profile_matrix.json",
    )
    parser.add_argument(
        "--output-csv",
        type=Path,
        default=DEFAULT_DATA_DIR / "benchmark_125hz_profile_matrix.csv",
    )
    parser.add_argument(
        "--output-html",
        type=Path,
        default=DEFAULT_DATA_DIR / "benchmark_125hz_profile_matrix.html",
    )
    parser.add_argument("--datasets", nargs="+", default=["Thielen2021"])
    parser.add_argument("--subjects", type=int, nargs="+", default=None)
    parser.add_argument("--max-subjects", type=int, default=None)
    parser.add_argument("--folds", type=int, default=5)
    parser.add_argument("--fold-index", type=int, nargs="+", default=None)
    parser.add_argument("--profiles", nargs="+", default=DEFAULT_PROFILES)
    parser.add_argument(
        "--window-seconds-grid", type=float, nargs="+", default=DEFAULT_WINDOWS
    )
    parser.add_argument("--skip-rust", action="store_true", default=True)
    parser.add_argument("--include-legacy", action="store_true")
    return parser.parse_args()


def run_module(module: str, command: list[str]) -> dict[str, Any]:
    result = subprocess.run(
        [sys.executable, "-m", module, *command], capture_output=True, text=True
    )
    if result.returncode != 0:
        raise RuntimeError(
            f"Command failed with code {result.returncode}\nstdout:\n{result.stdout}\n\nstderr:\n{result.stderr}"
        )
    output_json = Path(command[command.index("--output-json") + 1])
    payload = json.loads(output_json.read_text(encoding="utf-8"))
    payload["stdout"] = result.stdout
    payload["stderr"] = result.stderr
    return payload


def base_command(
    profile: str, output_prefix: Path, args: argparse.Namespace
) -> list[str]:
    command = [
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


def grouped_summary(rows: list[dict[str, Any]]) -> list[dict[str, Any]]:
    grouped: dict[tuple[str, str, str, float], list[dict[str, Any]]] = {}
    for row in rows:
        grouped.setdefault(
            (
                row["algorithm"],
                row["dataset"],
                row["profile"],
                float(row["requested_window_seconds"]),
            ),
            [],
        ).append(row)
    out = []
    for (algorithm, dataset, profile, requested_window), members in sorted(
        grouped.items()
    ):
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
    output.write_text(
        f"<!doctype html><html lang='en'><body><pre>{html.escape(json.dumps(config, indent=2))}</pre><table><tbody>{summary_rows}</tbody></table></body></html>",
        encoding="utf-8",
    )


def main() -> None:
    args = parse_args()
    args.output_json.parent.mkdir(parents=True, exist_ok=True)
    console = Console()
    profiles = list(args.profiles)
    if args.include_legacy and "legacy" not in profiles:
        profiles.append("legacy")
    all_rows: list[dict[str, Any]] = []
    run_log: list[dict[str, Any]] = []
    for profile in profiles:
        for module, stem, algs in [
            ("cvep_bench.cli.benchmark_pyntbci_vs_rust", "matrix_etrca", ["etrca"]),
            (
                "cvep_bench.cli.benchmark_cca_vs_rust",
                "matrix_cca",
                ["instantaneous_cca", "cumulative_cca"],
            ),
        ]:
            prefix = args.output_json.parent / f"{stem}_{profile}"
            command = base_command(profile, prefix, args) + ["--algorithms", *algs]
            try:
                payload = run_module(module, command)
            except RuntimeError as exc:
                run_log.append(
                    {"profile": profile, "module": module, "error": str(exc)}
                )
                continue
            for row in payload["results"]:
                row["script"] = module.rsplit(".", 1)[-1]
                all_rows.append(row)
            run_log.append(
                {"profile": profile, "module": module, "config": payload["config"]}
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
    for col in ["Algorithm", "Dataset", "Profile", "Req", "Eff", "Subjects", "Mean"]:
        table.add_column(col)
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
