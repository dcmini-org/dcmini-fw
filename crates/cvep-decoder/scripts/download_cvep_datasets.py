#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10,<3.12"
# dependencies = [
#   "filelock>=3.25.2",
#   "moabb>=1.4.3,<1.6",
#   "rich>=14.3.3",
# ]
# ///
"""Download the open c-VEP datasets used for decoder benchmarking.

This script uses MOABB's dataset wrappers to fetch the currently supported
c-VEP datasets into a local cache rooted at `crates/cvep-decoder/data/`.

Example:

    uv run --script crates/cvep-decoder/scripts/download_cvep_datasets.py
"""

from __future__ import annotations

import argparse
import concurrent.futures
import contextlib
import json
import logging
import os
import threading
import warnings
from collections import deque
from dataclasses import dataclass
from inspect import Parameter, signature
from pathlib import Path
from urllib.parse import urlparse
from typing import Any

from rich.console import Console, Group
from rich.live import Live
from rich.progress import (
    BarColumn,
    Progress,
    SpinnerColumn,
    TaskID,
    TaskProgressColumn,
    TextColumn,
    TimeElapsedColumn,
    TimeRemainingColumn,
)


@dataclass(frozen=True)
class DatasetSpec:
    name: str
    constructor_kwargs: dict[str, Any]


@dataclass
class DatasetMonitor:
    progress: Progress
    task_id: TaskID


class TailCapture:
    def __init__(self, max_chunks: int = 64) -> None:
        self._chunks: deque[str] = deque(maxlen=max_chunks)

    def write(self, text: str) -> int:
        if text:
            self._chunks.append(text)
        return len(text)

    def flush(self) -> None:
        return None

    def tail(self) -> str:
        return "".join(self._chunks).strip()


class RichDownloadProxy:
    def __init__(self, monitor: DatasetMonitor) -> None:
        self._monitor = monitor
        self._total: int | None = None
        self._completed = 0

    @property
    def total(self) -> int | None:
        return self._total

    @total.setter
    def total(self, value: int | None) -> None:
        self._total = int(value) if value else None
        self._completed = 0
        self._monitor.progress.update(
            self._monitor.task_id,
            total=self._total,
            completed=0,
            detail=format_bytes_progress(0, self._total),
        )

    def update(self, amount: int) -> None:
        self._completed += int(amount)
        self._monitor.progress.update(
            self._monitor.task_id,
            completed=self._completed,
            detail=format_bytes_progress(self._completed, self._total),
        )

    def reset(self) -> None:
        self._completed = 0
        self._monitor.progress.update(
            self._monitor.task_id,
            completed=0,
            detail=format_bytes_progress(0, self._total),
        )

    def close(self) -> None:
        return None


DEFAULT_DATASETS = (
    DatasetSpec("Thielen2015", {}),
    DatasetSpec("Thielen2021", {}),
    DatasetSpec("CastillosBurstVEP40", {}),
    DatasetSpec("CastillosBurstVEP100", {}),
    DatasetSpec("CastillosCVEP40", {}),
    DatasetSpec("CastillosCVEP100", {}),
    DatasetSpec("MartinezCagigal2023Checker", {}),
    DatasetSpec("MartinezCagigal2023Pary", {}),
)

_THREAD_MONITOR = threading.local()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--data-dir",
        type=Path,
        default=Path(__file__).resolve().parents[1] / "data",
        help="Directory where MOABB should cache the datasets.",
    )
    parser.add_argument(
        "--dataset",
        action="append",
        default=[],
        help=(
            "Optional dataset name to download. Repeat to limit the download "
            "set. Defaults to all supported c-VEP datasets."
        ),
    )
    parser.add_argument(
        "--force-update",
        action="store_true",
        help="Redownload datasets even if a local copy already exists.",
    )
    parser.add_argument(
        "--manifest-json",
        type=Path,
        default=None,
        help="Optional path for a JSON manifest describing the downloaded datasets.",
    )
    parser.add_argument(
        "--jobs",
        type=int,
        default=1,
        help="Number of datasets to download in parallel.",
    )
    return parser.parse_args()


def selected_specs(names: list[str]) -> list[DatasetSpec]:
    by_name = {spec.name: spec for spec in DEFAULT_DATASETS}
    if names:
        missing = [name for name in names if name not in by_name]
        if missing:
            raise RuntimeError(
                "Unknown dataset name(s): "
                + ", ".join(missing)
                + ". Supported names: "
                + ", ".join(sorted(by_name))
            )
        return [by_name[name] for name in names]
    return list(DEFAULT_DATASETS)


def available_dataset_classes() -> dict[str, Any]:
    try:
        import moabb
        from moabb import datasets
    except ImportError as exc:
        raise RuntimeError(
            "MOABB is required for dataset download. Run this script with "
            "`uv run --script` so the inline dependencies are installed."
        ) from exc

    configure_quiet_download_stack()
    moabb.set_log_level("warning")
    return {
        name: getattr(datasets, name)
        for name in dir(datasets)
        if not name.startswith("_")
    }


def instantiate_dataset(spec: DatasetSpec) -> Any:
    dataset_cls = available_dataset_classes().get(spec.name)
    if dataset_cls is None:
        raise RuntimeError(
            f"MOABB does not expose dataset {spec.name!r} in this environment."
        )
    return dataset_cls(**spec.constructor_kwargs)


def current_monitor() -> DatasetMonitor | None:
    return getattr(_THREAD_MONITOR, "monitor", None)


def format_bytes(value: int) -> str:
    if value < 1024:
        return f"{value} B"
    units = ("KiB", "MiB", "GiB", "TiB")
    size = float(value)
    unit = units[0]
    for unit in units:
        size /= 1024.0
        if size < 1024.0 or unit == units[-1]:
            break
    return f"{size:.1f} {unit}"


def format_bytes_progress(completed: int, total: int | None) -> str:
    if total is None:
        return format_bytes(completed) if completed > 0 else ""
    return f"{format_bytes(min(completed, total))} / {format_bytes(total)}"


def set_current_file(url: str, file_name: str | None = None) -> None:
    monitor = current_monitor()
    if monitor is None:
        return

    parsed = urlparse(url)
    name = file_name or Path(parsed.path).name or url
    monitor.progress.update(
        monitor.task_id,
        total=None,
        completed=0,
        status=f"downloading {name}",
        detail="",
    )


def configure_quiet_download_stack() -> None:
    if getattr(configure_quiet_download_stack, "_configured", False):
        return

    import pooch
    from urllib3.exceptions import InsecureRequestWarning

    import moabb.datasets.download as moabb_download

    warnings.filterwarnings(
        "ignore",
        message=r"Setting non-standard config type: .*",
        category=RuntimeWarning,
    )
    warnings.filterwarnings("ignore", category=InsecureRequestWarning)
    pooch.get_logger().setLevel(logging.WARNING)

    original_choose_downloader = moabb_download.choose_downloader
    original_retrieve = moabb_download.retrieve

    def quiet_choose_downloader(url: str, progressbar: bool = False):
        monitor = current_monitor()
        set_current_file(url)
        progress_proxy = RichDownloadProxy(monitor) if monitor is not None else False
        downloader = original_choose_downloader(url, progressbar=progress_proxy)
        if type(downloader).__name__ in ["HTTPDownloader", "DOIDownloader"]:
            downloader.kwargs.setdefault("verify", False)
        return downloader

    def quiet_retrieve(*args, **kwargs):
        url = kwargs.get("url")
        if url is None and args:
            url = args[0]
        fname = kwargs.get("fname")
        if fname is not None and url is not None:
            set_current_file(str(url), str(fname))
        kwargs["progressbar"] = False
        if "downloader" in kwargs and kwargs["downloader"] is not None:
            downloader = kwargs["downloader"]
            if hasattr(downloader, "kwargs"):
                downloader.kwargs.setdefault("verify", False)
        return original_retrieve(*args, **kwargs)

    moabb_download.choose_downloader = quiet_choose_downloader
    moabb_download.retrieve = quiet_retrieve
    configure_quiet_download_stack._configured = True


def dataset_entry(dataset: Any, paths: list[str | Path]) -> dict[str, Any]:
    metadata = getattr(dataset, "metadata", None)
    acquisition = getattr(metadata, "acquisition", None)
    participants = getattr(metadata, "participants", None)
    return {
        "name": dataset.__class__.__name__,
        "code": getattr(dataset, "code", dataset.__class__.__name__),
        "subjects": len(getattr(dataset, "subject_list", [])),
        "sessions_per_subject": getattr(dataset, "n_sessions", None),
        "sampling_rate": getattr(acquisition, "sampling_rate", None),
        "n_subjects": getattr(participants, "n_subjects", None),
        "paths": [str(path) for path in paths],
    }


def normalize_paths(paths: Any) -> list[str]:
    if paths is None:
        return []
    if isinstance(paths, (str, Path)):
        return [str(paths)]
    if isinstance(paths, (list, tuple, set)):
        flattened: list[str] = []
        for value in paths:
            flattened.extend(normalize_paths(value))
        return flattened
    return [str(paths)]


def materialize_dataset_paths(
    dataset: Any,
    data_dir: Path,
    force_update: bool,
) -> list[str]:
    paths: list[str] = []
    data_path_sig = signature(dataset.data_path)

    for subject in dataset.subject_list:
        kwargs: dict[str, Any] = {
            "subject": subject,
            "path": str(data_dir),
            "force_update": force_update,
            "update_path": False,
            "verbose": None,
        }
        if "accept" in data_path_sig.parameters:
            kwargs["accept"] = True

        for name, param in data_path_sig.parameters.items():
            if name in kwargs or name == "self":
                continue
            if param.default is not Parameter.empty:
                continue
            if hasattr(dataset, name):
                kwargs[name] = getattr(dataset, name)
            else:
                raise RuntimeError(
                    f"{dataset.__class__.__name__}.data_path() requires "
                    f"unsupported argument {name!r}"
                )

        paths.extend(normalize_paths(dataset.data_path(**kwargs)))

    # Keep ordering stable while removing duplicates.
    return list(dict.fromkeys(paths))


def cleanup_dataset_artifacts(dataset: Any, paths: list[str]) -> None:
    dataset_name = dataset.__class__.__name__
    if not dataset_name.startswith("Castillos"):
        return

    if not paths:
        return

    first_path = Path(paths[0])
    extracted_root = first_path.parents[1]
    archive_path = extracted_root.parent / f"{extracted_root.name}.zip"
    if archive_path.is_file() and extracted_root.is_dir():
        archive_path.unlink()


def download_dataset(
    spec: DatasetSpec,
    data_dir: Path,
    force_update: bool,
    progress: Progress,
    task_id: TaskID,
) -> dict[str, Any]:
    progress.update(task_id, visible=True, status="starting")
    progress.start_task(task_id)
    capture = TailCapture()
    _THREAD_MONITOR.monitor = DatasetMonitor(progress=progress, task_id=task_id)

    try:
        with contextlib.redirect_stdout(capture), contextlib.redirect_stderr(
            capture
        ):
            dataset = instantiate_dataset(spec)
            progress.update(task_id, status="downloading")
            paths = materialize_dataset_paths(
                dataset,
                data_dir,
                force_update,
            )
            cleanup_dataset_artifacts(dataset, paths)
        entry = dataset_entry(dataset, paths)
        progress.update(
            task_id,
            total=1,
            completed=1,
            status=f"done ({entry['subjects']} subjects)",
            detail="",
        )
        return entry
    except Exception as exc:
        tail = capture.tail()
        detail = f"{exc}"
        if tail:
            detail = f"{detail}\n{tail}"
        progress.update(task_id, status="failed", detail="")
        raise RuntimeError(detail) from exc
    finally:
        _THREAD_MONITOR.monitor = None


def main() -> None:
    args = parse_args()
    data_dir = args.data_dir.resolve()
    data_dir.mkdir(parents=True, exist_ok=True)
    if args.jobs < 1:
        raise SystemExit(f"--jobs must be >= 1, got {args.jobs}")

    # Keep MNE/MOABB cache writes rooted at the requested dataset directory.
    os.environ.setdefault("MNE_DATA", str(data_dir))

    specs = selected_specs(args.dataset)
    available_classes = available_dataset_classes()
    unavailable_specs = [
        spec.name for spec in specs if spec.name not in available_classes
    ]
    specs = [spec for spec in specs if spec.name in available_classes]
    console = Console()
    if unavailable_specs:
        console.print(
            "[yellow]Skipping unsupported datasets in this MOABB version:[/yellow] "
            + ", ".join(unavailable_specs)
        )
    if not specs:
        raise SystemExit("No supported datasets remain to download.")

    manifest_by_name: dict[str, dict[str, Any]] = {}
    overall_progress = Progress(
        SpinnerColumn(),
        TextColumn("{task.description}"),
        TimeElapsedColumn(),
        TextColumn("{task.fields[status]}"),
        console=console,
        transient=False,
    )
    progress = Progress(
        SpinnerColumn(),
        TextColumn("{task.description}"),
        BarColumn(),
        TaskProgressColumn(),
        TimeRemainingColumn(),
        TimeElapsedColumn(),
        TextColumn("{task.fields[status]}"),
        TextColumn("{task.fields[detail]}"),
        console=console,
        transient=False,
    )

    with Live(
        Group(overall_progress, progress),
        console=console,
        refresh_per_second=10,
        transient=False,
    ):
        overall_task = overall_progress.add_task(
            "datasets",
            status=f"0 / {len(specs)} complete",
        )
        dataset_tasks = {
            spec.name: progress.add_task(
                spec.name,
                total=None,
                status="queued",
                detail="",
                start=False,
                visible=False,
            )
            for spec in specs
        }

        if args.jobs == 1:
            failures: dict[str, str] = {}
            for done_count, spec in enumerate(specs, start=1):
                try:
                    manifest_by_name[spec.name] = download_dataset(
                        spec,
                        data_dir,
                        args.force_update,
                        progress,
                        dataset_tasks[spec.name],
                    )
                    overall_progress.update(
                        overall_task,
                        status=f"{done_count} / {len(specs)} complete",
                    )
                except BaseException as exc:
                    failures[spec.name] = str(exc)
                    overall_progress.update(
                        overall_task,
                        status=f"{len(manifest_by_name)} / {len(specs)} complete",
                    )
        else:
            failures = {}
            with concurrent.futures.ThreadPoolExecutor(
                max_workers=args.jobs,
                thread_name_prefix="cvep-download",
            ) as executor:
                future_to_spec = {
                    executor.submit(
                        download_dataset,
                        spec,
                        data_dir,
                        args.force_update,
                        progress,
                        dataset_tasks[spec.name],
                    ): spec
                    for spec in specs
                }
                done_count = 0
                for future in concurrent.futures.as_completed(future_to_spec):
                    spec = future_to_spec[future]
                    try:
                        manifest_by_name[spec.name] = future.result()
                    except BaseException as exc:  # pragma: no cover - operational path
                        failures[spec.name] = str(exc)
                    else:
                        done_count += 1
                    overall_progress.update(
                        overall_task,
                        status=f"{done_count} / {len(specs)} complete",
                    )

    if failures:
        console.print("[red]Dataset download failures:[/red]")
        for spec in specs:
            if spec.name in failures:
                console.print(f"[red]- {spec.name}: {failures[spec.name]}[/red]")
        raise SystemExit(f"{len(failures)} dataset download(s) failed")

    manifest = [manifest_by_name[spec.name] for spec in specs]

    manifest_path = args.manifest_json or (data_dir / "download_manifest.json")
    manifest_path.write_text(
        json.dumps(
            {
                "root": str(data_dir),
                "datasets": manifest,
            },
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )
    print(f"manifest={manifest_path}")


if __name__ == "__main__":
    main()
