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
