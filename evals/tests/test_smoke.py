from collections.abc import Callable

import pytest
from inspect_ai.log import EvalLog
from inspect_ai.scorer import CORRECT, INCORRECT


@pytest.mark.parametrize(
    ("completion", "expected_score"), [("violet", CORRECT), ("green", INCORRECT)]
)
def test_smoke_scores_a_mock_completion(
    run_smoke: Callable[[str], EvalLog], completion: str, expected_score: str
) -> None:
    log = run_smoke(completion)

    assert log.status == "success"
    assert log.samples is not None
    (sample,) = log.samples
    assert sample.output.completion == completion
    assert sample.scores is not None
    assert sample.scores["exact"].value == expected_score
