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
2. author scope, subject, and schema-defined property values;
3. review the exact canonical message and confirm before Git creates the
   staged-index commit.

The upper-right corner labels these views `Step 1/3`, `Step 2/3`, and
`Step 3/3`. A preselected commit type starts directly at `Step 2/3`.

`gitserious commit --type <COMMIT TYPE>` preselects the type and starts in the
composer. Both forms require interactive standard input and output. There is no
configured-Git-editor fallback or non-interactive authoring mode in this slice.

The composer places one prepopulated document in a full-width Edit form with
`scope`, `subject`, and every schema-defined property header. The headers are
immutable; values are authored beneath them without traversing separate
controls. The cursor starts in the scope value and skips both field headers and
their reserved blank separators when moving between editable regions. Scope and
subject accept one authored line; schema properties remain multiline. Blank
nonrequired fields are omitted, while blank required fields block review.

The passive Fields HUD and contextual Description pane sit side by side above
the Edit form. The HUD tracks complete, incomplete, and invalid values and
aligns field names and schema requirements in separate responsive columns.
Long names clip within the field column instead of colliding with requirement
labels or the pane border. Scope and subject guidance illustrates the
Conventional Commit header `type(scope): subject`, including the scope-free
`type: subject` form.

The `Compose commit message` view always edits an 80-column virtual surface. If
the visible Edit form is narrower, it follows the cursor horizontally instead
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
subject and skips a blank property; inside a populated property it inserts a
newline normally. The final blank property does not cycle back to scope.

Explicit property lines join normally with Backspace or Delete, while a
reserved blank row always separates authored text—including soft wraps—from the
next immutable heading. The cursor uses a visible cell without underlining the
input line. Navigation hints at the bottom of each terminal view use a
highlighted strip, bold `key: action` pairs, centered-dot separators, and
lowercase `ctrl` labels for quick scanning. The validation row above the strip
stays empty until an attempted review reports an error. Dirty-draft confirmation
centers its question and controls within the discard popup.
