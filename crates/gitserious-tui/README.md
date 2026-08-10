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

The composer prepopulates one editable document with `scope`, `subject`, and
every schema-defined property header. Values are authored beneath those headers
without traversing separate controls. The Fields pane is a passive HUD that
tracks complete, incomplete, and invalid values while showing the schema
requirement and the field under the document cursor. `Ctrl+S` compiles this form
into the exact canonical message shown during review; blank nonrequired fields
are omitted. Repeatable properties keep independent ordered sections.

| Context | Controls |
| --- | --- |
| Type picker | `Up`/`k`, `Down`/`j`, `Home`, `End`, `Enter`; `Esc`/`q` cancels |
| Composer | Normal document editing; `Ctrl+N`/`Ctrl+D` adds/removes a repeatable section under the cursor; `Ctrl+S` validates and reviews |
| Keymap | `Ctrl+T` toggles conventional and Vim editing; the active mode is always shown |
| Review | `Enter` confirms; `Esc` returns to editing; arrows or `j`/`k` scroll; `q` cancels |
| Cancellation | Untouched drafts cancel immediately; dirty drafts require explicit discard confirmation |

Conventional mode provides cursor movement, selection, word movement,
soft-wrapped Unicode input, paste, undo/redo, and `Ctrl+K` deletion to the end of
the line through the text-area widget. The navigation hints at the bottom of
each terminal view use a highlighted strip for quick scanning.
The bounded Vim mode provides Normal and Insert modes plus `h`/`j`/`k`/`l`,
`w`/`b`, `0`/`$`, `i`/`a`, `x`, `u`, and `Ctrl+R`. It intentionally does not
implement operators, counts, registers, macros, Visual mode, or ex commands.
