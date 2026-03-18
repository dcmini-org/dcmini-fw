from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from cvep_bench.datasets.loaders import load_subject, subject_list_for_dataset
from cvep_bench.datasets.profiles import BenchmarkProfile, resolve_window_grid
from cvep_bench.datasets.windows import decode_window_requests


def resolve_subjects(
    dataset: str,
    requested_subjects: list[int] | None,
    max_subjects: int | None,
) -> list[int]:
    subjects = requested_subjects or subject_list_for_dataset(dataset)
    return subjects if max_subjects is None else subjects[:max_subjects]


def ensure_output_dirs(paths: list[Path]) -> None:
    for path in paths:
        path.parent.mkdir(parents=True, exist_ok=True)


def resolve_window_requests_for_dataset(
    dataset: str,
    profile: BenchmarkProfile,
    explicit_grid: list[float] | None,
    step_seconds: float | None,
    full_trial_seconds: float,
) -> tuple[list[float], list[float] | None]:
    resolved_window_grid = resolve_window_grid(
        profile, dataset, explicit_grid, step_seconds
    )
    return decode_window_requests(
        full_trial_seconds, explicit=resolved_window_grid, step_seconds=step_seconds
    ), resolved_window_grid


@dataclass
class BenchmarkDataCache:
    data_dir: Path
    _cache: dict[tuple[str, int, int, float | None, str, str], Any] = field(
        default_factory=dict
    )

    def get(
        self,
        dataset: str,
        subject: int,
        target_fs: int,
        *,
        trial_seconds: float | None,
        preprocessing: Any,
        thielen2021_source: str = "raw",
    ) -> Any:
        cache_key = (
            dataset,
            subject,
            target_fs,
            trial_seconds,
            repr(preprocessing),
            thielen2021_source,
        )
        if cache_key not in self._cache:
            self._cache[cache_key] = load_subject(
                dataset,
                subject,
                self.data_dir,
                target_fs,
                trial_seconds=trial_seconds,
                preprocessing=preprocessing,
                thielen2021_source=thielen2021_source,
            )
        return self._cache[cache_key]
