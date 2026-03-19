from __future__ import annotations

import argparse
from pathlib import Path
from typing import Any

from rich.console import Console

from cvep_bench.algorithms.continuous_state_cca import (
    MODE_CROSS_TRIAL,
    MODE_HYBRID,
    MODE_STATELESS,
    MODE_WITHIN_TRIAL,
    STOP_FIXED_DWELL,
    STOP_MARGIN,
    UPDATE_CONFIDENCE,
    UPDATE_ORACLE,
    UPDATE_PSEUDO,
    UPDATE_SCOPE_ALL_OBSERVED,
    UPDATE_SCOPE_EMITTED_ONLY,
    decision_windows,
    initialize_offset_models,
    run_trial,
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
    add_preprocessing_override_args,
    add_profile_arg,
    add_target_fs_args,
    resolve_fold_indices,
)
from cvep_bench.datasets.loaders import (
    load_subject,
    trial_seconds_for_dataset,
    validate_target_fs,
)
from cvep_bench.datasets.profiles import (
    benchmark_profile_names,
    resolve_benchmark_profile,
    resolve_encoding_length,
    resolve_event,
    resolve_onset_event,
    resolve_preprocessing_options,
)
from cvep_bench.datasets.windowing import stimulus_to_sample_rate
from cvep_bench.evaluation.splits import fold_slices


DEFAULT_MODES = [
    MODE_STATELESS,
    MODE_WITHIN_TRIAL,
    MODE_CROSS_TRIAL,
    MODE_HYBRID,
]
DEFAULT_STOP_RULES = [STOP_FIXED_DWELL, STOP_MARGIN]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    add_profile_arg(
        parser, choices=benchmark_profile_names(), default="matched_embedded_125"
    )
    add_data_dir_arg(parser, DEFAULT_DATA_DIR)
    add_output_args(parser, output_dir=DEFAULT_DATA_DIR, stem="continuous_state_cca")
    add_dataset_args(parser, default_datasets=["Thielen2021"], default_max_subjects=8)
    add_fold_args(parser)
    add_target_fs_args(parser, default=250, include_grid=False)
    parser.add_argument("--window-seconds", type=float, default=1.0)
    parser.add_argument("--update-seconds", type=float, default=0.25)
    parser.add_argument("--max-dwell-seconds", type=float, default=4.2)
    parser.add_argument(
        "--modes", nargs="+", choices=DEFAULT_MODES, default=DEFAULT_MODES
    )
    parser.add_argument(
        "--stop-rules",
        nargs="+",
        choices=DEFAULT_STOP_RULES,
        default=DEFAULT_STOP_RULES,
    )
    parser.add_argument(
        "--margin-thresholds", type=float, nargs="+", default=[0.05, 0.10, 0.20]
    )
    parser.add_argument(
        "--update-policy",
        choices=[UPDATE_PSEUDO, UPDATE_CONFIDENCE, UPDATE_ORACLE],
        default=UPDATE_CONFIDENCE,
    )
    parser.add_argument("--update-min-margin", type=float, default=0.05)
    parser.add_argument(
        "--update-scope",
        choices=[UPDATE_SCOPE_EMITTED_ONLY, UPDATE_SCOPE_ALL_OBSERVED],
        default=UPDATE_SCOPE_EMITTED_ONLY,
    )
    parser.add_argument("--update-min-consecutive-winners", type=int, default=2)
    parser.add_argument("--encoding-length", type=float, default=None)
    parser.add_argument("--event", type=str, default=None)
    parser.add_argument(
        "--onset-event", action=argparse.BooleanOptionalAction, default=None
    )
    add_preprocessing_override_args(parser)
    parser.add_argument(
        "--thielen2021-source", choices=["raw", "packaged"], default="raw"
    )
    return parser.parse_args()


def grouped_summary(rows: list[dict[str, Any]]) -> list[dict[str, Any]]:
    grouped: dict[tuple[Any, ...], list[dict[str, Any]]] = {}
    for row in rows:
        key = (
            row["dataset"],
            row["target_fs"],
            row["mode"],
            row["stop_rule"],
            row["stop_threshold"],
            row["update_policy"],
            row["update_min_margin"],
            row["update_scope"],
            row["update_min_consecutive_winners"],
            row["window_seconds"],
            row["update_seconds"],
            row["max_dwell_seconds"],
        )
        grouped.setdefault(key, []).append(row)
    out = []
    for key, members in sorted(grouped.items()):
        (
            dataset,
            target_fs,
            mode,
            stop_rule,
            stop_threshold,
            update_policy,
            update_min_margin,
            update_scope,
            update_min_consecutive_winners,
            window_seconds,
            update_seconds,
            max_dwell_seconds,
        ) = key
        decision_seconds = sorted(row["decision_seconds"] for row in members)
        mid = len(decision_seconds) // 2
        median = (
            decision_seconds[mid]
            if len(decision_seconds) % 2
            else 0.5 * (decision_seconds[mid - 1] + decision_seconds[mid])
        )
        out.append(
            {
                "dataset": dataset,
                "target_fs": target_fs,
                "mode": mode,
                "stop_rule": stop_rule,
                "stop_threshold": stop_threshold,
                "update_policy": update_policy,
                "update_min_margin": update_min_margin,
                "update_scope": update_scope,
                "update_min_consecutive_winners": update_min_consecutive_winners,
                "window_seconds": window_seconds,
                "update_seconds": update_seconds,
                "max_dwell_seconds": max_dwell_seconds,
                "subjects": len({row["subject"] for row in members}),
                "mean_accuracy": sum(row["correct"] for row in members) / len(members),
                "mean_decision_seconds": sum(row["decision_seconds"] for row in members)
                / len(members),
                "median_decision_seconds": median,
                "early_stop_rate": sum(row["stopped_early"] for row in members)
                / len(members),
                "forced_stop_rate": sum(row["forced_stop"] for row in members)
                / len(members),
                "mean_score_margin": sum(row["score_margin"] for row in members)
                / len(members),
            }
        )
    return out


def rows_schema() -> list[str]:
    return [
        "dataset",
        "subject",
        "fold_index",
        "target_fs",
        "mode",
        "stop_rule",
        "stop_threshold",
        "update_policy",
        "update_min_margin",
        "update_scope",
        "update_min_consecutive_winners",
        "update_seconds",
        "window_seconds",
        "max_dwell_seconds",
        "decision_seconds",
        "update_index",
        "stopped_early",
        "forced_stop",
        "winner_score",
        "runner_up_score",
        "score_margin",
        "predicted_class",
        "true_class",
        "correct",
    ]


def main() -> None:
    args = parse_args()
    console = Console()
    profile = resolve_benchmark_profile(args.profile)
    preprocessing = resolve_preprocessing_options(
        profile,
        band_low=args.band_low,
        band_high=args.band_high,
        notch_hz=args.notch_hz,
        drop_first_seconds=args.drop_first_seconds,
    )
    event = resolve_event(profile, args.event)
    onset_event = resolve_onset_event(profile, args.onset_event)
    encoding_length = resolve_encoding_length(profile, args.encoding_length)
    validate_target_fs(args.target_fs)
    subjects = args.subjects or []
    if not subjects:
        from cvep_bench.datasets.loaders import subject_list_for_dataset

        subjects = subject_list_for_dataset("Thielen2021")[: args.max_subjects]
    elif args.max_subjects is not None:
        subjects = subjects[: args.max_subjects]
    fold_indices = resolve_fold_indices(args.folds, args.fold_index)

    results: list[dict[str, Any]] = []
    for dataset in args.datasets:
        full_trial_seconds = trial_seconds_for_dataset(dataset)
        if args.max_dwell_seconds > full_trial_seconds:
            raise ValueError(
                f"max_dwell_seconds={args.max_dwell_seconds} exceeds trial length {full_trial_seconds}"
            )
        for subject in subjects:
            data = load_subject(
                dataset,
                subject,
                args.data_dir,
                args.target_fs,
                preprocessing=preprocessing,
                thielen2021_source=args.thielen2021_source,
            )
            stimulus_fs = stimulus_to_sample_rate(
                data.stimulus, data.presentation_rate, data.fs
            )
            windows = decision_windows(
                trial_samples=data.x.shape[2],
                fs=data.fs,
                window_seconds=args.window_seconds,
                update_seconds=args.update_seconds,
                max_dwell_seconds=args.max_dwell_seconds,
            )
            for fold_idx in fold_indices:
                test_idx = fold_slices(data.x.shape[0], args.folds)[fold_idx]
                x_test = data.x[test_idx]
                y_test = data.y[test_idx]
                banks = {
                    (mode, stop_rule, threshold): initialize_offset_models(
                        windows,
                        stimulus_fs,
                        data.fs,
                        event=event,
                        onset_event=onset_event,
                        encoding_length=encoding_length,
                    )
                    for mode in [MODE_CROSS_TRIAL, MODE_HYBRID]
                    for stop_rule in args.stop_rules
                    for threshold in (
                        [None]
                        if stop_rule == STOP_FIXED_DWELL
                        else args.margin_thresholds
                    )
                }
                for trial_idx in range(x_test.shape[0]):
                    trial = x_test[trial_idx]
                    true_class = int(y_test[trial_idx])
                    for mode in args.modes:
                        for stop_rule in args.stop_rules:
                            thresholds = (
                                [None]
                                if stop_rule == STOP_FIXED_DWELL
                                else args.margin_thresholds
                            )
                            for threshold in thresholds:
                                decision = run_trial(
                                    trial,
                                    true_class,
                                    mode=mode,
                                    stop_rule=stop_rule,
                                    stop_threshold=threshold,
                                    windows=windows,
                                    stimulus=stimulus_fs,
                                    fs=data.fs,
                                    event=event,
                                    onset_event=onset_event,
                                    encoding_length=encoding_length,
                                    persistent_models=banks.get(
                                        (mode, stop_rule, threshold)
                                    ),
                                    update_policy=args.update_policy,
                                    update_min_margin=args.update_min_margin,
                                    update_scope=args.update_scope,
                                    update_min_consecutive_winners=args.update_min_consecutive_winners,
                                )
                                results.append(
                                    {
                                        "dataset": dataset,
                                        "subject": subject,
                                        "fold_index": fold_idx,
                                        "target_fs": data.fs,
                                        "mode": mode,
                                        "stop_rule": stop_rule,
                                        "stop_threshold": threshold,
                                        "update_policy": args.update_policy,
                                        "update_min_margin": args.update_min_margin,
                                        "update_scope": args.update_scope,
                                        "update_min_consecutive_winners": args.update_min_consecutive_winners,
                                        "update_seconds": args.update_seconds,
                                        "window_seconds": args.window_seconds,
                                        "max_dwell_seconds": args.max_dwell_seconds,
                                        "decision_seconds": decision.decision_seconds,
                                        "update_index": decision.update_index,
                                        "stopped_early": decision.stopped_early,
                                        "forced_stop": decision.forced_stop,
                                        "winner_score": decision.winner_score,
                                        "runner_up_score": decision.runner_up_score,
                                        "score_margin": decision.score_margin,
                                        "predicted_class": decision.predicted_class,
                                        "true_class": decision.true_class,
                                        "correct": decision.correct,
                                    }
                                )
                console.print(
                    f"[blue]continuous-cca[/blue] dataset={dataset} subject={subject} fold={fold_idx} fs={data.fs}"
                )
    payload = {
        "config": {
            "profile": profile.name,
            "datasets": args.datasets,
            "subjects": subjects,
            "fold_indices": fold_indices,
            "target_fs": args.target_fs,
            "window_seconds": args.window_seconds,
            "update_seconds": args.update_seconds,
            "max_dwell_seconds": args.max_dwell_seconds,
            "modes": args.modes,
            "stop_rules": args.stop_rules,
            "margin_thresholds": args.margin_thresholds,
            "update_policy": args.update_policy,
            "update_min_margin": args.update_min_margin,
            "update_scope": args.update_scope,
            "update_min_consecutive_winners": args.update_min_consecutive_winners,
            "event": event,
            "onset_event": onset_event,
            "encoding_length": encoding_length,
            "thielen2021_source": args.thielen2021_source,
        },
        "results": results,
    }
    write_json_payload(args.output_json, payload)
    args.output_csv.write_text(rows_to_csv(results, rows_schema()), encoding="utf-8")
    summary = grouped_summary(results)
    render_tabular_html(
        args.output_html,
        title="Continuous-State CCA Prototype",
        subtitle="Short fresh windows with retained synchronized zero-training CCA state.",
        config=payload["config"],
        summary_columns=[
            ("Dataset", "dataset"),
            ("fs", "target_fs"),
            ("Mode", "mode"),
            ("Stop", "stop_rule"),
            ("Threshold", "stop_threshold"),
            ("Update Policy", "update_policy"),
            ("Update Min Margin", "update_min_margin"),
            ("Update Scope", "update_scope"),
            ("Min Winners", "update_min_consecutive_winners"),
            ("Window", "window_seconds"),
            ("Update", "update_seconds"),
            ("Max Dwell", "max_dwell_seconds"),
            ("Subjects", "subjects"),
            ("Accuracy", "mean_accuracy"),
            ("Mean Decision", "mean_decision_seconds"),
            ("Median Decision", "median_decision_seconds"),
            ("Early Stop", "early_stop_rate"),
        ],
        summary_rows=summary,
        detail_columns=[
            ("Dataset", "dataset"),
            ("Subject", "subject"),
            ("Fold", "fold_index"),
            ("Mode", "mode"),
            ("Stop", "stop_rule"),
            ("Threshold", "stop_threshold"),
            ("Update Policy", "update_policy"),
            ("Update Scope", "update_scope"),
            ("Decision Seconds", "decision_seconds"),
            ("Update", "update_index"),
            ("Margin", "score_margin"),
            ("Pred", "predicted_class"),
            ("True", "true_class"),
            ("Correct", "correct"),
        ],
        detail_rows=results,
    )
    render_rich_table(
        console,
        title="Continuous-State CCA Prototype",
        columns=[
            ("Dataset", "dataset"),
            ("fs", "target_fs"),
            ("Mode", "mode"),
            ("Stop", "stop_rule"),
            ("Threshold", "stop_threshold"),
            ("Update Policy", "update_policy"),
            ("Update Scope", "update_scope"),
            ("Accuracy", "mean_accuracy"),
            ("Mean Decision", "mean_decision_seconds"),
            ("Early Stop", "early_stop_rate"),
        ],
        rows=summary,
        formatters={
            "stop_threshold": lambda value: "-" if value is None else f"{value:.3f}",
            "update_min_margin": lambda value: f"{value:.3f}",
            "update_min_consecutive_winners": lambda value: str(value),
            "mean_accuracy": lambda value: f"{value:.4f}",
            "mean_decision_seconds": lambda value: f"{value:.3f}",
            "early_stop_rate": lambda value: f"{value:.4f}",
        },
    )
