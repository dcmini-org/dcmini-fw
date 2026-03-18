from __future__ import annotations

import html
import json
from pathlib import Path
from typing import Any

import numpy as np


def sliding_window_starts(
    trial_samples: int,
    window_samples: int,
    step_samples: int,
) -> np.ndarray:
    if window_samples > trial_samples:
        return np.asarray([], dtype=np.int64)
    last_start = trial_samples - window_samples
    starts = np.arange(0, last_start + 1, step_samples, dtype=np.int64)
    if starts.size == 0 or starts[-1] != last_start:
        starts = np.concatenate((starts, np.asarray([last_start], dtype=np.int64)))
    return starts


def rows_to_csv(rows: list[dict[str, Any]], keys: list[str]) -> str:
    lines = [",".join(keys)]
    for row in rows:
        lines.append(
            ",".join("" if row.get(key) is None else str(row[key]) for key in keys)
        )
    return "\n".join(lines) + "\n"


def grouped_summary(
    rows: list[dict[str, Any]],
    *,
    extra_key_names: list[str] | None = None,
) -> list[dict[str, Any]]:
    extra_key_names = extra_key_names or []
    grouped: dict[tuple[Any, ...], list[dict[str, Any]]] = {}
    for row in rows:
        key = (
            row["variant"],
            row["dataset"],
            row["window_seconds"],
            row["window_end_seconds"],
            *[row[name] for name in extra_key_names],
        )
        grouped.setdefault(key, []).append(row)
    out = []
    for key, members in sorted(grouped.items()):
        base = {
            "variant": key[0],
            "dataset": key[1],
            "window_seconds": key[2],
            "window_end_seconds": key[3],
            "subjects": len({row["subject"] for row in members}),
            "mean_accuracy": float(np.mean([row["accuracy"] for row in members])),
        }
        for idx, name in enumerate(extra_key_names, start=4):
            base[name] = key[idx]
        out.append(base)
    return out


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
    def _fmt(value: Any) -> str:
        if isinstance(value, float):
            return f"{value:.4f}" if abs(value) < 1000 else f"{value}"
        return html.escape(str(value))

    summary_rows = "\n".join(
        "<tr>"
        + "".join(f"<td>{_fmt(row[key])}</td>" for _label, key in summary_columns)
        + "</tr>"
        for row in summary
    )
    detail_rows = "\n".join(
        "<tr>"
        + "".join(f"<td>{_fmt(row[key])}</td>" for _label, key in detail_columns)
        + "</tr>"
        for row in rows
    )
    summary_header = "".join(
        f"<th>{html.escape(label)}</th>" for label, _key in summary_columns
    )
    detail_header = "".join(
        f"<th>{html.escape(label)}</th>" for label, _key in detail_columns
    )
    document = f"""<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{html.escape(title)}</title>
  <style>
    :root {{ --bg: #f6f2ea; --panel: #fffdf8; --ink: #1d2935; --muted: #5b6875; --line: #d8cfbf; }}
    body {{ margin: 0; background: var(--bg); color: var(--ink); font-family: Georgia, serif; }}
    main {{ max-width: 1200px; margin: 0 auto; padding: 28px 18px 40px; }}
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
      <table><thead><tr>{summary_header}</tr></thead><tbody>{summary_rows}</tbody></table>
    </div>
    <div class="card">
      <h2>Details</h2>
      <table><thead><tr>{detail_header}</tr></thead><tbody>{detail_rows}</tbody></table>
    </div>
  </main>
</body>
</html>
"""
    output.write_text(document, encoding="utf-8")
