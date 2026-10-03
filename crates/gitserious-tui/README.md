# gitserious-tui

## Configuration

Bare `gitserious config` opens Project and Library tabs in a standalone Config
Options box. Project and Library widgets align with its outer edges and begin
directly below it, without an enclosing frame.
The selected tab uses yellow text on black with a yellow segment in the lower
border beneath its label. The label and underline both switch tabs when clicked.
Project shows the repository's pinned commit taxonomy and available set when
its lock is valid, or a labeled inherited preview when commit authoring is
blocked. Its Commit section opens the focused Project override editor;
Resolution shows the Library baseline, Project overrides, current resolution,
and pinned lock with their sources. Project text starts at the same inner border
edge used by Library panes.
Files shows root `gitserious.toml`, `gitserious.lock`, and lock status. A project
without configuration previews inherited values and offers `i` to review
initialization. Staged Project edits are labeled separately; commit authoring
continues using the persisted lock until those edits are applied.

Global configuration owns the user taxonomy library, default taxonomy, and
ordered set available to new project policy. Project configuration may override
the default and available set independently or inherit either field. An
uninitialized repository offers an Initialize action that reviews the portable
policy before creating `gitserious.toml` and `gitserious.lock`.

Library groups built-in and custom taxonomies beneath ruled headings with a
nonselectable blank row between sections. A separate Taxonomy Description box
sits below the list and shows the selected description and optional fork
lineage. An empty Custom section offers `+ create taxonomy`. Enter or a click
starts the reviewed create flow used by `n`. Once a custom taxonomy exists,
its rows replace the action. While the action is selected, Taxonomy
Description, Types, and Type Details show `n/a`, and focus stays on Taxonomies.
Wide terminals use three evenly spaced columns for Taxonomies,
Types, and Type Details; compact terminals stack Types and Type Details in the
right column.
Create Taxonomy has one Taxonomy frame with ruled Name and Description sections,
followed by a Types list. Section headings are bold white; the active widget's
border and title are yellow. Name is the single-line, lowercase, hyphenated
taxonomy ID; Description is multiline. One visible text cursor starts at the
beginning of Name. Left and Right move within text. Up and Down move through
Description and cross between Name, Description, and Types. Each text section
keeps its cursor position while away. Backspace, Delete, and paste edit at the
cursor. Enter advances or opens the selected type; Alt+Enter inserts a
Description newline, and Alt+/ inserts a literal slash.
Name and Description share an 80-visible-character limit, including Description
newlines. The bottom-right Taxonomy border shows the combined count; oversized
pastes insert only the portion that fits.
Clicks in Taxonomy text do not move its cursor; Types remains mouse selectable.
The `/` palette offers `add type` and `validate and stage taxonomy`. A
taxonomy needs at least one type before it can be staged. Staging returns to
Library, where `/` then `review changes` opens the existing review and apply flow.
Fork uses this same editor, populated with the selected taxonomy's name,
description, types, schema versions, and properties. The breadcrumb reads
`Library / fork taxonomy`. Rename the fork to an unused ID before staging.
Copied text is preserved in full, and Fork allows temporary totals above 80
while editing; validation alerts on an unchanged name or a total above 80.
Staging creates a snapshot at version 1 with lineage pointing to the selected
source ID and version. Source changes never propagate to the fork.
Edit Taxonomy retains its Metadata and type list.
In the Create flow, Config Options disappears immediately and returns after
the header settles back on Library. Project fades and moves away while Library
moves left into `[Library] / create taxonomy`; leaving reverses the motion.
The Taxonomy and Types widgets span the content width with no gap. Taxonomy
stays eight rows tall, with two Description input rows beneath its heading;
Types uses the remaining space. The Library breadcrumb uses the same moving
yellow underline. Its label and underline return from the main Create or Fork
view; in a child editor they confirm before discarding the whole draft.
Adding or editing a type in Create, Fork, or Edit Taxonomy opens an eight-row
Type frame with Name, Description, and an independent `used/80` counter.
Properties sits directly beneath it with a `+ add property` selector and
separate name, description, and policy columns. The active frame is yellow;
white ruled headings, arrow navigation, text editing, and mouse selection work
as they do in the Taxonomy view. Text-area clicks do not move the cursor.
The breadcrumb adds `add type` or `edit type`, then `add property` or
`edit property` while that child is open.

The Property frame contains Name, Description, and Policy, with a `used/100`
counter for Name and Description. New properties default to required policy
and a single value. Left and Right on Policy choose required, recommended,
or conditional. Conditional policy exposes Condition name and Condition
rationale; both are required and excluded from the counter. Existing Optional
policy, multiplicity, condition metadata, and schema versions are preserved
when copied items are edited. Choosing another policy explicitly replaces
Optional. Alt+Enter inserts a Description or Condition rationale newline.
Required and recommended Property views use an 11-row frame. Conditional
uses 17 rows, with a blank row after Policy and after Condition name, and two
Condition rationale input rows. On shorter terminals, the sections scroll to
keep the active heading and input visible while the frame and counter stay
fixed. Unused space below a compact frame stays empty.

`ctrl+s` completes a Property into the local Type draft, then completes the
Type into the taxonomy draft. Type and property names must be unique within
their parent. Types may have no properties. Adding properties does not require
the Type's text fields to be complete yet. New text uses hard input limits;
copied text is preserved without truncation and must fit its limit when that
item is completed. Untouched copied properties keep their original data.
Esc returns immediately when clean, or confirms discarding only that child
when changed. No child completion writes configuration files; taxonomy staging
and the existing review/apply flow remain the save boundary.
Escape from a changed Create, Fork, or Edit taxonomy draft also confirms before
discarding it. A blank Create or unchanged copied Fork returns immediately.
Discard alerts share the Commit dialog's two yellow mouse-clickable buttons.
Use `y` or click `y: discard` to discard, and Enter, Esc, `n`, or the
`enter/esc/n: keep editing` button to resume with the draft and cursor intact.
Clicks outside the alert and other input are ignored while it is open.
Library breadcrumb alerts discard the entire taxonomy draft; Type and Property
alerts discard only that child. Button targets refresh after terminal resize.

Type Details has ruled Description and Properties headings, followed by a
two-column properties table with left-aligned values. Left and Right change
focus between the two lists; Up and Down select within the focused list. Page Up
and Page Down scroll Taxonomy Description when Taxonomies has focus and Type
Details when Types has focus. The focused list's border and title are yellow;
its selected row is yellow while the other list retains a subdued selection.
The Project and Library command bars show `m: menu`, which cycles between them.
The `/` key opens commands; clicking a tab also switches views. Command bar
hints use spaces rather than pipe separators. Library commands use lowercase
labels: `create taxonomy`, `edit taxonomy`, `fork taxonomy`, `delete taxonomy`,
`review changes`, `switch to project`, `help`, and `quit`. Shortcuts align at the
right edge, including `m` beside the switch command.
Project has its own command list. A bordered Search field shows the `/` prompt
and input cursor above the results. Arrow keys select a filtered command and
Enter runs it; Esc closes the popup. Existing direct shortcuts such as `n`,
`e`, `f`, `d`, and `ctrl+s` still work. Built-ins may be forked; custom taxonomies
may also be edited or deleted. Fork lineage is informational and never updates.
The contextual Help popup aligns actions on the left and keys on the right;
Esc closes it. Both popups show their bottom hints as `[key] action`.

Changes remain in memory until `ctrl+s` opens semantic review and `enter`
applies them. Global saves use compare-and-swap. Project saves atomically replace
authored overrides and the complete portable lock. Failed saves retain the
draft. Browsing Library preserves a dirty Project draft; a Library edit requires
an explicit apply or discard decision before it proceeds. Errors and action
notices appear briefly over the content; source-load errors remain visible while
unavailable. Existing user-level default and availability settings remain the
read-only inheritance baseline in Project.

Page Up and Page Down scroll long forms and the focused Description; Review
retains its Up and Down scrolling. Lists keep their selected row in view as
content grows.

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
