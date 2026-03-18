from __future__ import annotations

import argparse
import html
import json
import tempfile
from pathlib import Path
from typing import Any

import numpy as np
from rich.console import Console
from rich.table import Table

from cvep_bench.algorithms.pyntbci_models import (
    build_etrca_bank,
    build_rcca_bank,
    fit_etrca,
    fit_rcca,
    quantize_trials_to_adc,
)
from cvep_bench.datasets.loaders import (
    DEFAULT_DATASETS,
    effective_etrca_cycle_size,
    load_subject,
    subject_list_for_dataset,
    trial_seconds_for_dataset,
    validate_dataset_algorithm_target_fs,
    validate_target_fs,
)
from cvep_bench.datasets.models import PreprocessingOptions, SubjectData
from cvep_bench.datasets.profiles import (
    benchmark_profile_names,
    resolve_benchmark_profile,
    resolve_encoding_length,
    resolve_event,
    resolve_preprocessing_options,
    resolve_target_fs,
    resolve_window_grid,
)
from cvep_bench.datasets.windows import (
    decode_window_requests,
    fold_slices,
    loader_trial_seconds_for_algorithm,
    slice_windowed_trials_and_stimulus,
    stimulus_to_sample_rate,
)
from cvep_bench.runtime.cargo import WORKSPACE_ROOT, build_rust_binary
from cvep_bench.runtime.fixtures import run_rust_fixture


DEFAULT_DATA_DIR = WORKSPACE_ROOT / "crates" / "cvep-decoder" / "data"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--profile", choices=benchmark_profile_names(), default="legacy"
    )
    parser.add_argument("--data-dir", type=Path, default=DEFAULT_DATA_DIR)
    parser.add_argument(
        "--output-json", type=Path, default=DEFAULT_DATA_DIR / "benchmark_results.json"
    )
    parser.add_argument(
        "--output-csv", type=Path, default=DEFAULT_DATA_DIR / "benchmark_results.csv"
    )
    parser.add_argument(
        "--output-html", type=Path, default=DEFAULT_DATA_DIR / "benchmark_results.html"
    )
    parser.add_argument(
        "--algorithms", nargs="+", choices=["etrca", "rcca"], default=["etrca", "rcca"]
    )
    parser.add_argument("--datasets", nargs="+", default=DEFAULT_DATASETS)
    parser.add_argument("--subjects", type=int, nargs="+", default=None)
    parser.add_argument("--max-subjects", type=int, default=None)
    parser.add_argument("--folds", type=int, default=5)
    parser.add_argument("--fold-index", type=int, nargs="+", default=None)
    parser.add_argument("--target-fs", type=int, default=None)
    parser.add_argument("--target-fs-grid", type=int, nargs="+", default=None)
    parser.add_argument("--window-step-seconds", type=float, default=None)
    parser.add_argument("--window-seconds-grid", type=float, nargs="+", default=None)
    parser.add_argument("--adc-bits", type=int, default=24)
    parser.add_argument("--adc-headroom", type=float, default=0.95)
    parser.add_argument("--encoding-length", type=float, default=None)
    parser.add_argument("--event", type=str, default=None)
    parser.add_argument(
        "--thielen2021-source", choices=["raw", "packaged"], default="raw"
    )
    parser.add_argument("--band-low", type=float, default=None)
    parser.add_argument("--band-high", type=float, default=None)
    parser.add_argument("--notch-hz", type=float, default=None)
    parser.add_argument("--drop-first-seconds", type=float, default=None)
    parser.add_argument("--skip-rust", action="store_true")
    return parser.parse_args()


def benchmark_subject_fold_windows(
    algorithm: str,
    data: SubjectData,
    rust_binary: Path | None,
    fold_idx: int,
    folds: int,
    window_requests_seconds: list[float],
    adc_bits: int,
    adc_headroom: float,
    encoding_length: float,
    event: str,
    preprocessing: PreprocessingOptions,
    profile_name: str,
) -> list[dict[str, Any]]:
    classes = np.unique(data.y)
    fold_parts = fold_slices(data.x.shape[0], folds)
    test_idx = fold_parts[fold_idx]
    train_idx = np.concatenate(
        [part for idx, part in enumerate(fold_parts) if idx != fold_idx]
    )
    x_train = data.x[train_idx]
    y_train = data.y[train_idx]
    x_test = data.x[test_idx]
    y_test = data.y[test_idx]
    stimulus_fs = stimulus_to_sample_rate(
        data.stimulus, presentation_rate=data.presentation_rate, fs=data.fs
    )
    etrca_model = None
    if algorithm == "etrca":
        etrca_model = fit_etrca(
            x_train,
            y_train,
            data.fs,
            effective_etrca_cycle_size(data.cycle_size, data.fs),
        )
    rows: list[dict[str, Any]] = []
    for requested_window_seconds in window_requests_seconds:
        x_train_window, stimulus_window, window_info = (
            slice_windowed_trials_and_stimulus(
                x_train,
                stimulus_fs,
                data.fs,
                data.fs,
                requested_window_seconds,
                preprocessing.drop_first_seconds,
            )
        )
        x_test_window, _stimulus_unused, _ = slice_windowed_trials_and_stimulus(
            x_test,
            stimulus_fs,
            data.fs,
            data.fs,
            requested_window_seconds,
            preprocessing.drop_first_seconds,
        )
        window_samples = int(window_info["effective_window_samples"])
        if algorithm == "etrca":
            assert etrca_model is not None
            model = etrca_model
            spatial_filters, templates = build_etrca_bank(
                model, window_samples, classes
            )
        elif algorithm == "rcca":
            model = fit_rcca(
                x_train_window,
                y_train,
                stimulus_window,
                data.fs,
                event=event,
                encoding_length=encoding_length,
            )
            spatial_filters, templates = build_rcca_bank(
                model,
                n_classes=stimulus_window.shape[0],
                n_channels=data.x.shape[1],
                n_samples=window_samples,
            )
        else:
            raise ValueError(f"Unsupported algorithm {algorithm}")
        benchmark_predictions = np.asarray(model.predict(x_test_window), dtype=np.int64)
        quantized_trials, _adc_scale = quantize_trials_to_adc(
            x_test_window, signed_bits=adc_bits, headroom=adc_headroom
        )
        fixture = {
            "algorithm": algorithm,
            "dataset": data.dataset,
            "subject": data.subject,
            "classes": int(classes.shape[0]),
            "channels": int(data.x.shape[1]),
            "window": int(window_samples),
            "spatial_filters": spatial_filters.astype(np.float32).tolist(),
            "projected_templates": templates.astype(np.float32).tolist(),
            "benchmark_predictions": benchmark_predictions.astype(np.int64).tolist(),
            "benchmark_labels": y_test.astype(np.int64).tolist(),
            "trials_i32": quantized_trials.tolist(),
        }
        rust = None
        if rust_binary is not None:
            with tempfile.TemporaryDirectory(prefix="cvep-benchmark-") as tmp_dir:
                fixture_path = Path(tmp_dir) / "fixture.json"
                fixture_path.write_text(json.dumps(fixture), encoding="utf-8")
                rust = run_rust_fixture(fixture_path, rust_binary)
        rows.append(
            {
                "algorithm": algorithm,
                "dataset": data.dataset,
                "subject": data.subject,
                "fold_index": fold_idx,
                "folds": folds,
                "classes": fixture["classes"],
                "channels": fixture["channels"],
                "target_fs": data.fs,
                "profile": profile_name,
                "cycle_size_seconds": effective_etrca_cycle_size(
                    data.cycle_size, data.fs
                ),
                "train_window_seconds": data.trial_seconds,
                "requested_window_seconds": requested_window_seconds,
                "window_seconds": float(window_info["nominal_window_seconds"]),
                "effective_window_seconds": float(
                    window_info["effective_window_seconds"]
                ),
                "leading_trim_seconds": float(window_info["leading_trim_seconds"]),
                "window": fixture["window"],
                "train_trials": int(x_train.shape[0]),
                "test_trials": int(x_test.shape[0]),
                "band_low": preprocessing.band_low,
                "band_high": preprocessing.band_high,
                "notch_hz": preprocessing.notch_hz,
                "pyntbci_accuracy": float(np.mean(benchmark_predictions == y_test)),
                "rust_exact_accuracy": None
                if rust is None
                else float(rust["rust_exact_accuracy"]),
                "rust_exact_match_rate": None
                if rust is None
                else float(rust["rust_exact_match_rate"]),
            }
        )
    return rows


def flatten_results_csv(results: list[dict[str, Any]]) -> str:
    keys = [
        "algorithm",
        "dataset",
        "subject",
        "fold_index",
        "folds",
        "classes",
        "channels",
        "profile",
        "target_fs",
        "band_low",
        "band_high",
        "notch_hz",
        "train_window_seconds",
        "requested_window_seconds",
        "window_seconds",
        "effective_window_seconds",
        "leading_trim_seconds",
        "window",
        "train_trials",
        "test_trials",
        "pyntbci_accuracy",
        "rust_exact_accuracy",
        "rust_exact_match_rate",
    ]
    lines = [",".join(keys)]
    for row in results:
        lines.append(
            ",".join("" if row.get(key) is None else str(row[key]) for key in keys)
        )
    return "\n".join(lines) + "\n"


def mean_or_none(values: list[float | None]) -> float | None:
    filtered = [value for value in values if value is not None]
    return None if not filtered else float(np.mean(filtered))


def fmt_metric(value: float | None) -> str:
    return "-" if value is None else f"{value:.4f}"


def grouped_summary_rows(results: list[dict[str, Any]]) -> list[dict[str, Any]]:
    grouped: dict[tuple[str, str, str, int, int], list[dict[str, Any]]] = {}
    for row in results:
        grouped.setdefault(
            (
                row["algorithm"],
                row["dataset"],
                row.get("profile", "legacy"),
                row["target_fs"],
                row["window"],
            ),
            [],
        ).append(row)
    out = []
    for (algorithm, dataset, profile, target_fs, window), rows in sorted(
        grouped.items()
    ):
        out.append(
            {
                "algorithm": algorithm,
                "dataset": dataset,
                "profile": profile,
                "target_fs": target_fs,
                "window": window,
                "window_seconds": rows[0]["window_seconds"],
                "effective_window_seconds": rows[0].get("effective_window_seconds"),
                "requested_window_seconds": rows[0]["requested_window_seconds"],
                "subjects": len({row["subject"] for row in rows}),
                "mean_pyntbci_accuracy": float(
                    np.mean([row["pyntbci_accuracy"] for row in rows])
                ),
                "mean_rust_exact_accuracy": mean_or_none(
                    [row.get("rust_exact_accuracy") for row in rows]
                ),
                "mean_rust_exact_match_rate": mean_or_none(
                    [row.get("rust_exact_match_rate") for row in rows]
                ),
            }
        )
    return out


def render_html_report(
    output: Path, config: dict[str, Any], results: list[dict[str, Any]]
) -> None:
    summary = grouped_summary_rows(results)
    summary_rows = "\n".join(
        (
            "<tr>"
            f"<td>{html.escape(row['algorithm'])}</td>"
            f"<td>{html.escape(row['dataset'])}</td>"
            f"<td>{row['target_fs']}</td>"
            f"<td>{row['requested_window_seconds']:.3f}</td>"
            f"<td>{row['window_seconds']:.3f}</td>"
            f"<td>{row['subjects']}</td>"
            f"<td>{row['mean_pyntbci_accuracy']:.4f}</td>"
            f"<td>{fmt_metric(row['mean_rust_exact_accuracy'])}</td>"
            f"<td>{fmt_metric(row['mean_rust_exact_match_rate'])}</td>"
            "</tr>"
        )
        for row in summary
    )
    detail_rows = "\n".join(
        (
            "<tr>"
            f"<td>{html.escape(row['algorithm'])}</td>"
            f"<td>{html.escape(row['dataset'])}</td>"
            f"<td>{row['target_fs']}</td>"
            f"<td>{row['subject']}</td>"
            f"<td>{row['fold_index']}</td>"
            f"<td>{row['train_trials']}</td>"
            f"<td>{row['test_trials']}</td>"
            f"<td>{row['classes']}</td>"
            f"<td>{row['channels']}</td>"
            f"<td>{row['requested_window_seconds']:.3f}</td>"
            f"<td>{row['window_seconds']:.3f}</td>"
            f"<td>{row['window']}</td>"
            f"<td>{row['pyntbci_accuracy']:.4f}</td>"
            f"<td>{fmt_metric(row['rust_exact_accuracy'])}</td>"
            f"<td>{fmt_metric(row['rust_exact_match_rate'])}</td>"
            "</tr>"
        )
        for row in results
    )
    config_html = html.escape(json.dumps(config, indent=2))
    output.write_text(
        f"<!doctype html><html lang='en'><head><meta charset='utf-8'><meta name='viewport' content='width=device-width, initial-scale=1'><title>CVEP Benchmark Report</title></head><body><main><section><h1>CVEP Benchmark Report</h1><pre>{config_html}</pre></section><section><table><thead><tr><th>Algorithm</th><th>Dataset</th><th>fs</th><th>Requested s</th><th>Actual s</th><th>Subjects</th><th>Mean PyntBCI</th><th>Mean Rust exact</th><th>Mean exact match</th></tr></thead><tbody>{summary_rows}</tbody></table></section><section><table><thead><tr><th>Algorithm</th><th>Dataset</th><th>fs</th><th>Subject</th><th>Fold</th><th>Train</th><th>Test</th><th>Classes</th><th>Channels</th><th>Requested s</th><th>Actual s</th><th>Window</th><th>PyntBCI</th><th>Rust exact</th><th>Exact match</th></tr></thead><tbody>{detail_rows}</tbody></table></section></main></body></html>",
        encoding="utf-8",
    )


def render_summary(console: Console, results: list[dict[str, Any]]) -> None:
    table = Table(title="PyntBCI vs Rust benchmark summary")
    for col in [
        "Algorithm",
        "Dataset",
        "fs",
        "Req s",
        "Actual s",
        "Subjects",
        "Mean PyntBCI",
        "Mean Rust exact",
        "Mean exact match",
    ]:
        table.add_column(col)
    for row in grouped_summary_rows(results):
        table.add_row(
            row["algorithm"],
            row["dataset"],
            str(row["target_fs"]),
            f"{row['requested_window_seconds']:.3f}",
            f"{row['window_seconds']:.3f}",
            str(row["subjects"]),
            f"{row['mean_pyntbci_accuracy']:.4f}",
            fmt_metric(row["mean_rust_exact_accuracy"]),
            fmt_metric(row["mean_rust_exact_match_rate"]),
        )
    console.print(table)


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
    encoding_length = resolve_encoding_length(profile, args.encoding_length)
    event = resolve_event(profile, args.event)
    rust_binary = (
        None if args.skip_rust else build_rust_binary("projected_correlation_benchmark")
    )
    target_fs_grid = args.target_fs_grid or [resolve_target_fs(profile, args.target_fs)]
    for target_fs in target_fs_grid:
        validate_target_fs(target_fs)
    for output in [args.output_json, args.output_csv, args.output_html]:
        output.parent.mkdir(parents=True, exist_ok=True)
    fold_indices = (
        args.fold_index if args.fold_index is not None else list(range(args.folds))
    )
    results: list[dict[str, Any]] = []
    resolved_window_grid = args.window_seconds_grid
    for dataset in args.datasets:
        subjects = args.subjects or subject_list_for_dataset(dataset)
        if args.max_subjects is not None:
            subjects = subjects[: args.max_subjects]
        for target_fs in target_fs_grid:
            for algorithm in args.algorithms:
                validate_dataset_algorithm_target_fs(dataset, algorithm, target_fs)
        for subject in subjects:
            for target_fs in target_fs_grid:
                full_trial_seconds = trial_seconds_for_dataset(dataset)
                resolved_window_grid = resolve_window_grid(
                    profile, dataset, args.window_seconds_grid, args.window_step_seconds
                )
                window_requests_seconds = decode_window_requests(
                    full_trial_seconds,
                    explicit=resolved_window_grid,
                    step_seconds=args.window_step_seconds,
                )
                data_cache: dict[tuple[str, float | None], SubjectData] = {}
                for algorithm in args.algorithms:
                    grouped_windows = [window_requests_seconds]
                    use_direct_window_trials = (
                        loader_trial_seconds_for_algorithm(
                            dataset,
                            algorithm,
                            requested_window_seconds=full_trial_seconds,
                        )
                        is not None
                    )
                    if use_direct_window_trials:
                        grouped_windows = [
                            [window] for window in window_requests_seconds
                        ]
                    for windows in grouped_windows:
                        load_seconds = (
                            loader_trial_seconds_for_algorithm(
                                dataset, algorithm, requested_window_seconds=windows[0]
                            )
                            if use_direct_window_trials
                            else None
                        )
                        cache_key = (algorithm, load_seconds)
                        if cache_key not in data_cache:
                            data_cache[cache_key] = load_subject(
                                dataset,
                                subject,
                                args.data_dir,
                                target_fs,
                                trial_seconds=load_seconds,
                                preprocessing=preprocessing,
                                thielen2021_source=args.thielen2021_source,
                            )
                        data = data_cache[cache_key]
                        for fold_idx in fold_indices:
                            results.extend(
                                benchmark_subject_fold_windows(
                                    algorithm,
                                    data,
                                    rust_binary=rust_binary,
                                    fold_idx=fold_idx,
                                    folds=args.folds,
                                    window_requests_seconds=windows,
                                    adc_bits=args.adc_bits,
                                    adc_headroom=args.adc_headroom,
                                    encoding_length=encoding_length,
                                    event=event,
                                    preprocessing=preprocessing,
                                    profile_name=profile.name,
                                )
                            )
    payload = {
        "config": {
            "datasets": args.datasets,
            "profile": profile.name,
            "profile_description": profile.description,
            "algorithms": args.algorithms,
            "folds": args.folds,
            "fold_indices": fold_indices,
            "target_fs_grid": target_fs_grid,
            "window_seconds_grid": resolved_window_grid,
            "window_step_seconds": args.window_step_seconds,
            "adc_bits": args.adc_bits,
            "adc_headroom": args.adc_headroom,
            "encoding_length": encoding_length,
            "event": event,
            "preprocessing": {
                "band_low": preprocessing.band_low,
                "band_high": preprocessing.band_high,
                "notch_hz": preprocessing.notch_hz,
                "drop_first_seconds": preprocessing.drop_first_seconds,
            },
            "skip_rust": args.skip_rust,
            "thielen2021_source": args.thielen2021_source,
        },
        "results": results,
    }
    args.output_json.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    args.output_csv.write_text(flatten_results_csv(results), encoding="utf-8")
    render_html_report(args.output_html, payload["config"], results)
    render_summary(console, results)
