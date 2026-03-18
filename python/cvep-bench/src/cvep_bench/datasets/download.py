from __future__ import annotations

import argparse
import concurrent.futures
import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from rich.console import Console


@dataclass(frozen=True)
class DatasetSpec:
    name: str
    constructor_kwargs: dict[str, Any]


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


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--data-dir",
        type=Path,
        default=Path(__file__).resolve().parents[4] / "crates/cvep-decoder/data",
    )
    parser.add_argument("--dataset", action="append", default=[])
    parser.add_argument("--force-update", action="store_true")
    parser.add_argument("--manifest-json", type=Path, default=None)
    parser.add_argument("--jobs", type=int, default=1)
    return parser.parse_args()


def selected_specs(names: list[str]) -> list[DatasetSpec]:
    by_name = {spec.name: spec for spec in DEFAULT_DATASETS}
    if names:
        missing = [name for name in names if name not in by_name]
        if missing:
            raise RuntimeError("Unknown dataset name(s): " + ", ".join(missing))
        return [by_name[name] for name in names]
    return list(DEFAULT_DATASETS)


def available_dataset_classes() -> dict[str, Any]:
    import moabb
    from moabb import datasets

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


def download_dataset(
    spec: DatasetSpec, data_dir: Path, force_update: bool
) -> dict[str, Any]:
    dataset = instantiate_dataset(spec)
    dataset.download(
        path=str(data_dir), force_update=force_update, update_path=False, verbose=False
    )
    return {"dataset": spec.name, "path": str(data_dir)}


def main() -> None:
    args = parse_args()
    args.data_dir.mkdir(parents=True, exist_ok=True)
    specs = selected_specs(args.dataset)
    console = Console()
    results = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
        futures = [
            pool.submit(download_dataset, spec, args.data_dir, args.force_update)
            for spec in specs
        ]
        for future in concurrent.futures.as_completed(futures):
            results.append(future.result())
            console.print(f"[green]downloaded[/green] {results[-1]['dataset']}")
    if args.manifest_json is not None:
        args.manifest_json.write_text(
            json.dumps(results, indent=2) + "\n", encoding="utf-8"
        )
