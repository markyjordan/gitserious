# gitserious-tui

## Configuration

Bare `gitserious config` opens Project and Library tabs. Project shows the
repository's pinned commit taxonomy and available set when its lock is valid,
or a labeled inherited preview when commit authoring is blocked. Its Commit
section opens the focused Project override editor; Resolution shows the
Library baseline, Project overrides, current resolution, and pinned lock with
their sources.
Files shows root `gitserious.toml`, `gitserious.lock`, and lock status. A project
without configuration previews inherited values and offers `i` to review
initialization. Staged Project edits are labeled separately; commit authoring
continues using the persisted lock until those edits are applied.

Global configuration owns the user taxonomy library, default taxonomy, and
ordered set available to new project policy. Project configuration may override
the default and available set independently or inherit either field. An
uninitialized repository offers an Initialize action that reviews the portable
policy before creating `gitserious.toml` and `gitserious.lock`.

Library lists built-in and globally authored taxonomies with visible ownership
labels. Its inspector shows the selected taxonomy's description, provenance,
types, and selected type's durable properties together. Left and Right change
focus between taxonomies and types; Up and Down select within the focused list.
`n`, `e`, `f`, and `d` open focused create, edit, fork, and delete flows. Built-ins
may be forked; custom taxonomies may also be edited or deleted. Fork lineage is
informational and never updates.

Changes remain in memory until `ctrl+s` opens semantic review and `enter`
applies them. Global saves use compare-and-swap. Project saves atomically replace
authored overrides and the complete portable lock. Failed saves retain the
draft. Browsing Library preserves a dirty Project draft; a Library edit requires
an explicit apply or discard decision before it proceeds. The framed message
row remains available for errors and action status. Existing user-level default
and availability settings remain the read-only inheritance baseline in Project.

The interface shares the commit TUI's true-black canvas, dark Unicode frames,
yellow active rows and navigation strip, zebra tables, mouse targets, bracketed
paste, and a 60-column by 18-row minimum-size guard.

## Commit authoring

`gitserious commit --taxonomy <TAXONOMY>` selects one taxonomy from the portable
project lock for that commit. `--type` preselects a type within it. With no
taxonomy option, authoring starts from the pinned project default. Picker tabs
show taxonomy identities and never mutate project policy.

The composer retains the established schema-document interaction: immutable
section headings, contextual property guidance, completion markers, explicit
conditional applicability, repeatable property controls, canonical review, and
discard confirmation. Generated messages contain `Gitserious-Taxonomy` and
`Gitserious-Schema` provenance trailers.
