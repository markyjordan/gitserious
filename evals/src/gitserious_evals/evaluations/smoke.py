"""A synthetic task that exercises Inspect generation and scoring."""

from inspect_ai import Task, task
from inspect_ai.dataset import Sample
from inspect_ai.scorer import exact
from inspect_ai.solver import generate


@task
def smoke() -> Task:
    return Task(
        dataset=[Sample(id="smoke-001", input="Reply with the word violet.", target="violet")],
        solver=generate(),
        scorer=exact(),
    )
