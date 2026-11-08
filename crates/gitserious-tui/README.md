# gitserious-tui

## Unified interactive shell

Bare `gitserious` opens Home. The nav bar is the top row: Home, Commit,
Config, and Help on the left, with the active tab underlined, and the package
version on the right. Home shows the static GITSERIOUS wordmark and its
tagline. The footer is the bottom row, with contextual `key action` hints on
the left and `tab nav bar  / commands  q quit` on the right; popups show
`[key] action`. Crowded footers drop arrow hints first. Bare invocation
without terminal streams prints help. Home, Library, and Help work outside a
repository.

Under the tagline, Home names the directory or repository and offers the one
step its state allows:

- Outside a Git repository, it shows the directory, `no git repository
  found`, and `initialize git and gitserious`. Enter opens the same editor as
  below; Initialize runs `git init`, which honors `init.defaultBranch`, and
  then writes the project. If `git init` fails, nothing is written.
- In a repository without `gitserious.toml`, Enter on `initialize gitserious`
  opens `initialize <name>`, described below.
- In an initialized repository, it shows `✓ <name> is initialized with
  <default>`, warns while `gitserious.toml` is untracked, and Enter on
  `Go to Commit` opens Commit.
- When Git or `gitserious.toml` cannot be read, it shows the error.

Home rereads the repository each time you return to it.

`initialize <name>` lists the taxonomies the new `gitserious.toml` will hold.
Rows show the taxonomy name with its type count and version aligned at the right
(`11 types  v1`), matching Library and Project. The built-ins start included,
`conventional` first; every Library
taxonomy with at least one type follows, excluded and tagged `custom`. If
the Library cannot be read, the editor names the error and still offers the
built-ins. The first included
taxonomy is the default, the one a commit considers first. Space includes or
excludes the selected taxonomy, though the last included one stays. Enter
makes it the default by moving it to the top. Alt+Up and Alt+Down reorder. A long list scrolls with the selection and keeps
the initialize row in view.
The preview shows the selected taxonomy's types and their properties, beside
the list when the terminal is at least two and a half times as wide as it is
tall and each pane gets 44 columns, and under the list otherwise.

The last row reads `initialize git and gitserious` in a directory without Git
and `initialize gitserious` in a repository. With it selected, the preview
pane reads `gitserious will write the following:` and lists `.git/` when Git
is initialized too, then `.gitserious/`, and `gitserious.toml` with its
default taxonomy. Enter writes them; Esc returns to Home and writes nothing. The editor stays open across visits to other sections.

Screens draw with the active appearance theme, a set of semantic colors
(ground, band, text, dim, cursor, pending, done, blocks). Bone is the first
and, for now, only theme.

Tab opens the next section and Shift+Tab the previous one, wrapping from Help
to Home and back. Clicking a tab opens it.

`gitserious config` opens the shell inside Config. The nav bar stays visible
above the configuration Menu and its Project and Library widgets, described
under Configuration. From the Project or Library browser, Tab and Shift+Tab
switch sections, Esc returns to Home, and `q` quits. Every other key, including `/` and `?`, reaches the
configuration interface. Config selections and unapplied changes survive
section visits. Library EDIT keeps focus inside Config until Esc validates and
saves the item. Tab, root-row clicks, and quit cannot leave EDIT. Standalone Project edits
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

Help lists its Overview, Init, Config, and Commit pages in a numbered sidebar
beside the chosen page. `gitserious --help` opens the shell at Help on the
Overview, and `gitserious <command> --help` or `gitserious help <command>` on
that command's page, or on the Overview when the command has none. The
Overview shows what `gitserious -h` prints. Each command page opens with `gitserious help: <command>` and shows what
`gitserious help <command>` prints. Pages wrap to the pane. Headings and command and flag names are bold;
descriptions are dim. Help opens in the sidebar on the page chosen last. There,
Up/Down or a page's number chooses the page, and clicking an entry selects it.
Right or Enter moves into the page to read it: Up/Down scroll by a line, Page
Up/Page Down by a screen, and Left returns to the sidebar.

## Configuration

`gitserious config` opens Project and Library beside a Menu. Up and Down choose
a Menu entry. Left and Right move focus between Menu and content. Clicking a
Menu entry switches to it while browsing.

In a Git repository without `gitserious.toml`, Project names the repository and
its path, marks the missing file in coral, and offers **initialize gitserious**.
Enter opens the same taxonomy selection editor as Home. Esc cancels without
writing; the editor's Initialize row writes the chosen taxonomies. The footer
shows `enter initialize` and `←/→ focus`.

An initialized Project opens directly on the taxonomies stored in
`gitserious.toml`, with the same outline, Inspector, responsive layout, policy
colors, and property spacing as Library. Taxonomy names stay separate from
`x types  vX` metadata. The configured default is labeled `default` in the
ownership column, independent of the selected row or the order in the file.
Other installed taxonomies leave that label blank.
The repository name and path appear
above the outline, with a sage file badge. Enter expands or collapses a taxonomy
or type; Up and Down choose a row, and Page Up and Page Down scroll its Inspector.
Left returns to the Menu. Project is read-only: there are no add rows, edit,
fork, delete, import, export, reorder, or save commands. Its preview does not
read definitions from Library and works when Library is unavailable.

The standalone `ConfigurationEditor` adapter retains its staged Project editing
and review API for embedders. Its pending drafts still require Apply or Discard
before leaving. The unified shell uses the read-only Project overview.

Config uses the shared Bone palette: ground behind content, band behind
raised rows, text for values, dim for context, pending orange for EDIT, and
blocks coral for validation. The Library header reads "Commit message taxonomy
library" above its configuration file path. Library has a taxonomy outline and
an Inspector. Custom taxonomy details show Created on and Modified on immediately
above Version. Dates use UTC calendar days in `YYYY-MM-DD` format. Custom
entries without recorded dates show `Unknown`; built-ins omit both facts.
Saving a new entry
records both dates; later content edits preserve Created on and update
Modified on. Browsing and no-op edits keep the recorded dates unchanged.
The new-taxonomy form omits these facts until saved. Project policy does not
carry Library dates.

The Inspector sits beside the
outline when the terminal has at least 100 columns and is at least three times
as wide as it is tall. Otherwise it sits below the outline, including at 87x40
and 125x51. Enter
opens or closes a node, Up and Down select rows, and Page Up and Page Down page
through Inspector details. A pane with more rows than fit counts the rest in
its last row. A type's browsing Inspector lists properties in a table with Property, Policy,
and Description columns. Long values wrap inside their column. Type rows in the
outline show only the type name; expanding a type reveals its property rows.
Valid taxonomy rows align their type count before their version (`11 types  v1`);
narrow rows drop the count before the version.
Built-in taxonomies show their origin in the outline and read-only status in
the Inspector. `f` forks a taxonomy, `n` creates one, `e` opens EDIT, and `d`
asks for deletion confirmation. Expanded custom taxonomies and types include
add rows for types and properties. A custom property's `c` opens a picker for
copying it to other types in the same taxonomy. Alt+Up and Alt+Down reorder a
type or property. Browsing footers lead with `←/→ focus`, `↑/↓ move`, then
the selected row’s Enter action and other actions. The `/` palette lists the
available Library actions.

Config pop-ups use the Bone ground and text, a double orange border, and
plain orange key hints with dim labels. Clickable confirmation actions use
orange chips with dark text and bold keys. Copy and command selections use a
bone cursor bar.

Policy words use coral for required, orange for conditional, sage for
recommended, and dim bone for optional. The selected outline row keeps its
contrasting bone cursor bar; EDIT also marks the current policy in bold with
an underline.

In EDIT, Enter advances through fields, Up and Down move between fields, and
Left and Right move the text cursor or cycle property policy. Esc validates
and saves the item. Amber gutters mark the selected node and focused field.
The surrounding navigation dims, and EDIT appears in the Inspector and footer.
The footer shows only the focused text field's character count, in coral when
that field is invalid. Browsing has no mode badge.
Missing descriptions, invalid or duplicate normalized
names, and incomplete conditional policies keep EDIT open, focus the first
invalid field, and show inline errors. Tab, navigation clicks, and quit cannot
leave EDIT. The Esc hint is cancel for a new item with no text or child entries, even
after changing its policy. Typing changes the hint to save; deleting every
text field back to empty restores cancel. Hidden condition and rationale
text also prevent cancellation. Esc removes a blank new item without writing;
a partly completed item must be corrected. Existing records always use save.

Names such as `Team Changes` save as `team-changes`. Name and Condition accept
80 graphemes; Description and Rationale accept 100. Loaded content beyond those
limits remains intact and may shrink. Edited descriptions and rationales are
trimmed, while unchanged loaded prose is preserved. A no-op edit performs no
write or version increment. Successful Library saves remain quiet; write
failures still report an actionable error and preserve the edit for retry.

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

Configuration requires 66 columns and 18 rows, inside the shell or alone.
Resizing retains editor state. The outline, Inspector, and footer adapt to
compact and wide terminals.

## Commit authoring

`gitserious commit` starts authoring from the default taxonomy in
`gitserious.toml`. Picker tabs support Tab/Shift+Tab to switch forward/backward and
show taxonomy identities and never mutate project policy.

The composer retains the established schema-document interaction: immutable
section headings, contextual property guidance, completion markers, explicit
conditional applicability, canonical review, and
discard confirmation. Generated messages contain `Gitserious-Taxonomy` and
`Gitserious-Schema` provenance trailers.
