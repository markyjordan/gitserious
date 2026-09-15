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

A user can initialize repository-local policy, choose a taxonomy for one commit,
author its structured properties, review the exact message, and create the staged
Git commit. A configuration TUI lets the user define complete taxonomies and
manage global defaults plus project-local selection overrides.

Ship:

- The existing Conventional taxonomy, plus Research and Infra Ops taxonomies.
- Per-commit taxonomy selection with a persistent active project default.
- Complete configuration authoring and management through a TUI.
- Consistent property validation, including explicit conditional applicability.
- Taxonomy and schema provenance in generated commit messages.
- Installation and workflow documentation, plus existing release verification.

Defer from v0.1.0:

- Standalone message validation and automatic hook installation.
- The `gsrs` alias and further Conventional taxonomy iteration.
- Taxonomy composition, external artifact integrations, and additional defaults.
- Noninteractive agent authoring, MCP, Lore, AST parsing, and repository graphs.
- Hypothesis evaluation and claims of measured reconstruction improvements.

Existing ordinary Git hooks and signing behavior still run through Git. Deferring
hook installation does not authorize bypassing hooks or changing signing policy.

### Taxonomy configuration contracts

A taxonomy owns its ordered commit types and every type's ordered durable
properties. There is no selectable template or independently configurable
typeset. Built-ins are immutable. Custom taxonomy identities remain immutable,
semantic edits advance versions, and forks record informational source lineage
without inheriting later updates.

Global configuration stores the custom taxonomy library, one default taxonomy,
and the ordered set available to projects. Project configuration stores optional
default and available-set overrides; each field inherits global independently.
The effective default must be available.

Generated locks contain the complete effective taxonomy snapshots. Commit
authoring uses this portable pinned policy without consulting mutable global
configuration. Global changes appear as updates in `gitserious config` and take
effect only after reviewed refresh. A project-config/lock mismatch blocks commit
creation with an actionable path through the Project configuration screen.

### Configuration user experience

Bare `gitserious config` opens the configuration TUI. Taxonomy-focused
`config list` and `config show` remain available for noninteractive inspection.

Support browse, create, edit, fork, select, and delete where applicable.
Provide forms for taxonomy types, property descriptions and ordering, requirement
levels and condition rationales, and multiplicity. Creation and forking add to
the library; separate Global and Project screens manage availability and defaults.

Keep related edits in a session and review the complete change before applying
it. An intermediate draft can be incomplete; saved configuration must be valid.
Reject unknown taxonomy references, invalid schemas, identity conflicts, and
deleting a globally selected taxonomy. Cancellation, invalid edits, concurrent
changes, and failed saves preserve stored configuration.

### Commit user experience and interfaces

Add `gitserious commit --taxonomy <id>` alongside the existing `--type` option.
Without an explicit taxonomy, begin with the pinned project default. Resolve
`--type` only within the selected taxonomy; do not search other taxonomies for a
matching identifier.

Carry the selected taxonomy identity with the authored draft through validation,
review, and commit creation. Overlapping types such as Conventional `fix` and Research
`fix` must never share a schema accidentally.

The picker displays available taxonomies and the selected taxonomy's change types.
Temporary selection does not
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

Append these two application-generated trailers in the listed order:

```text
Gitserious-Taxonomy: <id>@<version>
Gitserious-Schema: sha256:<resolved-schema-fingerprint>
```

Use the selected resolved schema and the existing semantic fingerprint machinery;
do not infer provenance from the type identifier or mutable global state. Show
trailers in the exact-message review and commit those reviewed bytes. Provenance
is not an editable durable property. It identifies the schema, not the truth of
authored claims. Keep trailers structural and unwrapped while preserving current
body and breaking-change rendering.

### Default taxonomies

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
completion. No new domain properties are conditional in v0.1; custom taxonomies and
the existing Conventional bundle still exercise conditional requirements.

#### Research

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

- [ ] All three built-in taxonomies resolve through the generic catalog; every type
  has a schema and Conventional definitions remain unchanged.
- [ ] Custom global definitions support complete authoring, taxonomy forks,
  default/availability selection, and safe deletion.
- [ ] Invalid references, cancellation, concurrent edits,
  and persistence failures preserve stored state.
- [ ] Project locks contain portable schemas; stale policy blocks commits with
  actionable repair output, including in linked worktrees.
- [ ] Commits using all three built-ins and a custom taxonomy use the selected
  schema, including overlapping type identifiers.
- [ ] Temporary selection leaves project files unchanged and does not depend on
  later edits to global configuration.
- [ ] Required, recommended, optional, conditional, and repeatable properties
  behave consistently, including custom label collisions and long names.
- [ ] Git receives exactly the reviewed message, including provenance and
  breaking-change content; cancellation creates no commit.
- [ ] Fresh-install walkthroughs cover first commit, taxonomy switching, and
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
- Iterate on the Conventional taxonomy after the initial release.

## v0.2

Deliver Lore indexing and query capabilities for recovering historical context.
Use evaluation findings to guide retrieval work and avoid assuming richer
messages necessarily improve downstream reasoning.

## Later roadmap

AST parsing, repository graph representation, and history/structure integration
remain later work. They are not prerequisites for v0.1 or commitments that every
such feature will ship in v0.2. Revisit other deferred surfaces using product
experience and evaluation evidence.
