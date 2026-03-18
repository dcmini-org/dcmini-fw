from __future__ import annotations

import argparse
from typing import Any, cast

from rich.console import Console

from cvep_bench.algorithms.umm_features import (
    ConfidenceModelName,
    EpochScheduleName,
    LayoutName,
    build_umm_features,
    cumulative_umm_predictions,
    instantaneous_umm_predictions,
    make_structure,
)
from cvep_bench.benchmarks.pyntbci_vs_rust import DEFAULT_DATA_DIR
from cvep_bench.benchmarks.reporting import (
    render_rich_table,
    render_tabular_html,
    rows_to_csv,
    write_json_payload,
)
from cvep_bench.cli.arg_groups import (
    add_data_dir_arg,
    add_dataset_args,
    add_fold_args,
    add_output_args,
    add_target_fs_args,
    add_window_args,
    parse_bool_choice_grid,
    resolve_fold_indices,
)
from cvep_bench.datasets.loaders import load_subject, validate_target_fs
from cvep_bench.datasets.windows import decode_window_requests, seconds_to_samples
from cvep_bench.evaluation.splits import fold_slices


DEFAULT_DATASETS = ["Thielen2021", "Thielen2015", "CastillosCVEP40", "CastillosCVEP100"]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    add_data_dir_arg(parser, DEFAULT_DATA_DIR)
    add_output_args(parser, output_dir=DEFAULT_DATA_DIR, stem="umm_benchmark_results")
    add_dataset_args(parser, default_datasets=DEFAULT_DATASETS)
    add_fold_args(parser)
    add_target_fs_args(parser, default=250, include_grid=True)
    add_window_args(parser, default_grid=None, include_step=True)
    parser.add_argument("--epoch-seconds-grid", type=float, nargs="+", default=[0.3])
    parser.add_argument(
        "--epoch-schedules",
        nargs="+",
        choices=["rounded_stride", "fractional_onset"],
        default=["fractional_onset"],
    )
    parser.add_argument("--lag-seconds-grid", type=float, nargs="+", default=[0.0])
    parser.add_argument(
        "--layouts",
        nargs="+",
        choices=["channel_prime", "time_prime"],
        default=["channel_prime", "time_prime"],
    )
    parser.add_argument(
        "--trial-demean-grid", nargs="+", choices=["false", "true"], default=["false"]
    )
    parser.add_argument(
        "--epoch-demean-grid", nargs="+", choices=["false", "true"], default=["false"]
    )
    parser.add_argument(
        "--confidence-models",
        nargs="+",
        choices=["inferred_normalized_margin", "margin_over_winner"],
        default=["inferred_normalized_margin", "margin_over_winner"],
    )
    parser.add_argument(
        "--variants",
        nargs="+",
        choices=["instantaneous_umm", "cumulative_umm"],
        default=["instantaneous_umm", "cumulative_umm"],
    )
    parser.add_argument("--regularization", type=float, default=1.0e-3)
    return parser.parse_args()


def grouped_summary(rows: list[dict[str, Any]]) -> list[dict[str, Any]]:
    grouped: dict[tuple[Any, ...], list[dict[str, Any]]] = {}
    for row in rows:
        key = (
            row["variant"],
            row["dataset"],
            row["target_fs"],
            row["requested_window_seconds"],
            row["epoch_seconds"],
            row["epoch_schedule"],
            row["lag_seconds"],
            row["layout"],
            row["trial_demean"],
            row["epoch_demean"],
            row["confidence_model"],
        )
        grouped.setdefault(key, []).append(row)
    out = []
    for key, members in sorted(grouped.items()):
        (
            variant,
            dataset,
            target_fs,
            requested_window_seconds,
            epoch_seconds,
            epoch_schedule,
            lag_seconds,
            layout,
            trial_demean,
            epoch_demean,
            confidence_model,
        ) = key
        out.append(
            {
                "variant": variant,
                "dataset": dataset,
                "target_fs": target_fs,
                "requested_window_seconds": requested_window_seconds,
                "epoch_seconds": epoch_seconds,
                "epoch_schedule": epoch_schedule,
                "lag_seconds": lag_seconds,
                "layout": layout,
                "trial_demean": trial_demean,
                "epoch_demean": epoch_demean,
                "confidence_model": confidence_model,
                "subjects": len({row["subject"] for row in members}),
                "mean_accuracy": sum(row["accuracy"] for row in members) / len(members),
            }
        )
    return out


def main() -> None:
    args = parse_args()
    target_fs_grid = args.target_fs_grid or [args.target_fs]
    trial_demean_grid = parse_bool_choice_grid(args.trial_demean_grid)
    epoch_demean_grid = parse_bool_choice_grid(args.epoch_demean_grid)
    console = Console()
    rows: list[dict[str, Any]] = []
    for target_fs in target_fs_grid:
        validate_target_fs(target_fs)
        for dataset in args.datasets:
            subjects = args.subjects or []
            if not subjects:
                from cvep_bench.datasets.loaders import subject_list_for_dataset

                subjects = subject_list_for_dataset(dataset)
            if args.max_subjects is not None:
                subjects = subjects[: args.max_subjects]
            fold_indices = resolve_fold_indices(args.folds, args.fold_index)
            for subject in subjects:
                data = load_subject(dataset, subject, args.data_dir, target_fs)
                window_requests = decode_window_requests(
                    data.trial_seconds,
                    args.window_seconds_grid,
                    args.window_step_seconds,
                )
                fold_parts = fold_slices(data.x.shape[0], args.folds)
                for fold_idx in fold_indices:
                    test_idx = fold_parts[fold_idx]
                    x_test = data.x[test_idx]
                    y_test = data.y[test_idx]
                    for requested_window_seconds in window_requests:
                        window_samples = min(
                            seconds_to_samples(requested_window_seconds, data.fs),
                            x_test.shape[2],
                        )
                        x_window = x_test[:, :, :window_samples]
                        for epoch_seconds in args.epoch_seconds_grid:
                            for epoch_schedule in args.epoch_schedules:
                                for lag_seconds in args.lag_seconds_grid:
                                    for layout in args.layouts:
                                        for trial_demean in trial_demean_grid:
                                            for epoch_demean in epoch_demean_grid:
                                                exported = build_umm_features(
                                                    x_window,
                                                    data.stimulus,
                                                    data.fs,
                                                    data.presentation_rate,
                                                    epoch_seconds,
                                                    cast(LayoutName, layout),
                                                    cast(
                                                        EpochScheduleName,
                                                        epoch_schedule,
                                                    ),
                                                    lag_seconds,
                                                    trial_demean,
                                                    epoch_demean,
                                                )
                                                structure = make_structure(exported)
                                                if "instantaneous_umm" in args.variants:
                                                    predictions, _scores = (
                                                        instantaneous_umm_predictions(
                                                            exported.features,
                                                            exported.codebook,
                                                            regularization=args.regularization,
                                                            structure=structure,
                                                        )
                                                    )
                                                    rows.append(
                                                        {
                                                            "variant": "instantaneous_umm",
                                                            "dataset": dataset,
                                                            "subject": subject,
                                                            "fold_index": fold_idx,
                                                            "target_fs": data.fs,
                                                            "requested_window_seconds": requested_window_seconds,
                                                            "window_seconds": window_samples
                                                            / data.fs,
                                                            "epoch_seconds": epoch_seconds,
                                                            "epoch_schedule": epoch_schedule,
                                                            "lag_seconds": lag_seconds,
                                                            "layout": layout,
                                                            "trial_demean": trial_demean,
                                                            "epoch_demean": epoch_demean,
                                                            "confidence_model": None,
                                                            "classes": int(
                                                                exported.codebook.shape[
                                                                    0
                                                                ]
                                                            ),
                                                            "channels": int(
                                                                data.x.shape[1]
                                                            ),
                                                            "feature_count": int(
                                                                exported.features.shape[
                                                                    1
                                                                ]
                                                            ),
                                                            "epochs_per_trial": exported.epochs_per_trial,
                                                            "trials": int(
                                                                exported.features.shape[
                                                                    0
                                                                ]
                                                            ),
                                                            "accuracy": float(
                                                                (
                                                                    predictions
                                                                    == y_test
                                                                ).mean()
                                                            ),
                                                        }
                                                    )
                                                if "cumulative_umm" in args.variants:
                                                    for (
                                                        confidence_model
                                                    ) in args.confidence_models:
                                                        predictions, _scores, _state = (
                                                            cumulative_umm_predictions(
                                                                exported.features,
                                                                exported.codebook,
                                                                regularization=args.regularization,
                                                                structure=structure,
                                                                confidence_model=cast(
                                                                    ConfidenceModelName,
                                                                    confidence_model,
                                                                ),
                                                            )
                                                        )
                                                        rows.append(
                                                            {
                                                                "variant": "cumulative_umm",
                                                                "dataset": dataset,
                                                                "subject": subject,
                                                                "fold_index": fold_idx,
                                                                "target_fs": data.fs,
                                                                "requested_window_seconds": requested_window_seconds,
                                                                "window_seconds": window_samples
                                                                / data.fs,
                                                                "epoch_seconds": epoch_seconds,
                                                                "epoch_schedule": epoch_schedule,
                                                                "lag_seconds": lag_seconds,
                                                                "layout": layout,
                                                                "trial_demean": trial_demean,
                                                                "epoch_demean": epoch_demean,
                                                                "confidence_model": confidence_model,
                                                                "classes": int(
                                                                    exported.codebook.shape[
                                                                        0
                                                                    ]
                                                                ),
                                                                "channels": int(
                                                                    data.x.shape[1]
                                                                ),
                                                                "feature_count": int(
                                                                    exported.features.shape[
                                                                        1
                                                                    ]
                                                                ),
                                                                "epochs_per_trial": exported.epochs_per_trial,
                                                                "trials": int(
                                                                    exported.features.shape[
                                                                        0
                                                                    ]
                                                                ),
                                                                "accuracy": float(
                                                                    (
                                                                        predictions
                                                                        == y_test
                                                                    ).mean()
                                                                ),
                                                            }
                                                        )
    payload = {
        "config": {
            "datasets": args.datasets,
            "subjects": args.subjects,
            "max_subjects": args.max_subjects,
            "folds": args.folds,
            "fold_index": args.fold_index,
            "target_fs_grid": target_fs_grid,
            "window_step_seconds": args.window_step_seconds,
            "window_seconds_grid": args.window_seconds_grid,
            "epoch_seconds_grid": args.epoch_seconds_grid,
            "epoch_schedules": args.epoch_schedules,
            "lag_seconds_grid": args.lag_seconds_grid,
            "layouts": args.layouts,
            "trial_demean_grid": trial_demean_grid,
            "epoch_demean_grid": epoch_demean_grid,
            "confidence_models": args.confidence_models,
            "variants": args.variants,
            "regularization": args.regularization,
            "source_note": "Winner-vs-runner-up confidence dependence is source-backed, but the exact confidence transform remains an exposed benchmark choice because the accessible papers and public repos did not disclose one verified formula.",
        },
        "results": rows,
    }
    write_json_payload(args.output_json, payload)
    args.output_csv.write_text(
        rows_to_csv(
            rows,
            [
                "variant",
                "dataset",
                "subject",
                "fold_index",
                "target_fs",
                "requested_window_seconds",
                "window_seconds",
                "epoch_seconds",
                "epoch_schedule",
                "lag_seconds",
                "layout",
                "trial_demean",
                "epoch_demean",
                "confidence_model",
                "classes",
                "channels",
                "feature_count",
                "epochs_per_trial",
                "trials",
                "accuracy",
            ],
        ),
        encoding="utf-8",
    )
    summary_rows = grouped_summary(rows)
    render_tabular_html(
        args.output_html,
        title="UMM benchmark summary",
        subtitle="UMM design-space sweep across datasets and feature settings.",
        config=payload["config"],
        summary_columns=[
            ("Variant", "variant"),
            ("Dataset", "dataset"),
            ("fs", "target_fs"),
            ("Window", "requested_window_seconds"),
            ("Epoch", "epoch_seconds"),
            ("Schedule", "epoch_schedule"),
            ("Lag", "lag_seconds"),
            ("Layout", "layout"),
            ("Trial Demean", "trial_demean"),
            ("Epoch Demean", "epoch_demean"),
            ("Confidence", "confidence_model"),
            ("Subjects", "subjects"),
            ("Mean Accuracy", "mean_accuracy"),
        ],
        summary_rows=summary_rows,
    )
    render_rich_table(
        console,
        title="UMM benchmark summary",
        columns=[
            ("variant", "variant"),
            ("dataset", "dataset"),
            ("fs", "target_fs"),
            ("window", "requested_window_seconds"),
            ("epoch", "epoch_seconds"),
            ("schedule", "epoch_schedule"),
            ("lag", "lag_seconds"),
            ("layout", "layout"),
            ("trial_demean", "trial_demean"),
            ("epoch_demean", "epoch_demean"),
            ("confidence", "confidence_model"),
            ("subjects", "subjects"),
            ("mean_acc", "mean_accuracy"),
        ],
        rows=summary_rows,
        formatters={
            "requested_window_seconds": lambda value: f"{value:.3f}",
            "epoch_seconds": lambda value: f"{value:.3f}",
            "lag_seconds": lambda value: f"{value:.3f}",
            "mean_accuracy": lambda value: f"{value:.4f}",
        },
    )
