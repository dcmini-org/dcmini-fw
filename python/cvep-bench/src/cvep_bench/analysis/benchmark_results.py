from __future__ import annotations

import argparse
import csv
import html
import json
from collections import defaultdict
from pathlib import Path
from typing import Any

import numpy as np
from rich.console import Console
from rich.table import Table
from scipy import stats

from cvep_bench.benchmarks.pyntbci_vs_rust import DEFAULT_DATA_DIR


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--input-json", type=Path, default=DEFAULT_DATA_DIR / "benchmark_results.json"
    )
    parser.add_argument(
        "--output-json", type=Path, default=DEFAULT_DATA_DIR / "benchmark_analysis.json"
    )
    parser.add_argument(
        "--output-csv", type=Path, default=DEFAULT_DATA_DIR / "benchmark_analysis.csv"
    )
    parser.add_argument(
        "--output-html", type=Path, default=DEFAULT_DATA_DIR / "benchmark_analysis.html"
    )
    parser.add_argument("--bootstrap-samples", type=int, default=5000)
    parser.add_argument("--seed", type=int, default=42)
    return parser.parse_args()


def mean_ci_bootstrap(
    values: np.ndarray, samples: int, rng: np.random.Generator
) -> tuple[float, float]:
    if values.size == 0:
        return float("nan"), float("nan")
    if values.size == 1:
        value = float(values[0])
        return value, value
    draws = rng.choice(values, size=(samples, values.size), replace=True)
    means = draws.mean(axis=1)
    low, high = np.quantile(means, [0.025, 0.975])
    return float(low), float(high)


def safe_std(values: np.ndarray) -> float:
    return 0.0 if values.size <= 1 else float(np.std(values, ddof=1))


def paired_tests(values: np.ndarray) -> dict[str, float | int | None]:
    values = np.asarray(values, dtype=np.float64)
    out: dict[str, float | int | None] = {
        "n": int(values.size),
        "mean_delta": float(np.mean(values)) if values.size else None,
        "t_stat": None,
        "t_pvalue": None,
        "wilcoxon_stat": None,
        "wilcoxon_pvalue": None,
        "cohens_dz": None,
    }
    if values.size >= 2 and not np.allclose(values, values[0]):
        t_res = stats.ttest_1samp(values, popmean=0.0)
        out["t_stat"] = float(t_res.statistic)
        out["t_pvalue"] = float(t_res.pvalue)
        std = np.std(values, ddof=1)
        if std > 0.0:
            out["cohens_dz"] = float(np.mean(values) / std)
    elif values.size >= 2:
        out["t_stat"] = 0.0
        out["t_pvalue"] = 1.0
        out["cohens_dz"] = 0.0
    nonzero = values[~np.isclose(values, 0.0)]
    if nonzero.size >= 1:
        try:
            w_res = stats.wilcoxon(
                values, zero_method="wilcox", alternative="two-sided"
            )
            out["wilcoxon_stat"] = float(w_res.statistic)
            out["wilcoxon_pvalue"] = float(w_res.pvalue)
        except ValueError:
            pass
    return out


def aggregate_subject_rows(results: list[dict[str, Any]]) -> list[dict[str, Any]]:
    grouped: dict[tuple[str, str, int, int, int], list[dict[str, Any]]] = defaultdict(
        list
    )
    for row in results:
        grouped[
            (
                row["dataset"],
                row["algorithm"],
                row["target_fs"],
                row["window"],
                row["subject"],
            )
        ] += [row]
    subject_rows = []
    for (dataset, algorithm, target_fs, window, subject), rows in sorted(
        grouped.items()
    ):
        ordered = sorted(rows, key=lambda row: row["fold_index"])
        subject_rows.append(
            {
                "dataset": dataset,
                "algorithm": algorithm,
                "target_fs": target_fs,
                "subject": subject,
                "n_folds": len(ordered),
                "classes": ordered[0]["classes"],
                "channels": ordered[0]["channels"],
                "window": window,
                "requested_window_seconds": ordered[0]["requested_window_seconds"],
                "window_seconds": ordered[0]["window_seconds"],
                "pyntbci_accuracy_mean": float(
                    np.mean([row["pyntbci_accuracy"] for row in ordered])
                ),
                "rust_exact_accuracy_mean": float(
                    np.mean([row["rust_exact_accuracy"] for row in ordered])
                ),
                "rust_exact_match_rate_mean": float(
                    np.mean([row["rust_exact_match_rate"] for row in ordered])
                ),
            }
        )
    return subject_rows


def grouped_statistics(
    subject_rows: list[dict[str, Any]], bootstrap_samples: int, seed: int
) -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
    grouped: dict[tuple[str, str, int, int], list[dict[str, Any]]] = defaultdict(list)
    for row in subject_rows:
        grouped[
            (row["dataset"], row["algorithm"], row["target_fs"], row["window"])
        ] += [row]
    summary_rows: list[dict[str, Any]] = []
    test_rows: list[dict[str, Any]] = []
    for (dataset, algorithm, target_fs, window), rows in sorted(grouped.items()):
        pynt = np.asarray(
            [row["pyntbci_accuracy_mean"] for row in rows], dtype=np.float64
        )
        exact = np.asarray(
            [row["rust_exact_accuracy_mean"] for row in rows], dtype=np.float64
        )
        match = np.asarray(
            [row["rust_exact_match_rate_mean"] for row in rows], dtype=np.float64
        )
        rng = np.random.default_rng(seed)
        pynt_ci = mean_ci_bootstrap(pynt, bootstrap_samples, rng)
        exact_ci = mean_ci_bootstrap(exact, bootstrap_samples, rng)
        match_ci = mean_ci_bootstrap(match, bootstrap_samples, rng)
        summary_rows.append(
            {
                "dataset": dataset,
                "algorithm": algorithm,
                "target_fs": target_fs,
                "subjects": len(rows),
                "classes": rows[0]["classes"],
                "channels": rows[0]["channels"],
                "window": window,
                "requested_window_seconds": rows[0]["requested_window_seconds"],
                "window_seconds": rows[0]["window_seconds"],
                "pyntbci_accuracy_mean": float(np.mean(pynt)),
                "pyntbci_accuracy_std": safe_std(pynt),
                "pyntbci_accuracy_ci_low": pynt_ci[0],
                "pyntbci_accuracy_ci_high": pynt_ci[1],
                "rust_exact_accuracy_mean": float(np.mean(exact)),
                "rust_exact_accuracy_std": safe_std(exact),
                "rust_exact_accuracy_ci_low": exact_ci[0],
                "rust_exact_accuracy_ci_high": exact_ci[1],
                "rust_exact_match_rate_mean": float(np.mean(match)),
                "rust_exact_match_rate_std": safe_std(match),
                "rust_exact_match_rate_ci_low": match_ci[0],
                "rust_exact_match_rate_ci_high": match_ci[1],
            }
        )
        test_rows.append(
            {
                "dataset": dataset,
                "algorithm": algorithm,
                "target_fs": target_fs,
                "window": window,
                "requested_window_seconds": rows[0]["requested_window_seconds"],
                "window_seconds": rows[0]["window_seconds"],
                "comparison": "rust_exact_minus_pyntbci",
                **paired_tests(exact - pynt),
            }
        )
    return summary_rows, test_rows


def write_csv(path: Path, rows: list[dict[str, Any]]) -> None:
    if not rows:
        path.write_text("", encoding="utf-8")
        return
    with path.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=list(rows[0].keys()))
        writer.writeheader()
        writer.writerows(rows)


def render_html(output: Path, payload: dict[str, Any]) -> None:
    summary_rows = "\n".join(
        (
            "<tr>"
            f"<td>{html.escape(row['dataset'])}</td><td>{html.escape(row['algorithm'])}</td><td>{row['target_fs']}</td><td>{row['requested_window_seconds']:.3f}</td><td>{row['pyntbci_accuracy_mean']:.4f}</td><td>{row['rust_exact_accuracy_mean']:.4f}</td><td>{row['rust_exact_match_rate_mean']:.4f}</td>"
            "</tr>"
        )
        for row in payload["summary_rows"]
    )
    output.write_text(
        f"<!doctype html><html lang='en'><body><pre>{html.escape(json.dumps(payload['config'], indent=2))}</pre><table><tbody>{summary_rows}</tbody></table></body></html>",
        encoding="utf-8",
    )


def main() -> None:
    args = parse_args()
    payload = json.loads(args.input_json.read_text(encoding="utf-8"))
    results = payload["results"]
    subject_rows = aggregate_subject_rows(results)
    summary_rows, test_rows = grouped_statistics(
        subject_rows, args.bootstrap_samples, args.seed
    )
    analysis = {
        "config": {
            "input_json": str(args.input_json),
            "bootstrap_samples": args.bootstrap_samples,
            "seed": args.seed,
        },
        "summary_rows": summary_rows,
        "test_rows": test_rows,
    }
    args.output_json.write_text(json.dumps(analysis, indent=2) + "\n", encoding="utf-8")
    write_csv(args.output_csv, summary_rows)
    render_html(args.output_html, analysis)
    table = Table(title="Benchmark Analysis Summary")
    for col in [
        "dataset",
        "algorithm",
        "fs",
        "window",
        "pyntbci",
        "rust_exact",
        "match",
    ]:
        table.add_column(col)
    for row in summary_rows:
        table.add_row(
            row["dataset"],
            row["algorithm"],
            str(row["target_fs"]),
            f"{row['requested_window_seconds']:.3f}",
            f"{row['pyntbci_accuracy_mean']:.4f}",
            f"{row['rust_exact_accuracy_mean']:.4f}",
            f"{row['rust_exact_match_rate_mean']:.4f}",
        )
    Console().print(table)
