from __future__ import annotations

from contextlib import contextmanager
import json
import tempfile
from pathlib import Path
from typing import Iterator


def write_fixture_json(path: Path, payload: dict) -> None:
    path.write_text(json.dumps(payload), encoding="utf-8")


@contextmanager
def temporary_fixture_path(*, prefix: str) -> Iterator[Path]:
    with tempfile.TemporaryDirectory(prefix=prefix) as tmp_dir:
        yield Path(tmp_dir) / "fixture.json"
