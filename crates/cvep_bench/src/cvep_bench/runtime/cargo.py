from __future__ import annotations

import subprocess
from pathlib import Path


WORKSPACE_ROOT = Path(__file__).resolve().parents[5]


def build_rust_binary(binary_name: str) -> Path:
    subprocess.run(
        [
            "cargo",
            "build",
            "--quiet",
            "-p",
            "cvep-decoder",
            "--features",
            "host-tools",
            "--bin",
            binary_name,
        ],
        check=True,
        cwd=WORKSPACE_ROOT,
    )
    return WORKSPACE_ROOT / "target" / "debug" / binary_name
