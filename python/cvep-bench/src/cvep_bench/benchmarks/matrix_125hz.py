from __future__ import annotations

import argparse
from pathlib import Path
from typing import Any

import numpy as np
from rich.console import Console

from cvep_bench.benchmarks.reporting import (
    render_rich_table,
    render_tabular_html,
    rows_to_csv,
    write_json_payload,
)
from cvep_bench.benchmarks.subprocesses import (
    build_common_benchmark_argv,
    run_module_with_json_output,
)
from cvep_bench.benchmarks.pyntbci_vs_rust import DEFAULT_DATA_DIR
from cvep_bench.cli.arg_groups import (
    add_data_dir_arg,
    add_dataset_args,
    add_fold_args,
    add_output_args,
    add_rust_args,
    add_window_args,
)


DEFAULT_WINDOWS = [1.05, 2.1, 4.2, 5.25, 10.5, 31.5]
DEFAULT_PROFILES = [
    "matched_embedded_125",
    "matched_diagnostic_125",
    "matched_onset_aware_125",
    "literature_oriented_125",
]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    add_data_dir_arg(parser, DEFAULT_DATA_DIR)
    add_output_args(
        parser, output_dir=DEFAULT_DATA_DIR, stem="benchmark_125hz_profile_matrix"
    )
    add_dataset_args(parser, default_datasets=["Thielen2021"])
    add_fold_args(parser)
    parser.add_argument("--profiles", nargs="+", default=DEFAULT_PROFILES)
    add_window_args(parser, default_grid=DEFAULT_WINDOWS, include_step=False)
    add_rust_args(parser)
    parser.add_argument("--include-legacy", action="store_true")
    return parser.parse_args()


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
            command = build_common_benchmark_argv(
                args,
                output_prefix=prefix,
                include_profile=True,
                include_target_fs=False,
                profile_value=profile,
            ) + ["--algorithms", *algs]
            try:
                payload = run_module_with_json_output(module, command)
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
    write_json_payload(args.output_json, payload)
    args.output_csv.write_text(
        rows_to_csv(
            all_rows,
            [
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
            ],
        ),
        encoding="utf-8",
    )
    render_tabular_html(
        args.output_html,
        title="125 Hz Method/Profile Matrix",
        subtitle="Matched-method benchmark matrix across profile presets.",
        config=payload["config"],
        summary_columns=[
            ("Algorithm", "algorithm"),
            ("Dataset", "dataset"),
            ("Profile", "profile"),
            ("Requested", "requested_window_seconds"),
            ("Effective", "effective_window_seconds"),
            ("Subjects", "subjects"),
            ("Mean Accuracy", "mean_accuracy"),
        ],
        summary_rows=summary,
    )
    render_rich_table(
        console,
        title="125 Hz Profile Matrix",
        columns=[
            ("Algorithm", "algorithm"),
            ("Dataset", "dataset"),
            ("Profile", "profile"),
            ("Req", "requested_window_seconds"),
            ("Eff", "effective_window_seconds"),
            ("Subjects", "subjects"),
            ("Mean", "mean_accuracy"),
        ],
        rows=summary,
        formatters={
            "requested_window_seconds": lambda value: f"{value:.3f}",
            "effective_window_seconds": lambda value: f"{value:.3f}",
            "mean_accuracy": lambda value: f"{value:.4f}",
        },
    )
