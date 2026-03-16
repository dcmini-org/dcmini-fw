#!/usr/bin/env python3
# /// script
# dependencies = [
#     "matplotlib>=3.10.7",
#     "numpy>=2.2.6",
#     "rich>=14.3.3",
#     "scipy>=1.15.3",
# ]
# ///
"""Analyze aggregate statistics from c-VEP benchmark results."""

from __future__ import annotations

import argparse
import html
import io
import json
import math
from collections import defaultdict
from pathlib import Path
from typing import Any

import matplotlib
import numpy as np
from rich.console import Console
from rich.table import Table
from scipy import stats

matplotlib.use("Agg")

import matplotlib.pyplot as plt


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--input-json",
        type=Path,
        default=Path("crates/cvep-decoder/data/benchmark_results.json"),
        help="Benchmark JSON produced by benchmark_pyntbci_vs_rust.py.",
    )
    parser.add_argument(
        "--output-json",
        type=Path,
        default=Path("crates/cvep-decoder/data/benchmark_analysis.json"),
        help="Path for the full analysis JSON.",
    )
    parser.add_argument(
        "--output-csv",
        type=Path,
        default=Path("crates/cvep-decoder/data/benchmark_analysis.csv"),
        help="Path for the grouped summary CSV.",
    )
    parser.add_argument(
        "--output-html",
        type=Path,
        default=Path("crates/cvep-decoder/data/benchmark_analysis.html"),
        help="Path for the HTML analysis report.",
    )
    parser.add_argument(
        "--bootstrap-samples",
        type=int,
        default=5000,
        help="Number of bootstrap resamples for mean confidence intervals.",
    )
    parser.add_argument(
        "--seed",
        type=int,
        default=42,
        help="Random seed for bootstrap confidence intervals.",
    )
    return parser.parse_args()


def mean_ci_bootstrap(
    values: np.ndarray,
    samples: int,
    rng: np.random.Generator,
) -> tuple[float, float]:
    if values.size == 0:
        return (float("nan"), float("nan"))
    if values.size == 1:
        value = float(values[0])
        return (value, value)
    draws = rng.choice(values, size=(samples, values.size), replace=True)
    means = draws.mean(axis=1)
    low, high = np.quantile(means, [0.025, 0.975])
    return float(low), float(high)


def safe_std(values: np.ndarray) -> float:
    if values.size <= 1:
        return 0.0
    return float(np.std(values, ddof=1))


def mean_sem(values: np.ndarray) -> tuple[float, float]:
    if values.size == 0:
        return (float("nan"), float("nan"))
    mean = float(np.mean(values))
    if values.size == 1:
        return (mean, 0.0)
    return (mean, float(stats.sem(values)))


def paired_tests(values: np.ndarray) -> dict[str, float | int | None]:
    values = np.asarray(values, dtype=np.float64)
    if values.size == 0:
        return {
            "n": 0,
            "mean_delta": None,
            "t_stat": None,
            "t_pvalue": None,
            "wilcoxon_stat": None,
            "wilcoxon_pvalue": None,
            "cohens_dz": None,
        }

    out: dict[str, float | int | None] = {
        "n": int(values.size),
        "mean_delta": float(np.mean(values)),
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
            w_res = stats.wilcoxon(values, zero_method="wilcox", alternative="two-sided")
            out["wilcoxon_stat"] = float(w_res.statistic)
            out["wilcoxon_pvalue"] = float(w_res.pvalue)
        except ValueError:
            out["wilcoxon_stat"] = None
            out["wilcoxon_pvalue"] = None

    return out


def aggregate_subject_rows(results: list[dict[str, Any]]) -> list[dict[str, Any]]:
    grouped: dict[tuple[str, str, int, int, int], list[dict[str, Any]]] = defaultdict(list)
    for row in results:
        grouped[
            (
                row["dataset"],
                row["algorithm"],
                row["target_fs"],
                row["window"],
                row["subject"],
            )
        ].append(row)

    subject_rows = []
    for (dataset, algorithm, target_fs, window, subject), rows in sorted(grouped.items()):
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
                "pyntbci_accuracy_mean": float(np.mean([row["pyntbci_accuracy"] for row in ordered])),
                "rust_exact_accuracy_mean": float(np.mean([row["rust_exact_accuracy"] for row in ordered])),
                "rust_exact_match_rate_mean": float(np.mean([row["rust_exact_match_rate"] for row in ordered])),
            }
        )
    return subject_rows


def optional_mean(values: list[float | None]) -> float | None:
    usable = [value for value in values if value is not None]
    if not usable:
        return None
    return float(np.mean(usable))


def grouped_statistics(
    subject_rows: list[dict[str, Any]],
    bootstrap_samples: int,
    seed: int,
) -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
    grouped: dict[tuple[str, str, int, int], list[dict[str, Any]]] = defaultdict(list)
    for row in subject_rows:
        grouped[(row["dataset"], row["algorithm"], row["target_fs"], row["window"])].append(row)

    summary_rows: list[dict[str, Any]] = []
    test_rows: list[dict[str, Any]] = []

    for (dataset, algorithm, target_fs, window), rows in sorted(grouped.items()):
        pynt = np.asarray([row["pyntbci_accuracy_mean"] for row in rows], dtype=np.float64)
        exact = np.asarray([row["rust_exact_accuracy_mean"] for row in rows], dtype=np.float64)
        match = np.asarray([row["rust_exact_match_rate_mean"] for row in rows], dtype=np.float64)
        exact_delta = exact - pynt

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
                **paired_tests(exact_delta),
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
                "comparison": "rust_exact_match_rate_minus_1",
                **paired_tests(match - 1.0),
            }
        )
    return summary_rows, test_rows


def chance_rate(classes: int) -> float:
    if classes <= 0:
        return float("nan")
    return 1.0 / float(classes)


def ordered_windows(rows: list[dict[str, Any]]) -> list[int]:
    return sorted({int(row["window"]) for row in rows})


def ordered_target_fs(rows: list[dict[str, Any]]) -> list[int]:
    return sorted({int(row["target_fs"]) for row in rows})


def grouped_summary_map(
    summary_rows: list[dict[str, Any]],
) -> dict[tuple[str, str], list[dict[str, Any]]]:
    grouped: dict[tuple[str, str], list[dict[str, Any]]] = defaultdict(list)
    for row in summary_rows:
        grouped[(row["dataset"], row["algorithm"])].append(row)
    for rows in grouped.values():
        rows.sort(key=lambda row: (row["target_fs"], row["window"]))
    return grouped


def save_figure_svg(fig: plt.Figure) -> str:
    buffer = io.StringIO()
    fig.savefig(buffer, format="svg", bbox_inches="tight")
    plt.close(fig)
    return buffer.getvalue()


def accuracy_line_svg(rows: list[dict[str, Any]]) -> str:
    dataset = rows[0]["dataset"]
    algorithm = rows[0]["algorithm"]
    classes = int(rows[0]["classes"])
    fig, ax = plt.subplots(figsize=(8.4, 4.6))
    for target_fs in ordered_target_fs(rows):
        fs_rows = [row for row in rows if int(row["target_fs"]) == target_fs]
        ax.plot(
            [row["window_seconds"] for row in fs_rows],
            [row["pyntbci_accuracy_mean"] for row in fs_rows],
            marker="o",
            linewidth=2.0,
            label=f"{target_fs} Hz",
        )
    ax.axhline(chance_rate(classes), color="#9a3412", linestyle="--", linewidth=1.5, label="Chance")
    ax.set_title(f"{dataset} {algorithm} accuracy vs window")
    ax.set_xlabel("Window (s)")
    ax.set_ylabel("Mean accuracy")
    ax.set_ylim(0.0, 1.02)
    ax.grid(True, alpha=0.25)
    ax.legend(frameon=False, ncols=3, loc="lower right")
    return save_figure_svg(fig)


def heatmap_svg(
    rows: list[dict[str, Any]],
    *,
    metric_key: str,
    title_suffix: str,
    vmin: float | None = 0.0,
    vmax: float | None = 1.0,
    cmap: str = "YlOrBr",
) -> str:
    windows = ordered_windows(rows)
    target_rates = ordered_target_fs(rows)
    row_map = {(int(row["target_fs"]), int(row["window"])): row for row in rows}
    window_seconds_map = {
        int(row["window"]): float(row["window_seconds"])
        for row in rows
    }
    grid = np.full((len(target_rates), len(windows)), np.nan, dtype=np.float64)
    for i, target_fs in enumerate(target_rates):
        for j, window in enumerate(windows):
            row = row_map.get((target_fs, window))
            if row is None:
                continue
            value = row.get(metric_key)
            if value is None:
                continue
            grid[i, j] = float(value)

    fig, ax = plt.subplots(figsize=(max(8.0, 0.48 * len(windows)), 2.0 + 0.7 * len(target_rates)))
    image = ax.imshow(grid, aspect="auto", cmap=cmap, vmin=vmin, vmax=vmax)
    ax.set_title(f"{rows[0]['dataset']} {rows[0]['algorithm']} {title_suffix}")
    ax.set_xlabel("Window (s)")
    ax.set_ylabel("Sample rate")
    ax.set_xticks(range(len(windows)))
    ax.set_xticklabels(
        [f"{window_seconds_map[window]:.2f}" for window in windows],
        rotation=45,
        ha="right",
    )
    ax.set_yticks(range(len(target_rates)))
    ax.set_yticklabels([f"{target_fs} Hz" for target_fs in target_rates])
    colorbar = fig.colorbar(image, ax=ax, shrink=0.9)
    colorbar.ax.set_ylabel(metric_key, rotation=270, labelpad=14)
    return save_figure_svg(fig)


def best_operating_points(summary_rows: list[dict[str, Any]], threshold: float = 0.95) -> list[dict[str, Any]]:
    grouped = grouped_summary_map(summary_rows)
    out: list[dict[str, Any]] = []
    for (dataset, algorithm), rows in sorted(grouped.items()):
        best_accuracy = max(float(row["pyntbci_accuracy_mean"]) for row in rows)
        target_accuracy = threshold * best_accuracy
        qualifying = [
            row for row in rows if float(row["pyntbci_accuracy_mean"]) >= target_accuracy
        ]
        qualifying.sort(
            key=lambda row: (
                float(row["window_seconds"]),
                int(row["target_fs"]),
            )
        )
        chosen = qualifying[0]
        out.append(
            {
                "dataset": dataset,
                "algorithm": algorithm,
                "target_fs": int(chosen["target_fs"]),
                "window_seconds": float(chosen["window_seconds"]),
                "requested_window_seconds": float(chosen["requested_window_seconds"]),
                "window": int(chosen["window"]),
                "accuracy_mean": float(chosen["pyntbci_accuracy_mean"]),
                "best_accuracy_mean": float(best_accuracy),
                "threshold_fraction": threshold,
                "threshold_accuracy": float(target_accuracy),
                "classes": int(chosen["classes"]),
            }
        )
    return out


def pareto_svg(points: list[dict[str, Any]]) -> str:
    fig, ax = plt.subplots(figsize=(8.4, 4.8))
    datasets = sorted({point["dataset"] for point in points})
    palette = plt.cm.tab10(np.linspace(0, 1, max(1, len(datasets))))
    dataset_colors = {dataset: palette[i] for i, dataset in enumerate(datasets)}

    for point in points:
        ax.scatter(
            point["window_seconds"],
            point["accuracy_mean"],
            s=95 if point["target_fs"] == 250 else 65,
            color=dataset_colors[point["dataset"]],
            marker="o" if point["target_fs"] == 250 else "s",
            alpha=0.9,
        )
        ax.text(
            point["window_seconds"] + 0.02,
            point["accuracy_mean"],
            f"{point['dataset']} {point['algorithm']} {point['target_fs']} Hz",
            fontsize=8,
            alpha=0.9,
        )

    ax.set_title("Shortest window reaching 95% of best accuracy")
    ax.set_xlabel("Window (s)")
    ax.set_ylabel("Mean accuracy")
    ax.set_ylim(0.0, 1.02)
    ax.grid(True, alpha=0.25)
    return save_figure_svg(fig)


def build_plot_sections(
    summary_rows: list[dict[str, Any]],
) -> tuple[list[dict[str, Any]], list[dict[str, Any]], list[dict[str, str]]]:
    grouped = grouped_summary_map(summary_rows)
    sections: list[dict[str, Any]] = []
    for (dataset, algorithm), rows in sorted(grouped.items()):
        plots = [
            {
                "title": "Accuracy vs window",
                "svg": accuracy_line_svg(rows),
            },
            {
                "title": "Accuracy heatmap",
                "svg": heatmap_svg(
                    rows,
                    metric_key="pyntbci_accuracy_mean",
                    title_suffix="accuracy heatmap",
                    vmin=0.0,
                    vmax=1.0,
                    cmap="YlOrBr",
                ),
            },
            {
                "title": "Rust exact match heatmap",
                "svg": heatmap_svg(
                    rows,
                    metric_key="rust_exact_match_rate_mean",
                    title_suffix="exact match heatmap",
                    vmin=0.0,
                    vmax=1.0,
                    cmap="Greens",
                ),
            },
        ]
        sections.append(
            {
                "dataset": dataset,
                "algorithm": algorithm,
                "plots": plots,
            }
        )

    operating_points = best_operating_points(summary_rows)
    overview_sections = [
        {
            "title": "95% operating points",
            "svg": pareto_svg(operating_points),
        }
    ]
    return sections, operating_points, overview_sections


def render_console(summary_rows: list[dict[str, Any]], test_rows: list[dict[str, Any]]) -> None:
    console = Console()
    summary = Table(title="CVEP benchmark statistics")
    summary.add_column("Dataset")
    summary.add_column("Algorithm")
    summary.add_column("fs")
    summary.add_column("Req s")
    summary.add_column("Actual s")
    summary.add_column("Subjects")
    summary.add_column("PyntBCI mean")
    summary.add_column("Rust exact mean")
    summary.add_column("Exact match mean")
    for row in summary_rows:
        summary.add_row(
            row["dataset"],
            row["algorithm"],
            str(row["target_fs"]),
            f"{row['requested_window_seconds']:.3f}",
            f"{row['window_seconds']:.3f}",
            str(row["subjects"]),
            f"{row['pyntbci_accuracy_mean']:.4f}",
            f"{row['rust_exact_accuracy_mean']:.4f}",
            f"{row['rust_exact_match_rate_mean']:.4f}",
        )
    console.print(summary)

    tests = Table(title="Paired tests on subject-level deltas")
    tests.add_column("Dataset")
    tests.add_column("Algorithm")
    tests.add_column("fs")
    tests.add_column("Actual s")
    tests.add_column("Comparison")
    tests.add_column("n")
    tests.add_column("Mean delta")
    tests.add_column("t p-value")
    tests.add_column("Wilcoxon p-value")
    for row in test_rows:
        tests.add_row(
            row["dataset"],
            row["algorithm"],
            str(row["target_fs"]),
            f"{row['window_seconds']:.3f}",
            row["comparison"],
            str(row["n"]),
            fmt_optional(row["mean_delta"]),
            fmt_optional(row["t_pvalue"]),
            fmt_optional(row["wilcoxon_pvalue"]),
        )
    console.print(tests)


def fmt_optional(value: float | None) -> str:
    if value is None or (isinstance(value, float) and not math.isfinite(value)):
        return "-"
    return f"{value:.4f}"


def csv_from_rows(rows: list[dict[str, Any]]) -> str:
    if not rows:
        return ""
    keys = list(rows[0].keys())
    lines = [",".join(keys)]
    for row in rows:
        values = []
        for key in keys:
            value = row[key]
            values.append("" if value is None else str(value))
        lines.append(",".join(values))
    return "\n".join(lines) + "\n"


def render_html(
    output: Path,
    input_config: dict[str, Any],
    summary_rows: list[dict[str, Any]],
    test_rows: list[dict[str, Any]],
    subject_rows: list[dict[str, Any]],
    plot_sections: list[dict[str, Any]],
    operating_points: list[dict[str, Any]],
    overview_plots: list[dict[str, str]],
) -> None:
    summary_html = "\n".join(
        (
            "<tr>"
            f"<td>{html.escape(row['dataset'])}</td>"
            f"<td>{html.escape(row['algorithm'])}</td>"
            f"<td>{row['target_fs']}</td>"
            f"<td>{row['requested_window_seconds']:.3f}</td>"
            f"<td>{row['window_seconds']:.3f}</td>"
            f"<td>{row['subjects']}</td>"
            f"<td>{row['pyntbci_accuracy_mean']:.4f}</td>"
            f"<td>{row['pyntbci_accuracy_ci_low']:.4f} to {row['pyntbci_accuracy_ci_high']:.4f}</td>"
            f"<td>{row['rust_exact_accuracy_mean']:.4f}</td>"
            f"<td>{row['rust_exact_accuracy_ci_low']:.4f} to {row['rust_exact_accuracy_ci_high']:.4f}</td>"
            f"<td>{row['rust_exact_match_rate_mean']:.4f}</td>"
            "</tr>"
        )
        for row in summary_rows
    )
    tests_html = "\n".join(
        (
            "<tr>"
            f"<td>{html.escape(row['dataset'])}</td>"
            f"<td>{html.escape(row['algorithm'])}</td>"
            f"<td>{row['target_fs']}</td>"
            f"<td>{row['window_seconds']:.3f}</td>"
            f"<td>{html.escape(row['comparison'])}</td>"
            f"<td>{row['n']}</td>"
            f"<td>{fmt_optional(row['mean_delta'])}</td>"
            f"<td>{fmt_optional(row['t_pvalue'])}</td>"
            f"<td>{fmt_optional(row['wilcoxon_pvalue'])}</td>"
            f"<td>{fmt_optional(row['cohens_dz'])}</td>"
            "</tr>"
        )
        for row in test_rows
    )
    subjects_html = "\n".join(
        (
            "<tr>"
            f"<td>{html.escape(row['dataset'])}</td>"
            f"<td>{html.escape(row['algorithm'])}</td>"
            f"<td>{row['target_fs']}</td>"
            f"<td>{row['subject']}</td>"
            f"<td>{row['n_folds']}</td>"
            f"<td>{row['requested_window_seconds']:.3f}</td>"
            f"<td>{row['window_seconds']:.3f}</td>"
            f"<td>{row['pyntbci_accuracy_mean']:.4f}</td>"
            f"<td>{row['rust_exact_accuracy_mean']:.4f}</td>"
            f"<td>{row['rust_exact_match_rate_mean']:.4f}</td>"
            "</tr>"
        )
        for row in subject_rows
    )
    operating_points_html = "\n".join(
        (
            "<tr>"
            f"<td>{html.escape(row['dataset'])}</td>"
            f"<td>{html.escape(row['algorithm'])}</td>"
            f"<td>{row['target_fs']}</td>"
            f"<td>{row['window_seconds']:.3f}</td>"
            f"<td>{row['accuracy_mean']:.4f}</td>"
            f"<td>{row['best_accuracy_mean']:.4f}</td>"
            f"<td>{row['threshold_accuracy']:.4f}</td>"
            "</tr>"
        )
        for row in operating_points
    )
    overview_html = "\n".join(
        (
            f"<section class=\"plot-card\"><h3>{html.escape(plot['title'])}</h3>"
            f"<div class=\"plot-frame\">{plot['svg']}</div></section>"
        )
        for plot in overview_plots
    )
    plot_sections_html = "\n".join(
        (
            "<section class=\"card\">"
            f"<h2>{html.escape(section['dataset'])} {html.escape(section['algorithm'])}</h2>"
            "<div class=\"plot-grid\">"
            + "\n".join(
                f"<section class=\"plot-card\"><h3>{html.escape(plot['title'])}</h3>"
                f"<div class=\"plot-frame\">{plot['svg']}</div></section>"
                for plot in section["plots"]
            )
            + "</div></section>"
        )
        for section in plot_sections
    )
    config_html = html.escape(json.dumps(input_config, indent=2))
    document = f"""<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>CVEP Benchmark Analysis</title>
  <style>
    :root {{
      color-scheme: light;
      --bg: #f2eee6;
      --panel: #fffdf9;
      --ink: #1e2933;
      --muted: #5d6a75;
      --line: #d8cfbf;
      --accent: #8a4f1d;
    }}
    body {{
      margin: 0;
      background: radial-gradient(circle at top, #efe0c2 0%, var(--bg) 42%, #f9f7f2 100%);
      color: var(--ink);
      font-family: Cambria, Georgia, serif;
    }}
    main {{
      max-width: 1240px;
      margin: 0 auto;
      padding: 28px 18px 56px;
    }}
    .card {{
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 18px;
      padding: 20px;
      box-shadow: 0 12px 36px rgba(52, 39, 18, 0.08);
      margin-bottom: 18px;
    }}
    h1, h2 {{
      margin: 0 0 12px;
    }}
    p {{
      margin: 0 0 14px;
      color: var(--muted);
    }}
    .table-wrap {{
      overflow: auto;
      border: 1px solid var(--line);
      border-radius: 14px;
    }}
    table {{
      width: 100%;
      border-collapse: collapse;
      font-family: "SFMono-Regular", Menlo, monospace;
      font-size: 0.88rem;
    }}
    th, td {{
      padding: 10px 12px;
      border-bottom: 1px solid var(--line);
      text-align: left;
    }}
    th {{
      background: #faf5ec;
      color: var(--accent);
      position: sticky;
      top: 0;
    }}
    pre {{
      margin: 0;
      overflow: auto;
      background: #fbf8f1;
      border: 1px solid var(--line);
      border-radius: 12px;
      padding: 14px;
    }}
    .plot-grid {{
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(360px, 1fr));
      gap: 16px;
    }}
    .plot-card {{
      border: 1px solid var(--line);
      border-radius: 14px;
      padding: 14px;
      background: #fffaf3;
    }}
    .plot-card h3 {{
      margin: 0 0 10px;
    }}
    .plot-frame {{
      overflow: auto;
      border-radius: 10px;
      background: #fff;
      border: 1px solid var(--line);
      padding: 8px;
    }}
    .plot-frame svg {{
      width: 100%;
      height: auto;
    }}
  </style>
</head>
<body>
  <main>
    <section class="card">
      <h1>CVEP Benchmark Analysis</h1>
      <p>Subject-level accuracy statistics and paired tests derived from benchmark_pyntbci_vs_rust.py output.</p>
    </section>
    <section class="card">
      <h2>Interpretation Plots</h2>
      <p>These plots emphasize the embedded tradeoff surface: how accuracy moves with decoding window and sample rate, and whether Rust parity drifts across operating points.</p>
      <div class="plot-grid">
        {overview_html}
      </div>
    </section>
    <section class="card">
      <h2>Operating Points</h2>
      <p>Shortest window in each dataset and algorithm family that still reaches at least 95% of that group's best measured accuracy.</p>
      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Dataset</th>
              <th>Algorithm</th>
              <th>fs</th>
              <th>Actual s</th>
              <th>Accuracy</th>
              <th>Best accuracy</th>
              <th>95% threshold</th>
            </tr>
          </thead>
          <tbody>{operating_points_html}</tbody>
        </table>
      </div>
    </section>
    {plot_sections_html}
    <section class="card">
      <h2>Grouped Summary</h2>
      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Dataset</th>
              <th>Algorithm</th>
              <th>fs</th>
              <th>Requested s</th>
              <th>Actual s</th>
              <th>Subjects</th>
              <th>PyntBCI mean</th>
              <th>PyntBCI 95% CI</th>
              <th>Rust exact mean</th>
              <th>Rust exact 95% CI</th>
              <th>Exact match mean</th>
            </tr>
          </thead>
          <tbody>{summary_html}</tbody>
        </table>
      </div>
    </section>
    <section class="card">
      <h2>Paired Tests</h2>
      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Dataset</th>
              <th>Algorithm</th>
              <th>fs</th>
              <th>Actual s</th>
              <th>Comparison</th>
              <th>n</th>
              <th>Mean delta</th>
              <th>t-test p</th>
              <th>Wilcoxon p</th>
              <th>Cohen's dz</th>
            </tr>
          </thead>
          <tbody>{tests_html}</tbody>
        </table>
      </div>
    </section>
    <section class="card">
      <h2>Subject Means</h2>
      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Dataset</th>
              <th>Algorithm</th>
              <th>fs</th>
              <th>Subject</th>
              <th>Folds</th>
              <th>Requested s</th>
              <th>Actual s</th>
              <th>PyntBCI mean</th>
              <th>Rust exact mean</th>
              <th>Exact match mean</th>
            </tr>
          </thead>
          <tbody>{subjects_html}</tbody>
        </table>
      </div>
    </section>
    <section class="card">
      <h2>Config</h2>
      <pre>{config_html}</pre>
    </section>
  </main>
</body>
</html>
"""
    output.write_text(document, encoding="utf-8")


def main() -> None:
    args = parse_args()
    payload = json.loads(args.input_json.read_text(encoding="utf-8"))
    results = payload["results"]

    subject_rows = aggregate_subject_rows(results)
    summary_rows, test_rows = grouped_statistics(
        subject_rows,
        bootstrap_samples=args.bootstrap_samples,
        seed=args.seed,
    )
    plot_sections, operating_points, overview_plots = build_plot_sections(summary_rows)

    analysis = {
        "benchmark_config": payload["config"],
        "analysis_config": {
            "bootstrap_samples": args.bootstrap_samples,
            "seed": args.seed,
        },
        "summary_rows": summary_rows,
        "test_rows": test_rows,
        "subject_rows": subject_rows,
        "operating_points": operating_points,
    }

    args.output_json.parent.mkdir(parents=True, exist_ok=True)
    args.output_csv.parent.mkdir(parents=True, exist_ok=True)
    args.output_html.parent.mkdir(parents=True, exist_ok=True)

    args.output_json.write_text(json.dumps(analysis, indent=2) + "\n", encoding="utf-8")
    args.output_csv.write_text(csv_from_rows(summary_rows), encoding="utf-8")
    render_html(
        args.output_html,
        payload["config"],
        summary_rows,
        test_rows,
        subject_rows,
        plot_sections,
        operating_points,
        overview_plots,
    )
    render_console(summary_rows, test_rows)

    console = Console()
    console.print(f"[green]wrote[/green] {args.output_json}")
    console.print(f"[green]wrote[/green] {args.output_csv}")
    console.print(f"[green]wrote[/green] {args.output_html}")


if __name__ == "__main__":
    main()
