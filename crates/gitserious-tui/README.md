# gitserious-tui

## Unified interactive shell

Bare `gitserious` opens Home with centered Home, Commit, Config, and Help root
choices. Home centers the package version inside a full-width bordered
content block directly beneath the navigation frame. Command footers show
`key: action` hints; popups show `[key] action`. Crowded footers drop arrow
hints first. Tab advances and Shift+Tab reverses through root sections. Bare
invocation without terminal streams prints help. Home, Library, and Help work
outside a repository.

`gitserious config` opens the shell inside Config. The root row stays visible
above the configuration Menu and its Project and Library widgets, described
under Configuration. From the Project or Library browser, Tab and Shift+Tab
switch root sections, Esc returns to Home, and `q` quits; the config footer adds
`tab/shift+tab: switch`. Every other key, including `/` and `?`, reaches the
configuration interface. Config selections and unapplied changes survive
section visits. Library EDIT keeps focus inside Config until Esc validates and
saves the item. Tab, root-row clicks, and quit cannot leave EDIT. Project edits
and retained Commit reviews keep their existing discard confirmation.

The Commit overview shows repository/branch identity and directory trees for
Staged, Unstaged, and Untracked changes. Partially staged files appear in both
applicable groups. Up/Down selects rows; Left/Right collapses or expands folders.
`r` refreshes and Enter starts authoring when staged changes and valid persisted
project policy exist. Conflicts block authoring. These actions are also available
through `/`. Stage changes using Git; the overview never modifies the index.

Shell authoring reuses the existing picker, composer, and review. Its header is
`Commit / select taxonomy and type / compose message / review and submit`,
with the active step highlighted. The parent is clickable and protects dirty
drafts. Submission restores the terminal while Git runs, then returns to a
refreshed overview with the result; `o` opens a scrollable full result. A
rejected commit retains its reviewed message and composer; Enter resumes review. `c` starts a new commit. Quit asks
before discarding a retained review. Direct `gitserious commit` still opens
authoring immediately and exits with the existing Git result.

Help supplies man-style Overview, Config, Commit, Init, and Keyboard
Navigation pages; Left/Right changes pages and Up/Down/Page Up/Page Down scrolls.

## Configuration

`gitserious config` opens Project and Library beside a Menu. Up and Down choose
a Menu entry. Left and Right move focus between Menu and content. Clicking a
Menu entry switches to it while browsing.

Project shows the commit default, the project's own taxonomies, and
`gitserious.toml` status. Enter on its Taxonomies section opens the same outline
and Inspector the Library uses, over the project's own definitions: create,
fork, edit, and delete taxonomies, types, and properties, including the
project's copy of a built-in such as `conventional`. These edits stay staged
until Project review applies them, and the project default cannot be deleted.
Left at the outline's root returns to the sections.

In the project outline, `i` lists the Library's taxonomies to copy into the
project, each marked new, already in the project, or different from the
project's copy. A new one is copied in; an identical one is left alone; a
different one asks whether to replace the project's copy or add the Library's
as `<name>-copy`. A copy is independent: later Library edits never reach the
project. Only this action reads the Library. It never reads the Library, so it works when the
Library is missing or corrupt. An uninitialized repository offers `i` to review
initialization, which copies the Library's default taxonomies. Ctrl+S opens
Project review, and Enter applies `gitserious.toml`; Apply refuses if the file
changed on disk after Project opened. Browsing Library preserves pending Project
edits. A Library action asks you to apply or discard those Project edits before
proceeding. Commit authoring uses `gitserious.toml` as last applied.

Library has a taxonomy outline and an Inspector. Enter opens or closes a node,
Up and Down select rows, and Page Up and Page Down scroll Inspector details.
Built-in taxonomies show their origin in the outline and read-only status in
the Inspector. `f` forks a taxonomy, `n` creates one, `e` opens EDIT, and `d`
asks for deletion confirmation. Expanded custom taxonomies and types include
add rows for types and properties. A custom property's `c` opens a picker for
copying it to other types in the same taxonomy. Alt+Up and Alt+Down reorder a
type or property. The `/` palette lists the available Library actions.

In EDIT, Enter advances through fields, Up and Down move between fields, and
Left and Right move the text cursor or cycle property policy. Esc validates
and saves the item. Missing descriptions, invalid or duplicate normalized
names, and incomplete conditional policies keep EDIT open, focus the first
invalid field, and show inline errors. Tab, navigation clicks, and quit cannot
leave EDIT. Esc removes a new item without writing only when all its contents
still equal the initial blank values. A partly completed item must be corrected.

Names such as `Team Changes` save as `team-changes`. Name and Condition accept
80 graphemes; Description and Rationale accept 100. Loaded content beyond those
limits remains intact and may shrink. Edited descriptions and rationales are
trimmed, while unchanged loaded prose is preserved. A no-op edit performs no
write or version increment.

A taxonomy may be saved without types. It shows `▲ invalid`, and the Inspector
explains: "Add a type before this taxonomy can be included in a project."
Types and properties use inline field errors without invalid badges. Saved
eligibility stays unchanged while a child is being edited. A taxonomy's name
locks after its first save with types and remains locked if its last type is
removed. Existing records default to locked names. New taxonomies start at
version 1, and each substantive successful save increments an existing version
once. Forks start at version 1 and record informational source lineage.

Copy, reorder, and confirmed deletion save immediately. All Library writes use
the canonical `config.toml` and compare-and-swap storage. A failed write or
concurrent change preserves the edit or originating interaction and shows the
storage error. Successful saves retain node identities, selection, expansion,
and scroll position. Library has no review, apply, pending count, or discard
flow. Library saves leave global selections and Project files unchanged.

Configuration requires 66 columns and 18 rows, or 21 rows inside the shell.
Resizing retains editor state. The outline, Inspector, and footer adapt to
compact and wide terminals.

## Commit authoring

`gitserious commit --taxonomy <TAXONOMY>` selects one taxonomy from
`gitserious.toml` for that commit. `--type` preselects a type within it. With no
taxonomy option, authoring starts from the project default. Picker tabs support Tab/Shift+Tab to switch forward/backward and
show taxonomy identities and never mutate project policy.

The composer retains the established schema-document interaction: immutable
section headings, contextual property guidance, completion markers, explicit
conditional applicability, canonical review, and
discard confirmation. Generated messages contain `Gitserious-Taxonomy` and
`Gitserious-Schema` provenance trailers.
