from __future__ import annotations

from pathlib import Path
from typing import Any

from cvep_bench.benchmarks.reporting import (
    build_group_summary,
    render_tabular_html,
    rows_to_csv,
)
from cvep_bench.datasets.windowing import sliding_window_starts


def grouped_summary(
    rows: list[dict[str, Any]],
    *,
    extra_key_names: list[str] | None = None,
) -> list[dict[str, Any]]:
    extra_key_names = extra_key_names or []
    return build_group_summary(
        rows,
        key_fields=[
            "variant",
            "dataset",
            "window_seconds",
            "window_end_seconds",
            *extra_key_names,
        ],
        metric_fields=["accuracy"],
    )


def render_sliding_html_report(
    output: Path,
    *,
    title: str,
    subtitle: str,
    config: dict[str, Any],
    rows: list[dict[str, Any]],
    summary: list[dict[str, Any]],
    summary_columns: list[tuple[str, str]],
    detail_columns: list[tuple[str, str]],
) -> None:
    render_tabular_html(
        output,
        title=title,
        subtitle=subtitle,
        config=config,
        summary_columns=summary_columns,
        summary_rows=summary,
        detail_columns=detail_columns,
        detail_rows=rows,
    )


__all__ = [
    "grouped_summary",
    "render_sliding_html_report",
    "rows_to_csv",
    "sliding_window_starts",
]
