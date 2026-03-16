#!/usr/bin/env python3
# /// script
# dependencies = [
#   "numpy",
#   "pyntbci",
#   "rich",
# ]
# ///
"""Reproduce PyNTBCI example_3_etrca.py and compare against the Rust runtime."""

from __future__ import annotations

import argparse
import html
import json
import subprocess
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import numpy as np
from rich.console import Console
from rich.table import Table


WORKSPACE_ROOT = Path(__file__).resolve().parents[3]


@dataclass
class SubjectDataset:
    subject: str
    x: np.ndarray
    y: np.ndarray
    v: np.ndarray
    fs: int


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output-json",
        type=Path,
        default=Path("crates/cvep-decoder/data/example3_etrca_results.json"),
        help="Path for the raw JSON output.",
    )
    parser.add_argument(
        "--output-csv",
        type=Path,
        default=Path("crates/cvep-decoder/data/example3_etrca_results.csv"),
        help="Path for the flattened CSV output.",
    )
    parser.add_argument(
        "--output-html",
        type=Path,
        default=Path("crates/cvep-decoder/data/example3_etrca_results.html"),
        help="Path for the HTML report.",
    )
    parser.add_argument(
        "--n-subjects",
        type=int,
        default=5,
        help="Number of packaged Thielen2021 example subjects to include.",
    )
    parser.add_argument(
        "--n-trials",
        type=int,
        default=100,
        help="Number of trials per subject.",
    )
    parser.add_argument(
        "--trialtime",
        type=float,
        default=4.2,
        help="Trial duration in seconds, matching the example.",
    )
    parser.add_argument(
        "--cycle-size",
        type=float,
        default=2.1,
        help="Code cycle duration in seconds.",
    )
    parser.add_argument(
        "--folds",
        type=int,
        default=5,
        help="Number of chronological folds.",
    )
    parser.add_argument(
        "--segmenttime",
        type=float,
        default=0.1,
        help="Segment step in seconds for the decoding curve.",
    )
    parser.add_argument(
        "--intertrialtime",
        type=float,
        default=1.0,
        help="ITI in seconds for ITR computation.",
    )
    parser.add_argument(
        "--curve-subject",
        type=str,
        default="sub-01",
        help="Subject to use for the learning and decoding curves.",
    )
    parser.add_argument(
        "--adc-bits",
        type=int,
        default=24,
        help="Signed ADC bit depth used to map held-out trials into ADC codes.",
    )
    parser.add_argument(
        "--adc-headroom",
        type=float,
        default=0.95,
        help="Fraction of signed full scale to use when mapping held-out trials into ADC codes.",
    )
    return parser.parse_args()


def packaged_pyntbci_path() -> Path:
    import pyntbci

    return Path(pyntbci.__file__).resolve().parent


def load_subject_dataset(
    subject: str,
    n_trials: int,
    trialtime: float,
) -> SubjectDataset:
    root = packaged_pyntbci_path()
    fn = root / "data" / f"thielen2021_{subject}.npz"
    raw = np.load(fn)
    fs = int(np.asarray(raw["fs"]).item())
    n_samples = int(round(trialtime * fs))
    x = np.asarray(raw["X"], dtype=np.float64)[:n_trials, :, :n_samples]
    y = np.asarray(raw["y"], dtype=np.int64)[:n_trials]
    v = np.asarray(raw["V"], dtype=np.float64)
    return SubjectDataset(subject=subject, x=x, y=y, v=v, fs=fs)


def chronological_folds(n_trials: int, n_folds: int) -> np.ndarray:
    if n_trials % n_folds != 0:
        raise ValueError(f"Expected n_trials divisible by n_folds, got {n_trials=} {n_folds=}")
    return np.repeat(np.arange(n_folds), n_trials // n_folds)


def fit_etrca(x_train: np.ndarray, y_train: np.ndarray, fs: int, cycle_size: float) -> Any:
    import pyntbci

    model = pyntbci.classifiers.eTRCA(
        lags=None,
        fs=fs,
        cycle_size=cycle_size,
        ensemble=True,
    )
    model.fit(x_train, y_train)
    return model


def build_etrca_bank(model: Any, n_samples: int, classes: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    if model.w_.ndim == 2:
        spatial = np.repeat(model.w_[:, :, np.newaxis], classes.shape[0], axis=2)
    else:
        spatial = np.asarray(model.w_)

    if spatial.shape[1] != 1:
        raise ValueError(f"Expected one spatial component, got {spatial.shape}")

    templates = np.asarray(model.get_T(n_samples), dtype=np.float64)[:, 0, :]
    spatial_filters = np.zeros((classes.shape[0], spatial.shape[0]), dtype=np.float64)
    for class_idx in range(classes.shape[0]):
        spatial_filters[class_idx] = spatial[:, 0, class_idx]
    return spatial_filters, templates


def quantize_trials_to_adc(x: np.ndarray, signed_bits: int, headroom: float) -> tuple[np.ndarray, float]:
    adc_peak = float((1 << (signed_bits - 1)) - 1)
    data_peak = float(np.max(np.abs(x)))
    scale = 1.0 if data_peak == 0.0 else (adc_peak * headroom) / data_peak
    quantized = np.rint(x * scale).clip(-adc_peak - 1.0, adc_peak).astype(np.int32)
    return quantized, scale


def build_rust_binary() -> Path:
    subprocess.run(
        [
            "cargo",
            "build",
            "--quiet",
            "-p",
            "cvep-decoder",
            "--bin",
            "projected_correlation_benchmark",
        ],
        check=True,
        cwd=WORKSPACE_ROOT,
    )
    return WORKSPACE_ROOT / "target" / "debug" / "projected_correlation_benchmark"


def run_rust_fixture(fixture_path: Path, rust_binary: Path) -> dict[str, Any]:
    result = subprocess.run(
        [str(rust_binary), str(fixture_path)],
        check=True,
        capture_output=True,
        text=True,
    )
    return json.loads(result.stdout)


def benchmark_subject_fold(
    dataset: SubjectDataset,
    fold_index: int,
    folds: np.ndarray,
    cycle_size: float,
    rust_binary: Path,
    adc_bits: int,
    adc_headroom: float,
) -> dict[str, Any]:
    train_mask = folds != fold_index
    test_mask = ~train_mask
    x_train = dataset.x[train_mask]
    y_train = dataset.y[train_mask]
    x_test = dataset.x[test_mask]
    y_test = dataset.y[test_mask]

    model = fit_etrca(x_train, y_train, dataset.fs, cycle_size)
    pyntbci_pred = np.asarray(model.predict(x_test), dtype=np.int64)
    classes = np.unique(dataset.y)
    spatial_filters, templates = build_etrca_bank(model, dataset.x.shape[2], classes)
    trials_i32, adc_scale = quantize_trials_to_adc(
        x_test, signed_bits=adc_bits, headroom=adc_headroom
    )

    fixture = {
        "algorithm": "etrca",
        "dataset": "pyntbci_example3",
        "subject": int(dataset.subject.split("-")[-1]),
        "classes": int(classes.shape[0]),
        "channels": int(dataset.x.shape[1]),
        "window": int(dataset.x.shape[2]),
        "spatial_filters": spatial_filters.astype(np.float32).tolist(),
        "projected_templates": templates.astype(np.float32).tolist(),
        "benchmark_predictions": pyntbci_pred.astype(np.int64).tolist(),
        "benchmark_labels": y_test.astype(np.int64).tolist(),
        "trials_i32": trials_i32.tolist(),
    }

    with tempfile.TemporaryDirectory(prefix="cvep-example3-") as tmp_dir:
        fixture_path = Path(tmp_dir) / "fixture.json"
        fixture_path.write_text(json.dumps(fixture), encoding="utf-8")
        rust = run_rust_fixture(fixture_path, rust_binary)

    return {
        "subject": dataset.subject,
        "fold_index": fold_index,
        "train_trials": int(x_train.shape[0]),
        "test_trials": int(x_test.shape[0]),
        "pyntbci_accuracy": float(np.mean(pyntbci_pred == y_test)),
        "rust_exact_accuracy": float(rust["rust_exact_accuracy"]),
        "rust_exact_match_rate": float(rust["rust_exact_match_rate"]),
    }


def learning_curve(dataset: SubjectDataset, folds: np.ndarray, cycle_size: float) -> dict[str, Any]:
    import pyntbci

    n_classes = dataset.v.shape[0]
    train_trials = np.arange(n_classes, 1 + np.sum(folds != 0))
    accuracy = np.zeros((np.unique(folds).size, train_trials.size))
    x = dataset.x
    y = dataset.y

    for i_fold in np.unique(folds):
        x_train = x[folds != i_fold]
        y_train = y[folds != i_fold]
        x_test = x[folds == i_fold]
        y_test = y[folds == i_fold]
        for i_trial, n_train in enumerate(train_trials):
            etrca = pyntbci.classifiers.eTRCA(
                lags=None, fs=dataset.fs, cycle_size=cycle_size, ensemble=True
            )
            etrca.fit(x_train[:n_train], y_train[:n_train])
            y_hat = etrca.predict(x_test)
            accuracy[i_fold, i_trial] = float(np.mean(y_hat == y_test))

    return {
        "train_trials": train_trials.astype(np.int64).tolist(),
        "learning_time_seconds": (train_trials * dataset.x.shape[2] / dataset.fs).astype(np.float64).tolist(),
        "accuracy_mean": accuracy.mean(axis=0).astype(np.float64).tolist(),
        "accuracy_std": accuracy.std(axis=0).astype(np.float64).tolist(),
        "chance": float(1.0 / dataset.v.shape[0]),
    }


def decoding_curve(
    dataset: SubjectDataset,
    folds: np.ndarray,
    cycle_size: float,
    segmenttime: float,
    intertrialtime: float,
) -> dict[str, Any]:
    import pyntbci

    trialtime = dataset.x.shape[2] / dataset.fs
    segments = np.arange(segmenttime, trialtime, segmenttime)
    accuracy = np.zeros((np.unique(folds).size, segments.size))
    itr = np.zeros_like(accuracy)

    for i_fold in np.unique(folds):
        x_train = dataset.x[folds != i_fold]
        y_train = dataset.y[folds != i_fold]
        x_test = dataset.x[folds == i_fold]
        y_test = dataset.y[folds == i_fold]
        etrca = pyntbci.classifiers.eTRCA(
            lags=None, fs=dataset.fs, cycle_size=cycle_size, ensemble=True
        )
        etrca.fit(x_train, y_train)
        for i_segment, segment in enumerate(segments):
            y_hat = etrca.predict(x_test[:, :, : int(dataset.fs * segment)])
            accuracy[i_fold, i_segment] = float(np.mean(y_hat == y_test))
        itr[i_fold] = pyntbci.utilities.itr(
            dataset.v.shape[0],
            accuracy[i_fold],
            segments + intertrialtime,
        )

    return {
        "segments_seconds": segments.astype(np.float64).tolist(),
        "accuracy_mean": accuracy.mean(axis=0).astype(np.float64).tolist(),
        "accuracy_std": accuracy.std(axis=0).astype(np.float64).tolist(),
        "itr_mean": itr.mean(axis=0).astype(np.float64).tolist(),
        "itr_std": itr.std(axis=0).astype(np.float64).tolist(),
        "chance": float(1.0 / dataset.v.shape[0]),
    }


def flatten_csv(rows: list[dict[str, Any]]) -> str:
    keys = [
        "subject",
        "fold_index",
        "train_trials",
        "test_trials",
        "pyntbci_accuracy",
        "rust_exact_accuracy",
        "rust_exact_match_rate",
    ]
    lines = [",".join(keys)]
    for row in rows:
        lines.append(",".join(str(row[key]) for key in keys))
    return "\n".join(lines) + "\n"


def render_console(subject_summary: list[dict[str, Any]], overall_avg: float) -> None:
    console = Console()
    table = Table(title="PyNTBCI example_3 eTRCA reproduction")
    table.add_column("Subject")
    table.add_column("Mean PyntBCI")
    table.add_column("Mean Rust exact")
    table.add_column("Mean exact match")
    for row in subject_summary:
        table.add_row(
            row["subject"],
            f"{row['pyntbci_accuracy_mean']:.4f}",
            f"{row['rust_exact_accuracy_mean']:.4f}",
            f"{row['rust_exact_match_rate_mean']:.4f}",
        )
    console.print(table)
    console.print(f"Average accuracy: {overall_avg:.2f}")


def render_html(
    output: Path,
    config: dict[str, Any],
    subject_summary: list[dict[str, Any]],
    fold_rows: list[dict[str, Any]],
    learning: dict[str, Any],
    decoding: dict[str, Any],
    overall_avg: float,
) -> None:
    subject_rows_html = "\n".join(
        (
            "<tr>"
            f"<td>{html.escape(row['subject'])}</td>"
            f"<td>{row['pyntbci_accuracy_mean']:.4f}</td>"
            f"<td>{row['rust_exact_accuracy_mean']:.4f}</td>"
            f"<td>{row['rust_exact_match_rate_mean']:.4f}</td>"
            "</tr>"
        )
        for row in subject_summary
    )
    fold_rows_html = "\n".join(
        (
            "<tr>"
            f"<td>{html.escape(row['subject'])}</td>"
            f"<td>{row['fold_index']}</td>"
            f"<td>{row['pyntbci_accuracy']:.4f}</td>"
            f"<td>{row['rust_exact_accuracy']:.4f}</td>"
            f"<td>{row['rust_exact_match_rate']:.4f}</td>"
            "</tr>"
        )
        for row in fold_rows
    )
    learning_rows_html = "\n".join(
        (
            "<tr>"
            f"<td>{int(train_trials)}</td>"
            f"<td>{seconds:.1f}</td>"
            f"<td>{acc:.4f}</td>"
            f"<td>{std:.4f}</td>"
            "</tr>"
        )
        for train_trials, seconds, acc, std in zip(
            learning["train_trials"],
            learning["learning_time_seconds"],
            learning["accuracy_mean"],
            learning["accuracy_std"],
        )
    )
    decoding_rows_html = "\n".join(
        (
            "<tr>"
            f"<td>{segment:.1f}</td>"
            f"<td>{acc:.4f}</td>"
            f"<td>{std:.4f}</td>"
            f"<td>{itr_mean:.4f}</td>"
            f"<td>{itr_std:.4f}</td>"
            "</tr>"
        )
        for segment, acc, std, itr_mean, itr_std in zip(
            decoding["segments_seconds"],
            decoding["accuracy_mean"],
            decoding["accuracy_std"],
            decoding["itr_mean"],
            decoding["itr_std"],
        )
    )
    config_html = html.escape(json.dumps(config, indent=2))
    document = f"""<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>PyNTBCI example_3 eTRCA reproduction</title>
  <style>
    :root {{
      color-scheme: light;
      --bg: #f3efe6;
      --panel: #fffdf9;
      --ink: #1f2933;
      --muted: #586471;
      --line: #d8cfbf;
      --accent: #0d5c63;
    }}
    body {{
      margin: 0;
      background: linear-gradient(180deg, #efe1cb 0%, var(--bg) 22%, #faf7f1 100%);
      color: var(--ink);
      font-family: Georgia, serif;
    }}
    main {{
      max-width: 1240px;
      margin: 0 auto;
      padding: 28px 18px 48px;
    }}
    .card {{
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 18px;
      padding: 20px;
      margin-bottom: 18px;
      box-shadow: 0 12px 36px rgba(52, 39, 18, 0.08);
    }}
    table {{ width: 100%; border-collapse: collapse; font-family: Menlo, monospace; font-size: 0.88rem; }}
    th, td {{ padding: 10px 12px; border-bottom: 1px solid var(--line); text-align: left; }}
    th {{ background: #faf5ec; color: var(--accent); position: sticky; top: 0; }}
    .table-wrap {{ overflow: auto; border: 1px solid var(--line); border-radius: 14px; }}
    pre {{ margin: 0; overflow: auto; background: #fbf8f1; border: 1px solid var(--line); border-radius: 12px; padding: 14px; }}
    .big {{ font-size: 2rem; font-weight: 700; color: var(--accent); }}
  </style>
</head>
<body>
  <main>
    <section class="card">
      <h1>PyNTBCI example_3 eTRCA reproduction</h1>
      <p>Reference benchmark on PyNTBCI packaged Thielen2021 data, with Rust parity replay.</p>
      <div class="big">Average accuracy: {overall_avg:.2f}</div>
    </section>
    <section class="card"><h2>Subject summary</h2><div class="table-wrap"><table><thead><tr><th>Subject</th><th>PyntBCI</th><th>Rust exact</th><th>Exact match</th></tr></thead><tbody>{subject_rows_html}</tbody></table></div></section>
    <section class="card"><h2>Fold rows</h2><div class="table-wrap"><table><thead><tr><th>Subject</th><th>Fold</th><th>PyntBCI</th><th>Rust exact</th><th>Exact match</th></tr></thead><tbody>{fold_rows_html}</tbody></table></div></section>
    <section class="card"><h2>Learning curve</h2><div class="table-wrap"><table><thead><tr><th>Train trials</th><th>Learning time [s]</th><th>Accuracy mean</th><th>Accuracy std</th></tr></thead><tbody>{learning_rows_html}</tbody></table></div></section>
    <section class="card"><h2>Decoding curve</h2><div class="table-wrap"><table><thead><tr><th>Decoding time [s]</th><th>Accuracy mean</th><th>Accuracy std</th><th>ITR mean</th><th>ITR std</th></tr></thead><tbody>{decoding_rows_html}</tbody></table></div></section>
    <section class="card"><h2>Config</h2><pre>{config_html}</pre></section>
  </main>
</body>
</html>
"""
    output.write_text(document, encoding="utf-8")


def main() -> None:
    args = parse_args()
    rust_binary = build_rust_binary()
    folds = chronological_folds(args.n_trials, args.folds)

    subjects = [f"sub-{1 + i:02d}" for i in range(args.n_subjects)]
    datasets = [load_subject_dataset(subject, args.n_trials, args.trialtime) for subject in subjects]

    fold_rows: list[dict[str, Any]] = []
    for dataset in datasets:
        for fold_index in range(args.folds):
            fold_rows.append(
                benchmark_subject_fold(
                    dataset,
                    fold_index,
                    folds,
                    args.cycle_size,
                    rust_binary,
                    args.adc_bits,
                    args.adc_headroom,
                )
            )

    subject_summary = []
    for subject in subjects:
        rows = [row for row in fold_rows if row["subject"] == subject]
        subject_summary.append(
            {
                "subject": subject,
                "pyntbci_accuracy_mean": float(np.mean([row["pyntbci_accuracy"] for row in rows])),
                "rust_exact_accuracy_mean": float(np.mean([row["rust_exact_accuracy"] for row in rows])),
                "rust_exact_match_rate_mean": float(np.mean([row["rust_exact_match_rate"] for row in rows])),
            }
        )

    overall_avg = float(np.mean([row["pyntbci_accuracy_mean"] for row in subject_summary]))

    curve_dataset = next(dataset for dataset in datasets if dataset.subject == args.curve_subject)
    learning = learning_curve(curve_dataset, folds, args.cycle_size)
    decoding = decoding_curve(
        curve_dataset,
        folds,
        args.cycle_size,
        args.segmenttime,
        args.intertrialtime,
    )

    config = {
        "n_subjects": args.n_subjects,
        "subjects": subjects,
        "n_trials": args.n_trials,
        "trialtime": args.trialtime,
        "cycle_size": args.cycle_size,
        "folds": args.folds,
        "segmenttime": args.segmenttime,
        "intertrialtime": args.intertrialtime,
        "curve_subject": args.curve_subject,
        "adc_bits": args.adc_bits,
        "adc_headroom": args.adc_headroom,
        "fs": datasets[0].fs,
        "channels": int(datasets[0].x.shape[1]),
        "classes": int(datasets[0].v.shape[0]),
        "window": int(datasets[0].x.shape[2]),
    }

    payload = {
        "config": config,
        "fold_rows": fold_rows,
        "subject_summary": subject_summary,
        "overall_average_accuracy": overall_avg,
        "learning_curve": learning,
        "decoding_curve": decoding,
    }

    args.output_json.parent.mkdir(parents=True, exist_ok=True)
    args.output_csv.parent.mkdir(parents=True, exist_ok=True)
    args.output_html.parent.mkdir(parents=True, exist_ok=True)
    args.output_json.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    args.output_csv.write_text(flatten_csv(fold_rows), encoding="utf-8")
    render_html(
        args.output_html,
        config,
        subject_summary,
        fold_rows,
        learning,
        decoding,
        overall_avg,
    )
    render_console(subject_summary, overall_avg)
    console = Console()
    console.print(f"[green]wrote[/green] {args.output_json}")
    console.print(f"[green]wrote[/green] {args.output_csv}")
    console.print(f"[green]wrote[/green] {args.output_html}")


if __name__ == "__main__":
    main()
