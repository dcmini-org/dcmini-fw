from __future__ import annotations

import json
import subprocess
from pathlib import Path


def run_rust_fixture(fixture_path: Path, rust_binary: Path) -> dict:
    result = subprocess.run(
        [str(rust_binary), str(fixture_path)],
        check=True,
        capture_output=True,
        text=True,
    )
    return json.loads(result.stdout)
