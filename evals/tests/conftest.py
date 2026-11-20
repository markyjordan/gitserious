from collections.abc import Callable
from pathlib import Path

import pytest
from inspect_ai import eval
from inspect_ai.log import EvalLog
from inspect_ai.model import ModelOutput, ModelUsage, get_model

from gitserious_evals.evaluations.smoke import smoke


@pytest.fixture
def run_smoke(tmp_path: Path) -> Callable[[str], EvalLog]:
    def run(completion: str) -> EvalLog:
        output = ModelOutput.from_content("offline-smoke", completion)
        output.usage = ModelUsage(input_tokens=6, output_tokens=1, total_tokens=7)
        model = get_model("mockllm/offline-smoke", custom_outputs=[output])
        (log,) = eval(smoke, model=model, log_dir=str(tmp_path), log_format="eval", display="none")
        return log

    return run
