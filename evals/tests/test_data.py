from pathlib import Path

import pytest
from inspect_ai import eval

from gitserious_evals.evaluations.smoke import smoke


def test_smoke_reads_the_versioned_sample() -> None:
    (sample,) = smoke().dataset

    assert sample.id == "smoke-001"
    assert sample.input == "Reply with the word violet."
    assert sample.target == "violet"


@pytest.mark.parametrize(
    ("record", "error"),
    [
        ("{", "(?i)invalid json"),
        ('{"id": "a", "target": "violet"}', "input"),
        ('{"id": "a", "input": "Prompt"}', "nonempty text target"),
        ('{"input": "Prompt", "target": "violet"}', "missing a stable sample ID"),
        ('{"id": "  ", "input": "Prompt", "target": "violet"}', "missing a stable sample ID"),
    ],
)
def test_smoke_rejects_invalid_data(tmp_path: Path, record: str, error: str) -> None:
    path = tmp_path / "invalid.jsonl"
    path.write_text(record + "\n", encoding="utf-8")

    with pytest.raises(ValueError, match=error):
        smoke(str(path))


def test_inspect_rejects_duplicate_sample_ids(tmp_path: Path) -> None:
    path = tmp_path / "duplicate.jsonl"
    path.write_text(
        '{"id": "a", "input": "Prompt", "target": "violet"}\n'
        '{"id": "a", "input": "Another prompt", "target": "green"}\n',
        encoding="utf-8",
    )
    task = smoke(str(path))

    with pytest.raises(Exception, match="duplicate sample ids"):
        eval(
            task,
            model="mockllm/offline-smoke",
            log_dir=str(tmp_path),
            run_samples=False,
            display="none",
        )
