from __future__ import annotations

from pathlib import Path


WORKSPACE_ROOT = Path(__file__).resolve().parents[3]
DATA_ROOT = WORKSPACE_ROOT / "crates" / "cvep-decoder" / "data"
