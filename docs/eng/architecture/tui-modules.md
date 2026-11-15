# TUI module ownership

This map records how `gitserious-tui` divides its modules at the end of the root refactor. Use it to find which module owns a key, a command, a dialog, or a drawing rule before changing one.

## Direction of execution

`gitserious-tui` is an inbound adapter. The executable builds the application services and passes them in. The adapter runs the terminal session, the root coordinates pages, and each page calls its own application port.

```text
gitserious (composition root)
    |
    v
RatatuiInteractiveUi            adapter.rs
    |
    v
Root                            root/
    |
    +--- HomePage   ---> ProjectWorkspace
    +--- CommitView ---> CommitWorkspace
    +--- ConfigView ---> ConfigurationWorkspace
    +--- HelpView
```

Application use cases stay in `gitserious-app`. No TUI module builds a workspace.

## Module tree

```text
crates/gitserious-tui/src/
├── lib.rs
├── adapter.rs
├── root/
│   ├── mod.rs
│   ├── navigation.rs
│   ├── input.rs
│   ├── commands.rs
│   ├── overlay.rs
│   ├── render.rs
│   └── tests/
├── home/
├── commit/
│   └── author/
│       ├── state.rs
│       ├── render.rs
│       └── cards/
│           ├── mod.rs
│           ├── layout.rs
│           └── render.rs
├── config/
├── help/
├── confirmation.rs
├── runtime.rs
└── theme.rs
```

`lib.rs` exports `RatatuiInteractiveUi` from `adapter`, `HelpPage` from `help`, and `Capabilities` and `InitialPage` from `root`.

## Ownership

| Module | Owns | Does not own |
|---|---|---|
| [`adapter.rs`](../../../crates/gitserious-tui/src/adapter.rs) | `RatatuiInteractiveUi`, the interactive-terminal check, and the event loop. It sees only `SessionRequest`: submit or quit. | Application services, which the executable builds. Root's actions and page state. |
| [`root/mod.rs`](../../../crates/gitserious-tui/src/root/mod.rs) | `Root`'s state, `Capabilities`, `InitialPage`, page entry, and `execute`, which carries out every root action and page request | Page workflows |
| [`root/navigation.rs`](../../../crates/gitserious-tui/src/root/navigation.rs) | `Section`, page order and wraparound, the tab strip, and which tab a click hits | Entering a page |
| [`root/input.rs`](../../../crates/gitserious-tui/src/root/input.rs) | Event precedence: key releases, root overlays, page modals, the too-small terminal, tab clicks, the root's keys, then the active page. Commit's open editor takes every key except tab and shift+tab before the root's keys. | Any page's keys |
| [`root/commands.rs`](../../../crates/gitserious-tui/src/root/commands.rs) | The palette's command list, search, selection, and drawing | Which page command is available, which each page decides |
| [`root/overlay.rs`](../../../crates/gitserious-tui/src/root/overlay.rs) | The palette overlay, the quit confirmation, and the popup frame | Page modals |
| [`root/render.rs`](../../../crates/gitserious-tui/src/root/render.rs) | The frame: background, too-small message, header band and version, page body and footer (with the EDIT badge while Commit's editor is open), and overlays on top | Any page's drawing |
| [`home/`](../../../crates/gitserious-tui/src/home/mod.rs) | The project overview, the initialization editor, which keys the editor captures, Enter's step, and Home's palette command | Reloading Config |
| [`commit/`](../../../crates/gitserious-tui/src/commit/mod.rs) | The status tree, its keys and hints, the Git result and staged views, palette commands, the in-page editor and its staged header, and failed-review retention | Suspending the terminal for submission, which the adapter starts |
| `commit/author/state.rs` | The authoring stages, taxonomy and type picker, changing type with ctrl+t, review, discard confirmation, and the cite list's keys | Card text layout and draft field parsing |
| `commit/author/cite.rs` | The cite list opened by `@`, its query, its kind, its selection, and its drawing | Reading the worktree. The Commit page supplies the paths and commits |
| `commit/author/cards/` | The fixed subject type and editable tail, property cards and their applicability, per-card text histories, validation notes, and draft construction | Taxonomy selection and Git submission |
| `commit/author/cards/layout.rs` | Wrapped rows, vertical cursor movement, and message columns | Terminal drawing |
| `commit/author/cards/render.rs` | Card marks, policy words, guidance, counters, validation notes, and scrolling | Draft validation rules |
| [`config/`](../../../crates/gitserious-tui/src/config/mod.rs) | Library editing, the read-only Project view, its own palette, its modals, and which keys it gives the root (`RootKeys`) | Opening Home's editor |
| [`help/`](../../../crates/gitserious-tui/src/help/mod.rs) | Help pages, the sidebar, scrolling, keys, and clicks | Which page opens first |
| [`confirmation.rs`](../../../crates/gitserious-tui/src/confirmation.rs) | The confirmation dialog that Config and the root's quit prompt share | When to ask |
| [`runtime.rs`](../../../crates/gitserious-tui/src/runtime.rs) | Terminal setup, restoration, and suspension | The event loop |
| [`theme.rs`](../../../crates/gitserious-tui/src/theme.rs) | Colors, minimum size, footer layout, and the EDIT badge for every page | Theme preferences |

## Page requests

Pages answer events with their own request types. There is no shared page trait, because their event shapes differ.

- Home returns `HomeRequest`: go to Commit, open the initialization editor, or report a new project. The root wraps it as `Action::Home`.
- Commit returns `CommitRequest`: refresh, show the Git result, author, resume, or submit. Commit runs every request but submit itself, so the editor opens inside the page. It hands submit back, because Git hooks and signing may need the terminal. The root wraps the request as `Action::Commit`.
- Config returns `ConfigRequest`: quit or initialize.
- Help handles its keys and clicks and returns nothing.

## Cross-page reactions

The root owns three reactions that span pages.

- Config asks to initialize. The root refreshes Home. If Home can still initialize, the root opens Home's editor. Otherwise it reloads Config.
- Home reports a new project. The root reloads Config, which loaded the project before it existed.
- Quit with a pending Commit review or an unsaved Commit draft opens the root's quit confirmation.

## Visibility

`root/` reads no field of any page and matches no page key code. It calls page methods and maps page requests. `CommitView`'s `snapshot`, `blocker`, and `notice` are private, and root tests read them through test-only accessors. `HomeAction` stays crate-visible for one root test, and no production code outside `home/` names it.

## Theme preferences

`theme.rs` stays at the crate root, because every page draws with it. Theme preferences would belong to Config, kept separate from the drawing rules in `theme.rs`. This refactor does not decide how.
