from __future__ import annotations

import argparse
import itertools
from pathlib import Path

import numpy as np
from rich.console import Console

from cvep_bench.benchmarks.pyntbci_vs_rust import DEFAULT_DATA_DIR
from cvep_bench.benchmarks.reporting import (
    render_rich_table,
    render_tabular_html,
    write_json_payload,
)
from cvep_bench.benchmarks.subprocesses import (
    build_common_benchmark_argv,
    run_module_with_json_output,
)
from cvep_bench.cli.arg_groups import (
    add_data_dir_arg,
    add_dataset_args,
    add_fold_args,
    add_output_args,
    add_rust_args,
    add_window_args,
)


DEFAULT_WINDOWS = [1.05, 2.1, 4.2, 5.25, 10.5, 31.5]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    add_data_dir_arg(parser, DEFAULT_DATA_DIR)
    add_output_args(
        parser,
        output_dir=DEFAULT_DATA_DIR,
        stem="search_best_125hz_configs",
        include_csv=False,
    )
    add_dataset_args(parser, default_datasets=["Thielen2021"], default_max_subjects=4)
    add_fold_args(parser)
    parser.set_defaults(fold_index=[0])
    add_window_args(parser, default_grid=DEFAULT_WINDOWS, include_step=False)
    add_rust_args(parser)
    parser.add_argument(
        "--families", nargs="+", choices=["etrca", "cca"], default=["etrca", "cca"]
    )
    parser.add_argument("--search-limit", type=int, default=None)
    parser.add_argument("--search-offset", type=int, default=0)
    return parser.parse_args()


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
            output_prefix = args.output_json.parent / f"search_{family}_{idx:03d}"
            command = build_common_benchmark_argv(
                args,
                output_prefix=output_prefix,
                include_profile=False,
                include_target_fs=False,
            )
            command.extend(
                [
                    "--target-fs",
                    "125",
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
            )
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
                payload = run_module_with_json_output(module, command)
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
    write_json_payload(args.output_json, payload)
    render_tabular_html(
        args.output_html,
        title="Best 125 Hz Config Search",
        subtitle="Constrained search over benchmark presets.",
        config=payload["config"],
        summary_columns=[
            ("Family", "family"),
            ("Mean", "mean_accuracy"),
            ("Params", "params"),
        ],
        summary_rows=best,
    )
    render_rich_table(
        console,
        title="Best 125 Hz Configs",
        columns=[("Family", "family"), ("Mean", "mean_accuracy"), ("Params", "params")],
        rows=best,
        formatters={
            "mean_accuracy": lambda value: f"{value:.4f}",
            "params": lambda value: str(value),
        },
    )
