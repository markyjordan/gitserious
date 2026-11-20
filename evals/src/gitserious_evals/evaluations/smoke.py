"""A synthetic task that exercises Inspect generation and scoring."""

from pathlib import Path

from inspect_ai import Task, task
from inspect_ai.dataset import json_dataset
from inspect_ai.scorer import exact
from inspect_ai.solver import generate

SMOKE_DATA = Path(__file__).resolve().parents[3] / "data" / "raw" / "smoke.jsonl"


@task
def smoke(dataset_path: str | None = None) -> Task:
    path = Path(dataset_path) if dataset_path is not None else SMOKE_DATA
    dataset = json_dataset(str(path))

    for row, sample in enumerate(dataset, start=1):
        if sample.id is None or not str(sample.id).strip():
            raise ValueError(f"{path}: sample {row} is missing a stable sample ID")
        if not isinstance(sample.target, str) or not sample.target.strip():
            raise ValueError(f"{path}: sample {row} needs a nonempty text target")

    return Task(dataset=dataset, solver=generate(), scorer=exact())
