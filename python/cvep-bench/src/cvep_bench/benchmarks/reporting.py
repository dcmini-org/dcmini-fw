from __future__ import annotations

import csv
import html
import json
from pathlib import Path
from typing import Any, Callable, Iterable

import numpy as np
from rich.console import Console
from rich.table import Table


def write_json_payload(path: Path, payload: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")


def rows_to_csv(rows: list[dict[str, Any]], columns: list[str]) -> str:
    lines = [",".join(columns)]
    for row in rows:
        lines.append(
            ",".join(
                "" if row.get(column) is None else str(row[column])
                for column in columns
            )
        )
    return "\n".join(lines) + "\n"


def write_csv_rows(path: Path, rows: list[dict[str, Any]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if not rows:
        path.write_text("", encoding="utf-8")
        return
    with path.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=list(rows[0].keys()))
        writer.writeheader()
        writer.writerows(rows)


def mean_or_none(values: Iterable[float | None]) -> float | None:
    filtered = [value for value in values if value is not None]
    return None if not filtered else float(np.mean(filtered))


def group_rows(
    rows: list[dict[str, Any]], key_fields: list[str]
) -> dict[tuple[Any, ...], list[dict[str, Any]]]:
    grouped: dict[tuple[Any, ...], list[dict[str, Any]]] = {}
    for row in rows:
        key = tuple(row[field] for field in key_fields)
        grouped.setdefault(key, []).append(row)
    return grouped


def build_group_summary(
    rows: list[dict[str, Any]],
    *,
    key_fields: list[str],
    metric_fields: list[str],
    optional_metric_fields: tuple[str, ...] = (),
    subject_field: str = "subject",
) -> list[dict[str, Any]]:
    grouped = group_rows(rows, key_fields)
    summary_rows = []
    for key, members in sorted(grouped.items()):
        base = {field: key[idx] for idx, field in enumerate(key_fields)}
        base["subjects"] = len(
            {row[subject_field] for row in members if subject_field in row}
        )
        for field in metric_fields:
            base[f"mean_{field}"] = float(np.mean([row[field] for row in members]))
        for field in optional_metric_fields:
            base[f"mean_{field}"] = mean_or_none(row.get(field) for row in members)
        summary_rows.append(base)
    return summary_rows


def render_tabular_html(
    output: Path,
    *,
    title: str,
    subtitle: str,
    config: dict[str, Any],
    summary_columns: list[tuple[str, str]],
    summary_rows: list[dict[str, Any]],
    detail_columns: list[tuple[str, str]] | None = None,
    detail_rows: list[dict[str, Any]] | None = None,
) -> None:
    def _fmt(value: Any) -> str:
        if isinstance(value, float):
            return f"{value:.4f}"
        return html.escape(str(value))

    summary_header = "".join(
        f"<th>{html.escape(label)}</th>" for label, _field in summary_columns
    )
    summary_body = "\n".join(
        "<tr>"
        + "".join(f"<td>{_fmt(row[field])}</td>" for _label, field in summary_columns)
        + "</tr>"
        for row in summary_rows
    )

    detail_section = ""
    if detail_columns is not None and detail_rows is not None:
        detail_header = "".join(
            f"<th>{html.escape(label)}</th>" for label, _field in detail_columns
        )
        detail_body = "\n".join(
            "<tr>"
            + "".join(
                f"<td>{_fmt(row[field])}</td>" for _label, field in detail_columns
            )
            + "</tr>"
            for row in detail_rows
        )
        detail_section = f'<div class="card"><h2>Details</h2><table><thead><tr>{detail_header}</tr></thead><tbody>{detail_body}</tbody></table></div>'

    document = f"""<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{html.escape(title)}</title>
  <style>
    :root {{ --bg: #f6f2ea; --panel: #fffdf8; --ink: #1d2935; --muted: #5b6875; --line: #d8cfbf; }}
    body {{ margin: 0; background: var(--bg); color: var(--ink); font-family: Georgia, serif; }}
    main {{ max-width: 1280px; margin: 0 auto; padding: 28px 18px 40px; }}
    .card {{ background: var(--panel); border: 1px solid var(--line); border-radius: 16px; padding: 18px; margin-bottom: 18px; }}
    table {{ width: 100%; border-collapse: collapse; font-size: 0.95rem; }}
    th, td {{ padding: 10px 8px; border-bottom: 1px solid var(--line); text-align: left; }}
    th {{ color: var(--muted); text-transform: uppercase; letter-spacing: 0.06em; font-size: 0.75rem; }}
    pre {{ overflow-x: auto; background: #f6f2ea; padding: 12px; border-radius: 12px; }}
  </style>
</head>
<body>
  <main>
    <div class="card">
      <h1>{html.escape(title)}</h1>
      <p>{html.escape(subtitle)}</p>
      <pre>{html.escape(json.dumps(config, indent=2))}</pre>
    </div>
    <div class="card">
      <h2>Summary</h2>
      <table><thead><tr>{summary_header}</tr></thead><tbody>{summary_body}</tbody></table>
    </div>
    {detail_section}
  </main>
</body>
</html>
"""
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(document, encoding="utf-8")


def render_rich_table(
    console: Console,
    *,
    title: str,
    columns: list[tuple[str, str]],
    rows: list[dict[str, Any]],
    formatters: dict[str, Callable[[Any], str]] | None = None,
) -> None:
    formatters = formatters or {}
    table = Table(title=title)
    for label, _field in columns:
        table.add_column(label)
    for row in rows:
        rendered = []
        for _label, field in columns:
            value = row[field]
            rendered.append(
                formatters[field](value) if field in formatters else str(value)
            )
        table.add_row(*rendered)
    console.print(table)
