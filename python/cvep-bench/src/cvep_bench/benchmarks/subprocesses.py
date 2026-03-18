from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path
from typing import Any


def run_module_with_json_output(module: str, argv: list[str]) -> dict[str, Any]:
    result = subprocess.run(
        [sys.executable, "-m", module, *argv], capture_output=True, text=True
    )
    if result.returncode != 0:
        raise RuntimeError(
            f"Command failed with code {result.returncode}\nstdout:\n{result.stdout}\n\nstderr:\n{result.stderr}"
        )
    output_json = Path(argv[argv.index("--output-json") + 1])
    payload = json.loads(output_json.read_text(encoding="utf-8"))
    payload["stdout"] = result.stdout
    payload["stderr"] = result.stderr
    return payload


def build_common_benchmark_argv(
    args: Any,
    *,
    output_prefix: Path,
    include_profile: bool = False,
    include_target_fs: bool = True,
    profile_value: str | None = None,
) -> list[str]:
    command = [
        "--data-dir",
        str(args.data_dir),
        "--output-json",
        str(output_prefix.with_suffix(".json")),
        "--output-csv",
        str(output_prefix.with_suffix(".csv")),
        "--output-html",
        str(output_prefix.with_suffix(".html")),
        "--datasets",
        *args.datasets,
        "--folds",
        str(args.folds),
        "--window-seconds-grid",
        *[str(value) for value in args.window_seconds_grid],
    ]
    if include_profile:
        resolved_profile = profile_value if profile_value is not None else args.profile
        command = ["--profile", resolved_profile, *command]
    if args.subjects is not None:
        command.extend(["--subjects", *[str(value) for value in args.subjects]])
    if args.max_subjects is not None:
        command.extend(["--max-subjects", str(args.max_subjects)])
    if args.fold_index is not None:
        command.extend(["--fold-index", *[str(value) for value in args.fold_index]])
    if include_target_fs and getattr(args, "target_fs", None) is not None:
        command.extend(["--target-fs", str(args.target_fs)])
    if getattr(args, "skip_rust", False):
        command.append("--skip-rust")
    return command
