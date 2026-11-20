# Evaluation workspace architecture

A suite owns its task, sample inputs, and scoring choices. Inspect discovers its
`@task` function and handles model calls, scoring, and native logs. The smoke
task exercises these paths with a synthetic sample and the built-in exact scorer.

The [data lifecycle](data-lifecycle.md) separates versioned source inputs from
derived data. The Python package holds reusable code. The `artifacts/` directory
holds generated logs and disposable scratch files. Reviewed findings and graphics
belong in `reports/`.

For reproducibility, record the Git revision, data revision, task and arguments,
resolved model identifier, Python version, and the locked environment used by
the run. Keep the native `.eval` log with that record. Use Inspect's public
`read_eval_log()` API or its viewer to inspect results.
