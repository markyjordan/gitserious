# Evaluation data

Keep immutable source inputs in [raw](raw/) and document third-party sources in
[external](external/). Write intermediate files to [interim](interim/) and
evaluation-ready derivatives to [processed](processed/). Keep each suite's
files in a named subdirectory as it grows.

The raw smoke fixture uses one Inspect sample per JSONL line with a stable
`id`, a prompt in `input`, and an expected answer in `target`. It is synthetic
and exists to check fixture loading and scoring.

See [the data lifecycle](../docs/data-lifecycle.md) for provenance, retention,
and source revision rules.
