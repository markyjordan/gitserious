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
3. review the exact canonical message; and
4. confirm before Git creates the staged-index commit.

`gitserious commit --type <COMMIT TYPE>` preselects the type and starts in the
composer. Both forms require interactive standard input and output. There is no
configured-Git-editor fallback or non-interactive authoring mode in this slice.

The composer places one prepopulated document on the left with `scope`,
`subject`, and every schema-defined property header. The headers are immutable;
values are authored beneath them without traversing separate controls. Blank
nonrequired fields are omitted, while blank required fields block review.

The right sidebar places the passive Fields HUD above the contextual Description
pane. The HUD tracks complete, incomplete, and invalid values and shows each
field's schema requirement. Scope and subject guidance illustrates the
Conventional Commit header `type(scope): subject`, including the scope-free
`type: subject` form.

The `Compose commit message` view always edits an 80-column virtual surface. If
the visible Edit form is narrower, it follows the cursor horizontally instead
of wrapping early, then returns to column 1 when a word or glyph soft-wraps at
column 80. Its highlighted status strip reports `col N/80`, separated from the
key hints by `▌`. `Ctrl+S` compiles the form into the exact canonical message
shown during review. Property values are rendered beneath their headings
without automatic indentation.

| Context | Controls |
| --- | --- |
| Type picker | `Up`/`k`, `Down`/`j`, `Home`, `End`, `Enter`; `Esc`/`q` cancels |
| Composer | Normal document editing; `Ctrl+S` validates and reviews |
| Keymap | `Ctrl+T` toggles conventional and Vim editing; the active mode is always shown |
| Review | `Enter` confirms; `Esc` returns to editing; arrows or `j`/`k` scroll; `q` cancels |
| Cancellation | Untouched drafts cancel immediately; dirty drafts require explicit discard confirmation |

Conventional mode provides cursor movement, selection, word movement,
soft-wrapped Unicode input, paste, undo/redo, and `Ctrl+K` deletion to the end of
the line through the text-area widget. The cursor uses a visible cell without
underlining the input line. Navigation hints at the bottom of each terminal view
use a highlighted strip, centered-dot separators, and lowercase `ctrl` labels
for quick scanning.
The bounded Vim mode provides Normal and Insert modes plus `h`/`j`/`k`/`l`,
`w`/`b`, `0`/`$`, `i`/`a`, `x`, `u`, and `Ctrl+R`. It intentionally does not
implement operators, counts, registers, macros, Visual mode, or ex commands.
