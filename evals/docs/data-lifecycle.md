# Preserve evaluation data provenance

1. Record the source revision or download location, access date, license, and
   any collection restrictions. Store third-party inventories and pointers in
   `data/external/<suite>/`.
2. Put immutable inputs in `data/raw/<suite>/`. Add a new version when inputs
   change so earlier runs can still identify the data they used. Commit small
   redistributable fixtures and keep restricted or large downloads local.
3. Write intermediate transformation outputs to `data/interim/<suite>/`.
   Document the source inputs and versioned commands that reproduce them.
4. Write evaluation-ready derived data to `data/processed/<suite>/`. Retain its
   input revision and preparation command. Generated content in both derived
   directories is ignored by default. A suite can explicitly track a reviewed
   small derived fixture.

Give evaluation samples stable IDs so you can trace logged results back to their
input records.
