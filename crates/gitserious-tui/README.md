# gitserious-tui

This crate is an internal component of `gitserious`.

The Rust API exposed here is unstable and may have breaking changes in any
release. The supported public interface is the `gitserious` command-line tool.

This crate implements terminal interaction ports with Ratatui. It owns terminal
setup, rendering, event handling, and restoration; commit policy and workflow
coordination remain in `gitserious-app`.

## Commit authoring

`gitserious commit` runs one structured terminal session:

1. select an effective commit type;
2. author scope, description, and schema-defined property values;
3. review the exact canonical message and confirm before Git creates the
   staged-index commit.

Each view starts with a borderless one-line header: `Select commit type`,
`Compose commit message`, or `Review and commit` on the left, with `Step 1/3`,
`Step 2/3`, or `Step 3/3` on the right. The composer keeps `Type: <type>`
directly below its header. A preselected commit type starts directly at
`Step 2/3`.

Every view is painted on a true `#000000` background. The picker, composer, and
review use dark-gray Unicode frames that consume the leftover area between the
header chrome and the navigation strip, with no extra outer spacer row or
column. Framed content retains one blank column on its left and right and
occupies the row immediately inside horizontal borders; this avoids the
visually taller inset produced by a full blank terminal row. Headers and
navigation strips remain outside the frames. The too-small fallback remains
borderless.

`gitserious commit --type <COMMIT TYPE>` preselects the type and starts in the
composer. Both forms require interactive standard input and output. There is no
configured-Git-editor fallback or non-interactive authoring mode in this slice.

Step 1 lists the effective catalog as a table inside that shared frame. A
`CONVENTIONAL` tab pane sits above a fused heading rule, then the type table: a
one-cell `›` marker on the current type, type identifiers that hug the longest
id, and descriptions in the remaining width. A two-cell gap separates the
columns, matching Message Properties. Rows alternate between `#000000` and
`#101010`; the current type uses a full black-on-yellow row. The selected
catalog tab uses the same black-on-yellow chip; `tab` cycles available type
sets and a click on a chip selects that set. The tab is currently fixed to `CONVENTIONAL`, so `tab` and a click on it
keep the current table. Type ids, ordering, and descriptions come from the
active project template, including domain and custom templates. A template-aware
label and per-commit template switching belong to the later TUI checkpoint.

The composer presents one prepopulated document as three immutable structural
sections. `Message Subject` contains optional `scope` and required `description`
fields; `Message Body` contains the selected type's schema-defined properties;
and `Message Footer` contains a global optional multiline `breaking-change`
field. Section and field headings are immutable. The form hides the
scaffold's internal heading colons and displays each field as a bold neutral
label followed by a dark-gray rule. Values are authored beneath field headings
without traversing separate controls. The cursor starts in the scope value and
skips structural headings and reserved separators. Scope and description accept
one authored line; schema properties remain multiline. Blank nonrequired fields
are omitted, while blank required fields block review. The core domain retains
the Conventional Commit subject primitive internally; `description` is the
adapter's user-facing vocabulary.

Step 2 places `Message Properties`, `Property Description`, the editor, and its
validation row inside one dark-gray Unicode frame. The header and type metadata
remain above that frame, and the navigation strip remains below it. Nested
panes retain a one-column content inset. At up to 100 terminal columns, the
properties and description remain side by side above the full-width editor.
At 101 columns and wider, the properties table hugs its content in the
upper-left pane, the description fills the lower-left pane, and the editor
occupies the right pane.
Both wide left-pane headings retain a horizontal rule beneath them. The
validation and column-status row stays fixed across the complete frame bottom
in both layouts. Resizing across the breakpoint does not alter the document,
cursor, selection, history, or viewport.

Nested layouts give the headings, HUD, description, editor, and
validation/status row independent content rectangles while one composite chrome
pass keeps their shared borders single-width. The properties pane measures the
complete marker, field-name, and requirement columns from the selected
definition and takes only the width they require. The requirement column hugs
the longest label in that definition (`recommended` / `conditional` stay
eleven cells; `style` and `revert` shrink to `required` / `optional`).
Property names never clip. A definition whose complete table cannot fit uses
a definition-specific too-small fallback. The HUD tracks
complete, incomplete, and invalid values and leaves a two-cell gap between its
columns. Rows alternate between `#000000` and `#101010`; the current property
uses a full black-on-yellow row while its status marker retains its semantic
color. Property guidance comes directly from the selected core definition. A
conditional property shows its description followed by the catalog's separate
`Required when...` rationale while remaining nonblocking. The description pane
does not repeat the selected field name or requirement already shown in Message
Properties. Scope and description guidance illustrates `type(scope):
description`, including the scope-free `type: description` form. The composer
requires at least 22 terminal rows so its context, editor, validation row, and
separators remain usable.

Step 3 places `Final Git Commit Message` in a heading pane with the same
under-heading rule as the Step 2 context headings. The canonical message
occupies the remaining framed area.

The `Compose commit message` view always edits an 80-column virtual surface. If
the framed editor is narrower, it follows the cursor horizontally instead of
wrapping early, then returns to column 1 when a word or glyph soft-wraps at
column 80. An always-visible scrollbar occupies the editor region's right edge,
immediately inside the outer frame, with a dark-gray `│` track and solid yellow
`█` thumb. Text and field rules retain their one-column right content inset
before that edge-fixed scrollbar. The thumb fills the track when all content
fits. When content overflows, deterministic viewport geometry maps the first
and final meaningful document rows to the exact ends of the track; the final
reserved scaffold separator does not extend the scroll range. The track covers
only the editable viewport and excludes validation errors and column status.
The single-line rules introducing `Message Body` and `Message Footer` also stop
before the content inset and scrollbar. All rules are render-only chrome and
never enter the authored document or Git message. `Message Subject`, `Message
Body`, and `Message Footer` remain the only yellow editor headings.

Backspace and Delete operate only inside the active semantic field. They may
join explicit lines within a multiline value, but an edit that would cross a
field heading or reserved separator is rejected without changing text, cursor,
selection, or undo history. This includes the final `breaking-change` field.

The fixed validation row shows red errors on the left and the right-aligned
`col N/80` status on the right, clipping long errors before the status. The
yellow navigation strip contains only its pipe-delimited key hints. `ctrl+s`
compiles the form into the exact canonical message shown during review.
Compilation removes trailing whitespace from every encoded line without
changing the editor document or intentional leading whitespace. Property
values are rendered beneath their headings without automatic indentation.
Canonical rendering wraps property and breaking-change prose at the same
80-column Unicode display-width boundary used by the editor, without mutating
the authored document. Step 3 displays those encoded line breaks exactly, even
when its viewport is wider than 80 columns. Internal scope whitespace converts
to hyphens. A populated `breaking-change` value also adds `!` before the header
colon and renders an uppercase `BREAKING CHANGE:` footer; a blank value adds
neither.

| Context | Controls |
| --- | --- |
| Type picker | `tab` switches type set; `↑`/`↓` moves; `enter` selects; `esc`/`q` cancels |
| Composer | Conventional document editing; `↑`/`↓` moves within and between fields; `esc` goes back; `ctrl+s` validates and reviews |
| Review | `enter` confirms; `esc` returns to editing; arrows or page up/page down scroll when the message exceeds the viewport; `q` cancels |
| Cancellation | Untouched drafts cancel immediately; dirty drafts require explicit keyboard or mouse confirmation |

The composer uses conventional cursor movement, selection, word movement,
soft-wrapped Unicode input, paste, undo/redo, and `ctrl+k` deletion to the end
of the line through the text-area widget. `up` and `down` move by visual line
inside multiline properties, then cross into adjacent fields without landing
on immutable headers or reserved separators. Left/right movement, including
word movement and selection, remains inside the current field. `enter` advances
from scope or description and skips a blank property; inside a populated
property it inserts a newline normally. The final blank property does not cycle
back to scope.

Explicit property lines join normally with Backspace or Delete. Each field has
one empty value row and one reserved separator row; description and the final
body property include an additional structural row consumed by their following
section divider. The cursor uses a visible cell without underlining the input
line. A blank cursor at terminal column zero uses a one-cell foreground block
so terminal background bleed cannot make it appear wider. Navigation hints at
the bottom of each terminal view use a highlighted strip, bold `key: action`
pairs, ` | ` separators, and lowercase key names for quick scanning.
Dirty-draft confirmation centers its bold heading, question, and controls in a
double-line `Discard Message` alert. Its `y: discard` and
`enter/esc/n: keep editing` controls are distinct black-on-yellow buttons that
accept both mouse clicks and their existing keyboard inputs. Mouse capture is
disabled on every terminal exit path.
