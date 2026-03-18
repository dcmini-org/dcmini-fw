from __future__ import annotations

import argparse
import html
import itertools
import json
from pathlib import Path
import subprocess
import sys

import numpy as np
from rich.console import Console
from rich.table import Table

from cvep_bench.benchmarks.pyntbci_vs_rust import DEFAULT_DATA_DIR


DEFAULT_WINDOWS = [1.05, 2.1, 4.2, 5.25, 10.5, 31.5]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data-dir", type=Path, default=DEFAULT_DATA_DIR)
    parser.add_argument(
        "--output-json",
        type=Path,
        default=DEFAULT_DATA_DIR / "search_best_125hz_configs.json",
    )
    parser.add_argument(
        "--output-html",
        type=Path,
        default=DEFAULT_DATA_DIR / "search_best_125hz_configs.html",
    )
    parser.add_argument("--datasets", nargs="+", default=["Thielen2021"])
    parser.add_argument("--subjects", type=int, nargs="+", default=None)
    parser.add_argument("--max-subjects", type=int, default=4)
    parser.add_argument("--folds", type=int, default=5)
    parser.add_argument("--fold-index", type=int, nargs="+", default=[0])
    parser.add_argument(
        "--window-seconds-grid", type=float, nargs="+", default=DEFAULT_WINDOWS
    )
    parser.add_argument("--skip-rust", action="store_true", default=True)
    parser.add_argument(
        "--families", nargs="+", choices=["etrca", "cca"], default=["etrca", "cca"]
    )
    parser.add_argument("--search-limit", type=int, default=None)
    parser.add_argument("--search-offset", type=int, default=0)
    return parser.parse_args()


def base_command(module: str, output_json: Path, args: argparse.Namespace) -> list[str]:
    command = [
        sys.executable,
        "-m",
        module,
        "--data-dir",
        str(args.data_dir),
        "--output-json",
        str(output_json),
        "--output-csv",
        str(output_json.with_suffix(".csv")),
        "--output-html",
        str(output_json.with_suffix(".html")),
        "--datasets",
        *args.datasets,
        "--folds",
        str(args.folds),
        "--fold-index",
        *[str(v) for v in args.fold_index],
        "--window-seconds-grid",
        *[str(v) for v in args.window_seconds_grid],
        "--target-fs",
        "125",
    ]
    if args.subjects is not None:
        command.extend(["--subjects", *[str(v) for v in args.subjects]])
    if args.max_subjects is not None:
        command.extend(["--max-subjects", str(args.max_subjects)])
    if args.skip_rust:
        command.append("--skip-rust")
    return command


def run_command(command: list[str]) -> dict:
    result = subprocess.run(command, capture_output=True, text=True)
    if result.returncode != 0:
        raise RuntimeError(
            f"Command failed with code {result.returncode}\nstdout:\n{result.stdout}\n\nstderr:\n{result.stderr}"
        )
    output_json = Path(command[command.index("--output-json") + 1])
    payload = json.loads(output_json.read_text(encoding="utf-8"))
    payload["stdout"] = result.stdout
    payload["stderr"] = result.stderr
    return payload


def summary_accuracy(payload: dict) -> float:
    rows = payload["results"]
    if not rows:
        return 0.0
    key = (
        "python_reference_accuracy"
        if rows[0].get("python_reference_accuracy") is not None
        else "pyntbci_accuracy"
    )
    return float(np.mean([row[key] for row in rows]))


def etrca_search_space() -> list[dict]:
    out = []
    for band_low, band_high, drop_first in itertools.product(
        [1.0, 4.0, 6.0], [40.0, 50.0, 65.0], [0.0, 0.5]
    ):
        if band_low < band_high:
            out.append(
                {
                    "profile": "matched_embedded_125",
                    "algorithms": ["etrca"],
                    "band_low": band_low,
                    "band_high": band_high,
                    "drop_first_seconds": drop_first,
                }
            )
    return out


def cca_search_space() -> list[dict]:
    out = []
    for algorithm, event, onset_event, encoding_length, drop_first, (
        band_low,
        band_high,
    ) in itertools.product(
        ["instantaneous_cca", "cumulative_cca"],
        ["refe", "duration"],
        [False, True],
        [0.2, 0.3, 0.4],
        [0.0, 0.5],
        [(1.0, 65.0), (4.0, 40.0), (6.0, 50.0)],
    ):
        if event == "duration" and not onset_event:
            continue
        out.append(
            {
                "profile": "matched_embedded_125",
                "algorithms": [algorithm],
                "band_low": band_low,
                "band_high": band_high,
                "event": event,
                "onset_event": onset_event,
                "encoding_length": encoding_length,
                "drop_first_seconds": drop_first,
            }
        )
    return out


def main() -> None:
    args = parse_args()
    console = Console()
    trials = []
    families = [
        ("etrca", "cvep_bench.cli.benchmark_pyntbci_vs_rust", etrca_search_space()),
        ("cca", "cvep_bench.cli.benchmark_cca_vs_rust", cca_search_space()),
    ]
    for family, module, search_space in families:
        if family not in args.families:
            continue
        if args.search_offset:
            search_space = search_space[args.search_offset :]
        if args.search_limit is not None:
            search_space = search_space[: args.search_limit]
        for idx, params in enumerate(search_space):
            output_json = args.output_json.parent / f"search_{family}_{idx:03d}.json"
            command = base_command(module, output_json, args) + [
                "--profile",
                params["profile"],
                "--algorithms",
                *params["algorithms"],
                "--band-low",
                str(params["band_low"]),
                "--band-high",
                str(params["band_high"]),
                "--drop-first-seconds",
                str(params["drop_first_seconds"]),
            ]
            if family == "cca":
                command.extend(
                    [
                        "--event",
                        params["event"],
                        "--encoding-length",
                        str(params["encoding_length"]),
                    ]
                )
                command.append(
                    "--onset-event" if params["onset_event"] else "--no-onset-event"
                )
            try:
                payload = run_command(command)
                trials.append(
                    {
                        "family": family,
                        "params": params,
                        "mean_accuracy": summary_accuracy(payload),
                        "results": payload["results"],
                        "error": None,
                    }
                )
            except RuntimeError as exc:
                trials.append(
                    {
                        "family": family,
                        "params": params,
                        "mean_accuracy": float("nan"),
                        "results": [],
                        "error": str(exc),
                    }
                )
    best = []
    for family in args.families:
        members = [
            trial
            for trial in trials
            if trial["family"] == family and not np.isnan(trial["mean_accuracy"])
        ]
        if members:
            best.append(max(members, key=lambda item: item["mean_accuracy"]))
    payload = {
        "config": {
            "datasets": args.datasets,
            "subjects": args.subjects,
            "max_subjects": args.max_subjects,
            "folds": args.folds,
            "fold_index": args.fold_index,
            "window_seconds_grid": args.window_seconds_grid,
            "skip_rust": args.skip_rust,
            "families": args.families,
            "search_limit": args.search_limit,
            "search_offset": args.search_offset,
        },
        "best": best,
        "trials": sorted(
            trials,
            key=lambda item: (
                -item["mean_accuracy"]
                if not np.isnan(item["mean_accuracy"])
                else float("inf"),
                item["family"],
            ),
        ),
    }
    args.output_json.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    args.output_html.write_text(
        f"<!doctype html><html lang='en'><body><pre>{html.escape(json.dumps(payload, indent=2))}</pre></body></html>",
        encoding="utf-8",
    )
    table = Table(title="Best 125 Hz Configs")
    for col in ["Family", "Mean", "Params"]:
        table.add_column(col)
    for item in best:
        table.add_row(
            item["family"],
            f"{item['mean_accuracy']:.4f}",
            json.dumps(item["params"], sort_keys=True),
        )
    console.print(table)
