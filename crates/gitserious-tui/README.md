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

The views use a borderless, spacing-led hierarchy. Bold yellow section headings
sit directly above unpadded content, and exactly one blank row separates major
groups. The type list, message form, reviewed commit message, validation status,
and navigation strip therefore use the terminal width from column zero without
rectangular pane chrome. Discard confirmation is a centered five-line group;
the too-small fallback uses the same borderless heading-and-content treatment.

`gitserious commit --type <COMMIT TYPE>` preselects the type and starts in the
composer. Both forms require interactive standard input and output. There is no
configured-Git-editor fallback or non-interactive authoring mode in this slice.

The composer presents one prepopulated document as three immutable structural
sections. `Message Subject` contains optional `scope` and required `description`
fields; `Message Body` contains the selected type's schema-defined properties;
and `Message Footer` reserves a distinct boundary for future footer-specific
properties. Section and field headings are immutable. Values are authored
beneath field headings without traversing separate controls. The cursor starts
in the scope value and skips structural headings and reserved separators. Scope
and description accept one authored line; schema properties remain multiline.
Blank nonrequired fields are omitted, while blank required fields block review.
The core domain retains the Conventional Commit subject primitive internally;
`description` is the adapter's user-facing vocabulary.

The passive `Fields` HUD and contextual `Field guidance` section sit side by
side above the message form at a 40/60 split. The HUD tracks complete,
incomplete, and invalid values. Its field-name column hugs the widest visible
name and leaves a two-cell gap between each internal table column so requirement
labels remain distinct, while long names clip before those labels. The shared
context group takes the height of whichever side has more real content: the HUD
rows or the wrapped guidance. Scope and description guidance illustrates
`type(scope): description`, including the scope-free `type: description` form.
Neither section adds outer padding.

The `Compose commit message` view always edits an 80-column virtual surface. If
the visible message form is narrower, it follows the cursor horizontally instead
of wrapping early, then returns to column 1 when a word or glyph soft-wraps at
column 80. The editor renders directly from column zero and uses the full visible
width up to that limit. Its highlighted status strip reports `col N/80`,
separated from the key hints by `▌`. `Ctrl+S` compiles the form into the exact
canonical message shown during review. Compilation removes trailing whitespace
from every encoded line without changing the editor document or intentional
leading whitespace. Property values are rendered beneath their headings without
automatic indentation.

| Context | Controls |
| --- | --- |
| Type picker | `Up`/`Down` moves; `Enter` selects; `Esc`/`q` cancels |
| Composer | Conventional document editing; `Up`/`Down` moves within and between fields; `Esc` goes back; `Ctrl+S` validates and reviews |
| Review | `Enter` confirms; `Esc` returns to editing; arrows or Page Up/Page Down scroll when the message exceeds the viewport; `q` cancels |
| Cancellation | Untouched drafts cancel immediately; dirty drafts require explicit discard confirmation |

The composer uses conventional cursor movement, selection, word movement,
soft-wrapped Unicode input, paste, undo/redo, and `Ctrl+K` deletion to the end
of the line through the text-area widget. `Up` and `Down` move by visual line
inside multiline properties, then cross into adjacent fields without landing
on immutable headers or reserved separators. `Enter` advances from scope or
description and skips a blank property; inside a populated property it inserts a
newline normally. The final blank property does not cycle back to scope.

Explicit property lines join normally with Backspace or Delete, while a
reserved blank row always separates authored text—including soft wraps—from the
next immutable heading. The cursor uses a visible cell without underlining the
input line. A blank cursor at terminal column zero uses a one-cell foreground
block so terminal background bleed cannot make it appear wider. Navigation
hints at the bottom of each terminal view use a highlighted strip, bold
`key: action` pairs, centered-dot separators, and lowercase `ctrl` labels for
quick scanning. The validation row above the strip stays empty until an
attempted review reports an error. Dirty-draft confirmation centers its bold
heading, question, and controls as a borderless group.
