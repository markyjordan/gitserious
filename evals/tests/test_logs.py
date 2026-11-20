from collections.abc import Callable
from pathlib import Path

import pytest
from inspect_ai.log import EvalLog, read_eval_log
from inspect_ai.scorer import CORRECT, INCORRECT


@pytest.mark.parametrize(
    ("completion", "expected_score"), [("violet", CORRECT), ("green", INCORRECT)]
)
def test_smoke_round_trips_a_native_log(
    tmp_path: Path, run_smoke: Callable[[str], EvalLog], completion: str, expected_score: str
) -> None:
    run_smoke(completion)
    (path,) = tmp_path.glob("*.eval")

    log = read_eval_log(str(path))

    assert log.status == "success"
    assert log.eval.task == "gitserious_evals/smoke"
    assert log.eval.model == "mockllm/offline-smoke"
    assert log.samples is not None
    (sample,) = log.samples
    assert sample.id == "smoke-001"
    assert sample.scores is not None
    assert sample.scores["exact"].value == expected_score
