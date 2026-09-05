# gitserious roadmap

## v0.1.0: commit authoring and reusable configuration

This document specifies intended release behavior. An unchecked item is planned,
not a claim that the current binary implements it. The dependency-ordered work
and planned commit messages are in [DELIVERY.md](DELIVERY.md).

### Product hypothesis

Can domain-appropriate change categories and durable properties preserve
information at write time that reduces uncertainty and reconstruction cost when
someone later understands, reviews, or modifies a repository?

Capture intent, causal explanations, constraints, assumptions, and interpretations
that are expensive to reconstruct. Prefer deriving structural facts from code
when that is sufficient. More text alone is not evidence of useful information.
The hypothesis remains unproven; v0.1 delivers the authoring surface needed for
later evaluation, rather than making measured improvement a publication gate.

### Release boundary

A user can initialize repository-local policy, choose a template for one commit,
author its structured properties, review the exact message, and create the staged
Git commit. A configuration TUI lets the user define taxonomies and typesets,
assemble templates, and manage reusable global and project-local configuration.

Ship:

- The existing Conventional bundle, plus ML Research and Infra Ops bundles.
- Per-commit template selection with a persistent active project default.
- Complete configuration authoring and management through a TUI.
- Consistent property validation, including explicit conditional applicability.
- Template and schema provenance in generated commit messages.
- Installation and workflow documentation, plus existing release verification.

Defer from v0.1.0:

- Standalone message validation and automatic hook installation.
- The `gsrs` alias and further Conventional typeset iteration.
- Taxonomy composition, external artifact integrations, and additional defaults.
- Noninteractive agent authoring, MCP, Lore, AST parsing, and repository graphs.
- Hypothesis evaluation and claims of measured reconstruction improvements.

Existing ordinary Git hooks and signing behavior still run through Git. Deferring
hook installation does not authorize bypassing hooks or changing signing policy.

### Current foundation

At the reviewed integration baseline, `7e664db`, the project already provides
`init`, interactive `commit`, explicit configuration CLI subcommands, domain
configuration models, global persistence, and self-contained project policy.
The built-in catalog and picker still assume Conventional. Configuration backend
operations exceed what the CLI exposes. The composer and newer property validator
also differ on conditional applicability. These are the seams the release closes.

### Configuration and template contracts

Preserve `Template -> Taxonomy + Typeset`:

- A taxonomy defines change categories and their ordered descriptions.
- A typeset defines the ordered durable properties for every taxonomy type,
  including intentional empty schemas.
- A template chooses one taxonomy and one typeset.

Keep the existing Conventional definitions, versions, ordering, and `default`
template identity unchanged. Add `ml-research` and `infra-ops` template identities,
each referencing its same-named taxonomy and its taxonomy-scoped `default` typeset.
The two new bundles start at version 1. Do not introduce a competing Conventional
template identity merely to make names symmetrical.

Built-ins are immutable. Custom definitions use the same resolution and validation
model. Their identities remain immutable; semantic edits advance versions.

Global configuration stores reusable custom definitions. Project configuration
stores repository-owned custom definitions and one active template. Commit
selection uses all built-ins plus project-local custom templates. A global custom
template must be explicitly imported with its complete dependency chain before
it can be used by a project. Later global changes cannot alter project policy.

Keep authored project configuration compatible. Extend generated locks to cover
every selectable template, not only the active one. Support reading existing
locks so `gitserious init` can upgrade them atomically. Old or stale locks block
commit creation with a repair instruction. Guard configuration/lock updates using
the existing persistence protections and preserve files when migration fails.

### Configuration user experience

Bare `gitserious config` opens the configuration TUI; existing explicit CLI
subcommands remain available. The TUI provides explicit Global and Project
destinations and shows which destination each operation affects.

Support browse, create, edit, fork, import, select, and delete where applicable.
Provide forms for taxonomy types, property descriptions and ordering, requirement
levels and condition rationales, multiplicity, and template references. Permit
forking any available bundle into editable custom definitions.

Keep related edits in a session and review the complete change before applying
it. An intermediate draft can be incomplete; saved configuration must be valid.
Reject dangling references, invalid schemas, identity conflicts, and unsafe
active-template deletion. Cancellation, invalid edits, concurrent changes, and
failed saves preserve stored configuration. Import can copy a complete global
bundle alone or import and select it in one operation; conflicts never silently
replace project definitions.

### Commit user experience and interfaces

Add `gitserious commit --template <id>` alongside the existing `--type` option.
Without an explicit template, begin with the active project template. Resolve
`--type` only within the selected template; do not search other taxonomies for a
matching identifier.

Replace the flat authoring input with resolved template choices. Carry the
selected template identity with the authored draft through validation, review,
and commit creation. Overlapping types such as Conventional `fix` and ML Research
`fix` must never share a schema accidentally.

The picker displays available templates and the selected template's change types.
Distinguish custom templates sharing a taxonomy. Temporary selection does not
rewrite project configuration or the lock. Switching after editing requires the
existing discard confirmation before resetting the draft.

Use one requirement model throughout composition, review, and commit creation:

- Required omissions block.
- Recommended omissions warn but do not block.
- Optional omissions pass.
- Conditional fields require an explicit applicable/not-applicable choice.
  Applicable fields require a value; not-applicable fields reject a value.
- Repeatable properties allow adding/removing occurrences and preserve authored
  order. Single-valued properties reject duplicate occurrences.

Preserve field identity even when custom names overlap editor labels. Retain exact
message review, cancellation behavior, canonical Unicode prose wrapping, and
ordinary staged-index Git commit behavior.

### Commit provenance

Append these four application-generated trailers in the listed order:

```text
Gitserious-Template: <id>@<version>
Gitserious-Taxonomy: <id>@<version>
Gitserious-Typeset: <taxonomy>/<typeset>@<version>
Gitserious-Schema: sha256:<resolved-schema-fingerprint>
```

Use the selected resolved schema and the existing semantic fingerprint machinery;
do not infer provenance from the type identifier or mutable global state. Show
trailers in the exact-message review and commit those reviewed bytes. Provenance
is not an editable durable property. It identifies the schema, not the truth of
authored claims. Keep trailers structural and unwrapped while preserving current
body and breaking-change rendering.

### Default taxonomies and typesets

Conventional remains the current repository baseline:

```text
feat, fix, refactor, perf, test, docs, chore, build, ci, style, revert
```

Its versioned property definitions remain in
[the built-in source](../../../crates/gitserious-core/src/built_in.rs).
Do not replace them with an earlier conversational summary of those definitions.

For the two new domains, rows below define type order. Properties are listed in
schema order, including their requirement levels: **R** = required, **rec** =
recommended. All new properties are single-valued multiline text. Preserve the
agreed explicit levels, including required `reproduce.result`. For schemas where
the conversation omitted levels or definitions, this table records the adopted
completion. No new domain properties are conditional in v0.1; custom typesets and
the existing Conventional bundle still exercise conditional requirements.

#### ML Research

| Type | Meaning | Ordered properties |
| --- | --- | --- |
| `hypothesis` | Introduce or revise a falsifiable research hypothesis. | claim (R), motivation (R), prediction (R), falsifier (rec), assumptions (rec) |
| `data` | Change data, sampling, labels, splits, filtering, preprocessing, or augmentation. | objective (R), population (rec), transformation (R), assumptions (rec), leakage-risk (rec), validation (rec) |
| `model` | Change representation, architecture, objective, or inference formulation. | objective (R), change (R), rationale (R), assumptions (rec), tradeoffs (rec) |
| `experiment` | Introduce or modify an intervention or controlled comparison. | question (R), intervention (R), control (R), prediction (rec), confounders (rec), result (rec) |
| `eval` | Change how performance or behavior is measured. | target (R), protocol (R), metrics (R), rationale (rec), limitations (rec) |
| `analysis` | Interpret evidence or record diagnostic findings. | evidence (R), finding (R), interpretation (R), confidence (rec), next-question (rec) |
| `reproduce` | Attempt to reproduce or replicate an existing result. | source (R), target-result (R), deviations (rec), result (R), discrepancy (rec) |
| `fix` | Correct an implementation or experimental defect. | symptom (R), cause (R), affected-results (rec), decision (R), validation (rec) |
| `infra` | Change execution machinery without changing the intended experiment. | objective (R), change (R), experimental-impact (rec), reproducibility-impact (rec), validation (rec) |
| `docs` | Change the research knowledge surface. | intent (R), decision (R), audience (rec), validation (rec) |

#### Infra Ops

| Type | Meaning | Ordered properties |
| --- | --- | --- |
| `provision` | Introduce operational resources or systems. | purpose (R), topology (R), capacity-assumption (rec), failure-domain (rec), dependencies (rec), rollback (rec) |
| `configure` | Change operational configuration. | objective (R), change (R), rationale (R), assumptions (rec), rollback (rec), validation (rec) |
| `deploy` | Roll an artifact into an operational environment. | objective (R), artifact (R), environment (R), strategy (R), risk (rec), rollback (rec), validation (rec) |
| `migrate` | Move between operational states. | from-state (R), to-state (R), reason (R), compatibility (rec), invariants (rec), rollback (rec), validation (rec) |
| `scale` | Adjust capacity in response to a constraint or signal. | constraint (R), signal (R), change (R), capacity-assumption (rec), tradeoff (rec), validation (rec) |
| `observe` | Address a monitoring or diagnostic blind spot. | blind-spot (R), signal (R), interpretation (R), threshold (rec), response (rec), cost (rec) |
| `incident` | Record operational impact and mitigation. | symptom (R), impact (R), trigger (rec), cause (rec), mitigation (R), follow-up (rec) |
| `recover` | Restore service or data from a failed state. | failure-state (R), target-state (R), action (R), data-loss (rec), residual-risk (rec), validation (rec) |
| `secure` | Address an operational threat or exposure. | threat (R), exposure (R), control (R), assumptions (rec), residual-risk (rec), validation (rec) |
| `decommission` | Retire an operational resource or system. | target (R), reason (R), dependencies (rec), migration (rec), residual-state (rec), validation (rec) |

Each property needs author guidance describing the enduring information to
preserve. For example, `affected-results` identifies prior research conclusions
invalidated by a defect; `control` identifies the comparison used to interpret an
intervention; `rollback` describes the recovery path and its limits. Guidance
must not imply that supplying prose proves an experiment or operation succeeded.

### Release acceptance

- [ ] All three built-in bundles resolve through the generic catalog; every type
  has a schema and Conventional definitions remain unchanged.
- [ ] Custom global/project definitions support complete authoring, arbitrary
  bundle forks, import, default selection, and safe deletion.
- [ ] Invalid dependencies, import conflicts, cancellation, concurrent edits,
  and persistence failures preserve stored state.
- [ ] Old locks upgrade through initialization; stale policy blocks commits with
  actionable repair output, including in linked worktrees.
- [ ] Commits using all three built-ins and a custom template use the selected
  schema, including overlapping type identifiers.
- [ ] Temporary selection leaves project files unchanged and does not depend on
  later edits to global configuration.
- [ ] Required, recommended, optional, conditional, and repeatable properties
  behave consistently, including custom label collisions and long names.
- [ ] Git receives exactly the reviewed message, including provenance and
  breaking-change content; cancellation creates no commit.
- [ ] Fresh-install walkthroughs cover first commit, template switching, and
  global/project custom configuration.
- [ ] Repository-required quality and release checks pass. Verify all six current
  workspace crates in dependency-ordered packaging/publication.
- [ ] Existing native targets pass release verification: Linux x64, macOS Intel,
  macOS Apple Silicon, and Windows x64.
- [ ] README, architecture/configuration docs, changelog, and release guidance
  describe the delivered behavior rather than the earlier scaffold.

Use the established `dev -> main -> release/0.1` process, mandatory release dry
run, and existing publication approvals. Release mechanics are specified in
[RELEASE_POSTURE.md](../../eng/RELEASE_POSTURE.md); workflow code and hosted
controls remain the enforcement surfaces. Correct stale release documentation
rather than designing another pipeline. A release candidate remains optional
under the existing release posture. Hypothesis evaluation is not a release gate.

## After v0.1.0

- Conduct evals comparing ordinary user-authored commits with Gitserious-authored
  commits, including selection across the three default domains.
- Build MCP v2 in a v0.1 patch release.
- Iterate on the Conventional typeset after the initial release.

## v0.2

Deliver Lore indexing and query capabilities for recovering historical context.
Use evaluation findings to guide retrieval work and avoid assuming richer
messages necessarily improve downstream reasoning.

## Later roadmap

AST parsing, repository graph representation, and history/structure integration
remain later work. They are not prerequisites for v0.1 or commitments that every
such feature will ship in v0.2. Revisit other deferred surfaces using product
experience and evaluation evidence.
