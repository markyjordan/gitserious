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

Every view is painted on a true `#000000` background. The picker and composer
use dark-gray Unicode frames inset one cell from each terminal edge. Their
content starts one cell inside each border, matching the outer gutter. The
picker frame begins directly below its header; the composer frame begins
directly below `Type: <type>`. Review, discard confirmation, and the too-small
fallback remain borderless. Discard confirmation is a centered five-line group;
the too-small fallback uses the same heading-and-content treatment.

`gitserious commit --type <COMMIT TYPE>` preselects the type and starts in the
composer. Both forms require interactive standard input and output. There is no
configured-Git-editor fallback or non-interactive authoring mode in this slice.

The composer presents one prepopulated document as three immutable structural
sections. `Message Subject` contains optional `scope` and required `description`
fields; `Message Body` contains the selected type's schema-defined properties;
and `Message Footer` reserves a distinct boundary for future footer-specific
properties. Section and field headings are immutable. The form hides the
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
remain above that frame, and the navigation strip remains below it. The passive
properties HUD and contextual description use a 40/60 split with one vertical
divider. The HUD tracks complete, incomplete, and invalid values. Its field-name
column hugs the widest visible name and leaves a two-cell gap between internal
table columns so requirement labels remain distinct, while long names clip
before those labels. Rows alternate between `#000000` and `#101010`; the current
property uses a full black-on-yellow row while its status marker retains its
semantic color. The shared context area takes the height of whichever side has
more real content: the HUD rows or the wrapped description. Scope and
description guidance illustrates `type(scope): description`, including the
scope-free `type: description` form. The composer requires at least 21 terminal
rows so the context, three-row minimum editor, validation row, and separators
remain usable.

The `Compose commit message` view always edits an 80-column virtual surface. If
the framed editor is narrower, it follows the cursor horizontally instead of
wrapping early, then returns to column 1 when a word or glyph soft-wraps at
column 80. The rightmost inner editor column is reserved for an always-visible
scrollbar with a dark-gray `│` track and yellow `┃` thumb. Its thumb fills the
track when all content fits and follows the fixed-width soft-wrapped document
when it overflows. Field rules stop before that reserved column. Full-width
rules introduce `Message Body` and `Message Footer`. These rules are render-only
layout chrome, use `─` (`U+2500`) rather than em dashes, and never enter the
authored document or Git message. `Message Subject`, `Message Body`, and
`Message Footer` remain the only yellow editor headings.

The fixed validation row shows red errors on the left and the right-aligned
`col N/80` status on the right, clipping long errors before the status. The
yellow navigation strip contains only its pipe-delimited key hints. `ctrl+s`
compiles the form into the exact canonical message shown during review.
Compilation removes trailing whitespace from every encoded line without
changing the editor document or intentional leading whitespace. Property
values are rendered beneath their headings without automatic indentation.

| Context | Controls |
| --- | --- |
| Type picker | `↑`/`↓` moves; `enter` selects; `esc`/`q` cancels |
| Composer | Conventional document editing; `↑`/`↓` moves within and between fields; `esc` goes back; `ctrl+s` validates and reviews |
| Review | `enter` confirms; `esc` returns to editing; arrows or page up/page down scroll when the message exceeds the viewport; `q` cancels |
| Cancellation | Untouched drafts cancel immediately; dirty drafts require explicit discard confirmation |

The composer uses conventional cursor movement, selection, word movement,
soft-wrapped Unicode input, paste, undo/redo, and `ctrl+k` deletion to the end
of the line through the text-area widget. `up` and `down` move by visual line
inside multiline properties, then cross into adjacent fields without landing
on immutable headers or reserved separators. `enter` advances from scope or
description and skips a blank property; inside a populated property it inserts a
newline normally. The final blank property does not cycle back to scope.

Explicit property lines join normally with Backspace or Delete. Each field has
one empty value row and one reserved separator row; description and the final
body property include an additional structural row consumed by their following
section divider. The cursor uses a visible cell without underlining the input
line. A blank cursor at terminal column zero uses a one-cell foreground block
so terminal background bleed cannot make it appear wider. Navigation hints at
the bottom of each terminal view use a highlighted strip, bold `key: action`
pairs, ` | ` separators, and lowercase key names for quick scanning.
Dirty-draft confirmation centers its bold heading, question, and controls as a
borderless group.
