from __future__ import annotations

import argparse
from pathlib import Path


def add_data_dir_arg(parser: argparse.ArgumentParser, default: Path) -> None:
    parser.add_argument("--data-dir", type=Path, default=default)


def add_output_args(
    parser: argparse.ArgumentParser,
    *,
    output_dir: Path,
    stem: str,
    include_csv: bool = True,
    include_html: bool = True,
) -> None:
    parser.add_argument("--output-json", type=Path, default=output_dir / f"{stem}.json")
    if include_csv:
        parser.add_argument(
            "--output-csv", type=Path, default=output_dir / f"{stem}.csv"
        )
    if include_html:
        parser.add_argument(
            "--output-html", type=Path, default=output_dir / f"{stem}.html"
        )


def add_dataset_args(
    parser: argparse.ArgumentParser,
    *,
    default_datasets: list[str],
    allow_subjects: bool = True,
    default_max_subjects: int | None = None,
) -> None:
    parser.add_argument("--datasets", nargs="+", default=default_datasets)
    if allow_subjects:
        parser.add_argument("--subjects", type=int, nargs="+", default=None)
    parser.add_argument("--max-subjects", type=int, default=default_max_subjects)


def add_fold_args(
    parser: argparse.ArgumentParser,
    *,
    default_folds: int = 5,
    multi_index: bool = True,
) -> None:
    parser.add_argument("--folds", type=int, default=default_folds)
    if multi_index:
        parser.add_argument("--fold-index", type=int, nargs="+", default=None)
    else:
        parser.add_argument("--fold-index", type=int, default=0)


def add_target_fs_args(
    parser: argparse.ArgumentParser,
    *,
    default: int | None = None,
    include_grid: bool = False,
) -> None:
    parser.add_argument("--target-fs", type=int, default=default)
    if include_grid:
        parser.add_argument("--target-fs-grid", type=int, nargs="+", default=None)


def add_window_args(
    parser: argparse.ArgumentParser,
    *,
    default_grid: list[float] | None = None,
    include_step: bool = True,
) -> None:
    if include_step:
        parser.add_argument("--window-step-seconds", type=float, default=None)
    parser.add_argument(
        "--window-seconds-grid", type=float, nargs="+", default=default_grid
    )


def add_profile_arg(
    parser: argparse.ArgumentParser, *, choices: list[str], default: str = "legacy"
) -> None:
    parser.add_argument("--profile", choices=choices, default=default)


def add_rust_args(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--skip-rust", action="store_true")


def add_adc_args(
    parser: argparse.ArgumentParser,
    *,
    bits: int = 24,
    headroom: float = 0.95,
) -> None:
    parser.add_argument("--adc-bits", type=int, default=bits)
    parser.add_argument("--adc-headroom", type=float, default=headroom)


def add_preprocessing_override_args(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--band-low", type=float, default=None)
    parser.add_argument("--band-high", type=float, default=None)
    parser.add_argument("--notch-hz", type=float, default=None)
    parser.add_argument("--drop-first-seconds", type=float, default=None)


def parse_bool_choice_grid(values: list[str]) -> list[bool]:
    return [value == "true" for value in values]


def resolve_fold_indices(folds: int, fold_index: list[int] | None) -> list[int]:
    return list(range(folds)) if fold_index is None else fold_index
