# gitserious evaluations

This workspace uses Inspect AI to run gitserious evaluations. Install it
separately from the Rust application.

## Set up the workspace

Install [uv](https://docs.astral.sh/uv/getting-started/installation/) and Make.
From the repository root, run:

```sh
cd evals
uv python install 3.12
make setup
make lint
make format-check
make test
```

`make setup` syncs the workspace with `uv.lock` on CPython 3.12.
The lock includes Hatchling so uv can use the same build backend version for
the editable install.

## Find workspace files

- `configs/` holds suite configuration.
- `data/` holds source inputs and generated datasets.
- `docs/` holds contributor guides.
- `notebooks/` holds exploratory analysis and `references/` holds source citations.
- `src/gitserious_evals/` holds package code and `tests/` holds verification.
- `reports/` holds curated findings and `artifacts/` holds local run output.

## Run the synthetic smoke task

From `evals/`, run `make smoke` to check known correct and incorrect
answers with an offline Inspect mock. `make test` also checks the data and
reads both outcomes back from native `.eval` logs in temporary directories.

To keep an inspectable smoke log, run this from `evals/`:

```sh
uv run --locked --offline python - <<'PY'
from inspect_ai import eval
from inspect_ai.model import ModelOutput, ModelUsage, get_model

output = ModelOutput.from_content("offline-smoke", "violet")
output.usage = ModelUsage(input_tokens=6, output_tokens=1, total_tokens=7)
model = get_model("mockllm/offline-smoke", custom_outputs=[output])
eval(
    "src/gitserious_evals/evaluations/smoke.py",
    model=model,
    log_dir="artifacts/logs",
    log_format="eval",
    ctl_server=False,
)
PY
make view
```

The sample and token counts are synthetic. The result demonstrates that
Inspect can run the task and preserve a score.

Copy `.env.example` to `.env` to use `artifacts/logs/` by default in future
Inspect runs. Keep native logs and disposable outputs under `artifacts/`.
Use `reports/` for reviewed results and graphics.
