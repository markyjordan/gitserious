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

The passive Fields HUD and contextual Field guidance pane sit side by side above
the message form. The HUD tracks complete, incomplete, and invalid values. Its
field-name column hugs the widest visible name so requirement labels remain
near their values, while long names clip before labels or pane borders. Scope
and description guidance illustrates `type(scope): description`, including the
scope-free `type: description` form. Every bordered pane uses equal one-cell
padding on all four sides.

The `Compose commit message` view always edits an 80-column virtual surface. If
the visible Message form is narrower, it follows the cursor horizontally instead
of wrapping early, then returns to column 1 when a word or glyph soft-wraps at
column 80. Its highlighted status strip reports `col N/80`, separated from the
key hints by `▌`. `Ctrl+S` compiles the form into the exact canonical message
shown during review. Compilation removes trailing whitespace from every encoded
line without changing the editor document or intentional leading whitespace.
Property values are rendered beneath their headings without automatic
indentation.

| Context | Controls |
| --- | --- |
| Type picker | `Up`/`Down` moves; `Enter` selects; `Esc`/`q` cancels |
| Composer | Conventional document editing; `Up`/`Down` moves within and between fields; `Esc` goes back; `Ctrl+S` validates and reviews |
| Review | `Enter` confirms; `Esc` returns to editing; arrows or Page Up/Page Down scroll; `q` cancels |
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
input line. Navigation hints at the bottom of each terminal view use a
highlighted strip, bold `key: action` pairs, centered-dot separators, and
lowercase `ctrl` labels for quick scanning. The validation row above the strip
stays empty until an attempted review reports an error. Dirty-draft confirmation
centers its question and controls within the discard popup.
