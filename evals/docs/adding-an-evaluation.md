# Add an evaluation suite

1. Create a module under `src/gitserious_evals/evaluations/<suite>/`. Define
   an Inspect `@task` that returns a `Task` with its dataset, solver, and scorer.
2. Put source and derived inputs in the corresponding `data/` subdirectories.
   Load the fixture with the native Inspect dataset API. Use stable sample IDs.
3. Add tests under `tests/<suite>/`. Use an explicit mock model to check task
   wiring and scoring offline.
4. Run `make lint format-check test` from `evals/`. Inspect discovers tasks by
   module path, so adding a suite takes no registry change.
5. Record the command and provenance for any run you keep. Use
   `artifacts/logs/` for native logs and `reports/` for reviewed findings.

[Inspect tasks](https://inspect.aisi.org.uk/tasks.html) and
[Inspect datasets](https://inspect.aisi.org.uk/datasets.html) document the APIs
used by the scaffold.
