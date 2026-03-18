from __future__ import annotations

import json
import subprocess
from pathlib import Path

from cvep_bench.runtime.json_fixtures import write_fixture_json


def run_fixture_file(binary: Path, fixture_path: Path) -> dict:
    result = subprocess.run(
        [str(binary), str(fixture_path)], check=True, capture_output=True, text=True
    )
    return json.loads(result.stdout)


def run_fixture_payload(binary: Path, payload: dict, *, fixture_path: Path) -> dict:
    write_fixture_json(fixture_path, payload)
    return run_fixture_file(binary, fixture_path)


def maybe_run_fixture_payload(
    binary: Path | None, payload: dict, *, fixture_path: Path
) -> dict | None:
    if binary is None:
        return None
    return run_fixture_payload(binary, payload, fixture_path=fixture_path)
