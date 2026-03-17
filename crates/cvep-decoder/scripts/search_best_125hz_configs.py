#!/usr/bin/env -S uv run --script
# /// script
# dependencies = [
#   "numpy>=2.2.6",
#   "rich>=14.3.3",
# ]
# ///
"""Constrained search for best-known 125 Hz eTRCA and CCA settings."""

from __future__ import annotations

import argparse
import html
import itertools
import json
from pathlib import Path
import subprocess
from typing import Any

import numpy as np
from rich.console import Console
from rich.table import Table


CRATE_ROOT = Path(__file__).resolve().parents[1]
WORKSPACE_ROOT = Path(__file__).resolve().parents[3]
ETRCA_SCRIPT = CRATE_ROOT / "scripts/benchmark_pyntbci_vs_rust.py"
CCA_SCRIPT = CRATE_ROOT / "scripts/benchmark_cca_vs_rust.py"
DEFAULT_WINDOWS = [1.05, 2.1, 4.2, 5.25, 10.5, 31.5]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data-dir", type=Path, default=CRATE_ROOT / "data")
    parser.add_argument(
        "--output-json",
        type=Path,
        default=CRATE_ROOT / "data/search_best_125hz_configs.json",
    )
    parser.add_argument(
        "--output-html",
        type=Path,
        default=CRATE_ROOT / "data/search_best_125hz_configs.html",
    )
    parser.add_argument("--datasets", nargs="+", default=["Thielen2021"])
    parser.add_argument("--subjects", type=int, nargs="+", default=None)
    parser.add_argument("--max-subjects", type=int, default=4)
    parser.add_argument("--folds", type=int, default=5)
    parser.add_argument("--fold-index", type=int, nargs="+", default=[0])
    parser.add_argument(
        "--window-seconds-grid",
        type=float,
        nargs="+",
        default=DEFAULT_WINDOWS,
    )
    parser.add_argument("--skip-rust", action="store_true", default=True)
    parser.add_argument(
        "--families", nargs="+", choices=["etrca", "cca"], default=["etrca", "cca"]
    )
    parser.add_argument("--search-limit", type=int, default=None)
    return parser.parse_args()


def run_script(command: list[str]) -> dict[str, Any]:
    result = subprocess.run(
        command,
        cwd=WORKSPACE_ROOT,
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise RuntimeError(
            f"Command failed with code {result.returncode}\n"
            f"stdout:\n{result.stdout}\n\n"
            f"stderr:\n{result.stderr}"
        )
    output_json = Path(command[command.index("--output-json") + 1])
    payload = json.loads(output_json.read_text(encoding="utf-8"))
    payload["stdout"] = result.stdout
    payload["stderr"] = result.stderr
    return payload


def summary_accuracy(payload: dict[str, Any]) -> float:
    rows = payload["results"]
    if not rows:
        return 0.0
    key = (
        "python_reference_accuracy"
        if rows[0].get("python_reference_accuracy") is not None
        else "pyntbci_accuracy"
    )
    return float(np.mean([row[key] for row in rows]))


def render_html(output: Path, payload: dict[str, Any]) -> None:
    best_rows = "\n".join(
        (
            "<tr>"
            f"<td>{html.escape(item['family'])}</td>"
            f"<td>{item['mean_accuracy']:.4f}</td>"
            f"<td><pre>{html.escape(json.dumps(item['params'], indent=2))}</pre></td>"
            "</tr>"
        )
        for item in payload["best"]
    )
    tried_rows = "\n".join(
        (
            "<tr>"
            f"<td>{html.escape(item['family'])}</td>"
            f"<td>{item['mean_accuracy']:.4f}</td>"
            f"<td><pre>{html.escape(json.dumps(item['params'], indent=2))}</pre></td>"
            "</tr>"
        )
        for item in payload["trials"]
    )
    document = f"""<!doctype html>
<html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>Best 125 Hz Search</title>
<style>:root {{ --bg:#f6f2ea; --panel:#fffdf8; --ink:#1d2935; --muted:#5b6875; --line:#d8cfbf; }} body {{ margin:0; background:var(--bg); color:var(--ink); font-family:Georgia,serif; }} main {{ max-width:1280px; margin:0 auto; padding:28px 18px 40px; }} .card {{ background:var(--panel); border:1px solid var(--line); border-radius:16px; padding:18px; margin-bottom:18px; }} table {{ width:100%; border-collapse:collapse; font-size:0.95rem; }} th,td {{ padding:10px 8px; border-bottom:1px solid var(--line); text-align:left; vertical-align:top; }} th {{ color:var(--muted); text-transform:uppercase; letter-spacing:0.06em; font-size:0.75rem; }} pre {{ overflow-x:auto; background:#f6f2ea; padding:12px; border-radius:12px; }}</style></head>
<body><main>
<div class=\"card\"><h1>Best 125 Hz Search</h1><pre>{html.escape(json.dumps(payload["config"], indent=2))}</pre></div>
<div class=\"card\"><h2>Best configs</h2><table><thead><tr><th>Family</th><th>Mean accuracy</th><th>Params</th></tr></thead><tbody>{best_rows}</tbody></table></div>
<div class=\"card\"><h2>All tried configs</h2><table><thead><tr><th>Family</th><th>Mean accuracy</th><th>Params</th></tr></thead><tbody>{tried_rows}</tbody></table></div>
</main></body></html>"""
    output.write_text(document, encoding="utf-8")


def base_command(
    script: Path,
    output_json: Path,
    args: argparse.Namespace,
) -> list[str]:
    command = [
        "uv",
        "run",
        str(script),
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
        *[str(value) for value in args.fold_index],
        "--window-seconds-grid",
        *[str(value) for value in args.window_seconds_grid],
        "--target-fs",
        "125",
    ]
    if args.subjects is not None:
        command.extend(["--subjects", *[str(value) for value in args.subjects]])
    if args.max_subjects is not None:
        command.extend(["--max-subjects", str(args.max_subjects)])
    if args.skip_rust:
        command.append("--skip-rust")
    return command


def etrca_search_space() -> list[dict[str, Any]]:
    out = []
    for band_low, band_high, drop_first in itertools.product(
        [1.0, 4.0, 6.0],
        [40.0, 50.0, 65.0],
        [0.0, 0.5],
    ):
        if band_low >= band_high:
            continue
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


def cca_search_space() -> list[dict[str, Any]]:
    out = []
    band_pairs = [(1.0, 65.0), (4.0, 40.0), (6.0, 50.0)]
    for (
        algorithm,
        event,
        onset_event,
        encoding_length,
        drop_first,
        (band_low, band_high),
    ) in itertools.product(
        ["instantaneous_cca", "cumulative_cca"],
        ["refe", "duration"],
        [False, True],
        [0.2, 0.3, 0.4],
        [0.0, 0.5],
        band_pairs,
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
    args.output_json.parent.mkdir(parents=True, exist_ok=True)
    console = Console()
    trials: list[dict[str, Any]] = []

    families = [
        ("etrca", ETRCA_SCRIPT, etrca_search_space()),
        ("cca", CCA_SCRIPT, cca_search_space()),
    ]
    for family, script, search_space in families:
        if family not in args.families:
            continue
        if args.search_limit is not None:
            search_space = search_space[: args.search_limit]
        for idx, params in enumerate(search_space):
            output_json = args.output_json.parent / f"search_{family}_{idx:03d}.json"
            command = base_command(script, output_json, args)
            command.extend(
                ["--profile", params["profile"], "--algorithms", *params["algorithms"]]
            )
            command.extend(
                [
                    "--band-low",
                    str(params["band_low"]),
                    "--band-high",
                    str(params["band_high"]),
                ]
            )
            command.extend(["--drop-first-seconds", str(params["drop_first_seconds"])])
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
            console.print(
                f"[cyan]search[/cyan] {family} trial {idx + 1}/{len(search_space)}"
            )
            try:
                payload = run_script(command)
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
                console.print(
                    f"[yellow]search[/yellow] failed: {family} trial {idx + 1}"
                )
                continue
            trials.append(
                {
                    "family": family,
                    "params": params,
                    "mean_accuracy": summary_accuracy(payload),
                    "results": payload["results"],
                    "error": None,
                }
            )

    best = []
    for family in args.families:
        members = [trial for trial in trials if trial["family"] == family]
        members = [trial for trial in members if not np.isnan(trial["mean_accuracy"])]
        if not members:
            continue
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
        },
        "best": best,
        "trials": sorted(
            trials, key=lambda item: (-item["mean_accuracy"], item["family"])
        ),
    }
    args.output_json.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    render_html(args.output_html, payload)

    table = Table(title="Best 125 Hz Configs")
    for column in ["Family", "Mean", "Params"]:
        table.add_column(column)
    for item in best:
        table.add_row(
            item["family"],
            f"{item['mean_accuracy']:.4f}",
            json.dumps(item["params"], sort_keys=True),
        )
    console.print(table)


if __name__ == "__main__":
    main()
