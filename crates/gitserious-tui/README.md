# gitserious-tui

## Configuration

Bare `gitserious config` opens a project-first configuration home. The framed
Scope tab groups Project and Global policy; Taxonomies opens the library
directly. Its Library list sits beside Overview and Types panes. `n`, `e`, `f`,
and `d` open the existing create, edit, fork, and delete flows. A framed message
row shows errors and action status below the content.

Global configuration owns the user taxonomy library, default taxonomy, and
ordered set available to new project policy. Project configuration may override
the default and available set independently or inherit either field. An
uninitialized repository offers an Initialize action that reviews the portable
policy before creating `gitserious.toml` and `gitserious.lock`.

Taxonomy Overview shows the selected definition's description, origin, version,
type count, and fork lineage where present. Right opens the Types pane;
`up`/`down` selects a type and `enter` inspects its durable properties. Built-ins
may be forked; custom taxonomies may also be edited or deleted. Focused child
screens return to the selected taxonomy on completion. Fork lineage is
informational and never updates.

Changes remain in memory until `ctrl+s` opens semantic review and `enter`
applies them. Global saves use compare-and-swap. Project saves atomically replace
authored overrides and the complete portable lock. Failed saves retain the
draft. Browsing the library preserves a dirty Project draft; a Global library
action requires an explicit apply or discard decision before it proceeds.

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
