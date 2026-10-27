//! Bounded exhaustive check of the configuration editor's reachable states.
//!
//! The check drives the real [`State`] through every sequence of keys from each seed's
//! [`Alphabet`], up to the seed's depth bound, and checks every entry of [`PROPERTIES`] on every
//! transition, the seed's own keys included. A seed explores every key in [`INPUTS`] unless it
//! names a smaller alphabet, which lets it reach deeper for the same number of replays. After a
//! seed's exploration, each of its [`Reach`] goals must have seen every expected value in some
//! explored state. A state is rebuilt by replaying its keys into a fresh fake workspace and
//! rendering into a [`TestBackend`] after every event, as the terminal loop does. Two states merge
//! only when their [`Snapshot`]s print the same, and properties read nothing but snapshots, so a
//! merge never hides a difference that a property can see. A key that quits the editor ends its
//! branch, and the properties judge it as a transition that drops every draft. Exploration is
//! breadth first, so a counterexample is a shortest one for its seed.
//!
//! To add a property of every transition, write a [`Check`] and add it to [`PROPERTIES`]. To add a
//! property of what a seed can reach, add a [`Reach`] to that seed. Add to [`Snapshot`] any field
//! either one reads that the snapshot does not hold yet.
//!
//! P1, no silent draft loss. A taxonomy draft, type editor, or property editor with unsaved changes
//! closes only after the user confirms the discard or after its changes reach the parent draft or
//! the staged Library. Without it, a user loses typed work without a warning. A stage that the
//! session reports as `No changes to stage.` also closes a draft legitimately: its text differs
//! from the original only in whitespace that staging trims, and the notice says so.
//!
//! P2, an unchanged edit stages nothing. Ctrl+S in the edit editor while the taxonomy draft has no
//! unsaved changes leaves the staged Library as it was. Without it, an edit that changes nothing
//! advances the taxonomy's version and rewrites the user's configuration on apply.
//!
//! P3, the review names every taxonomy change. While the Global review shows, every custom
//! taxonomy whose content differs between the saved and the staged Library appears in the frame
//! as `created`, `edited`, or `deleted` followed by its id. Without it, a user applies a change
//! that the review did not show.
//!
//! P4, the property editor reaches every policy. From a new property editor, the arrow keys and
//! enter alone set the editor to all eight pairs of a requirement level (required, recommended,
//! conditional, optional) and a multiplicity (single, multiple). Without it, a property needs a
//! hand edit of `config.toml` to take a level or multiplicity the editor cannot set.
//!
//! P5, an edit keeps the taxonomy's name. While the editor edits an existing taxonomy, its Name
//! field holds the name the taxonomy was opened with. Without it, a user could rename a taxonomy
//! that projects pin by name, and staging would fail or address the wrong taxonomy.
//!
//! P6, no key grows an entry past its budget. A taxonomy, type, or property's name and description
//! share one budget sized to the element that shows them: 80 characters for a taxonomy or type and
//! 100 for a property. No key leaves an open editor's entry longer than its budget and longer than
//! before. Without it, a user can type text that misshapes the editor's rows or that staging later
//! refuses.
//!
//! P7, every alert is a whole sentence. Every notice a reachable state shows starts with a capital
//! letter, ends a sentence, avoids the application's own words (`session`, `save`, `global
//! configuration`), and appears in the frame in full. Without it, raw domain errors and clipped
//! toasts reach the screen.
//!
//! Scope limits:
//! - Only the keyboard events in [`INPUTS`]. Mouse, paste, and resize events are out of scope.
//! - One terminal of `MINIMUM_WIDTH` by `MINIMUM_HEIGHT` cells. Mouse hit areas that rendering
//!   records are not projected.
//! - The property editor's scroll position and the child editors' initial drafts are private to
//!   `child_editor`. The snapshot sees them only through the rendered frame and each editor's
//!   `is_dirty`. It reads the property editor's policy and multiplicity through a test-only view
//!   that returns the labels the editor renders.
//! - `State` runs standalone, not inside the unified shell, so `ConfigView::request_leave_editor`
//!   is not driven.
//! - Workspace saves always succeed, and the project starts uninitialized.
//! - P3 reads the frame, so it sees only the review rows that fit the viewport. Every seed's review
//!   fits without scrolling.
//! - P4 explores only the up, down, left, right, and enter keys from one new property editor. It
//!   reads the editor's policy and multiplicity, not the frame, because at the minimum size a
//!   conditional property's Policy and Multiplicity rows never fit the viewport together.
//! - A pass covers the seven seeds to their depth bounds, not every reachable state. Every replay
//!   renders each frame in a debug build, so the bounds keep the check near 45 seconds on six
//!   cores. The editor seeds get the deeper bound because P1 lives in the editors. The P4 seed's
//!   bound is deep enough to expand every state its five keys reach.

use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use gitserious_app::GlobalConfiguration;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use super::child_editor::PropertyFocus;
use super::library_view::LibraryFocus;
use super::tests::Workspace;
use super::*;
use crate::confirmation::{DiscardChoice, DiscardDialog};

const fn press(code: KeyCode, modifiers: KeyModifiers) -> Event {
    Event::Key(KeyEvent::new(code, modifiers))
}

const fn plain(code: KeyCode) -> Event {
    press(code, KeyModifiers::NONE)
}

const INPUTS: &[(&str, Event)] = &[
    ("esc", plain(KeyCode::Esc)),
    ("enter", plain(KeyCode::Enter)),
    ("tab", plain(KeyCode::Tab)),
    ("backtab", press(KeyCode::BackTab, KeyModifiers::SHIFT)),
    ("up", plain(KeyCode::Up)),
    ("down", plain(KeyCode::Down)),
    ("left", plain(KeyCode::Left)),
    ("right", plain(KeyCode::Right)),
    ("home", plain(KeyCode::Home)),
    ("end", plain(KeyCode::End)),
    ("backspace", plain(KeyCode::Backspace)),
    ("delete", plain(KeyCode::Delete)),
    ("ctrl+s", press(KeyCode::Char('s'), KeyModifiers::CONTROL)),
    ("ctrl+n", press(KeyCode::Char('n'), KeyModifiers::CONTROL)),
    ("alt+up", press(KeyCode::Up, KeyModifiers::ALT)),
    ("alt+down", press(KeyCode::Down, KeyModifiers::ALT)),
    ("alt+enter", press(KeyCode::Enter, KeyModifiers::ALT)),
    ("/", plain(KeyCode::Char('/'))),
    ("?", plain(KeyCode::Char('?'))),
    ("n", plain(KeyCode::Char('n'))),
    ("e", plain(KeyCode::Char('e'))),
    ("f", plain(KeyCode::Char('f'))),
    ("d", plain(KeyCode::Char('d'))),
    ("y", plain(KeyCode::Char('y'))),
    ("q", plain(KeyCode::Char('q'))),
    ("a", plain(KeyCode::Char('a'))),
    ("1", plain(KeyCode::Char('1'))),
    ("space", plain(KeyCode::Char(' '))),
];

struct Seed {
    name: &'static str,
    workspace: fn() -> Result<Workspace, String>,
    keys: &'static [&'static str],
    ready: fn(&Snapshot) -> bool,
    depth: usize,
    alphabet: Alphabet,
    reaches: &'static [Reach],
}

/// The keys a seed explores after its own keys.
enum Alphabet {
    Every,
    Only(&'static [&'static str]),
}

/// A property of what a seed can reach: every value in `expected` is what `observe` returns for
/// at least one state explored from the seed.
struct Reach {
    name: &'static str,
    observe: fn(&Snapshot) -> Option<String>,
    expected: &'static [&'static str],
}

const SEEDS: &[Seed] = &[
    Seed {
        name: "built-ins at Library",
        workspace: built_ins,
        keys: &["down", "right"],
        ready: at_taxonomy_list,
        depth: 2,
        alphabet: Alphabet::Every,
        reaches: &[],
    },
    Seed {
        name: "team at Library",
        workspace: team_library,
        keys: &["down", "right", "down", "down", "down"],
        ready: team_selected,
        depth: 2,
        alphabet: Alphabet::Every,
        reaches: &[],
    },
    Seed {
        name: "built-ins with the create editor open",
        workspace: built_ins,
        keys: &["down", "right", "n"],
        ready: creating,
        depth: 3,
        alphabet: Alphabet::Every,
        reaches: &[],
    },
    Seed {
        name: "a fork of conventional open at its full budget",
        workspace: built_ins,
        keys: &["down", "right", "f"],
        ready: forking,
        depth: 2,
        alphabet: Alphabet::Every,
        reaches: &[],
    },
    Seed {
        name: "team with the edit editor open",
        workspace: team_library,
        keys: &["down", "right", "down", "down", "down", "e"],
        ready: editing,
        depth: 3,
        alphabet: Alphabet::Every,
        reaches: &[],
    },
    Seed {
        name: "team edited at Library",
        workspace: team_library,
        keys: &[
            "down", "right", "down", "down", "down", "e", "up", "a", "ctrl+s",
        ],
        ready: team_selected,
        depth: 2,
        alphabet: Alphabet::Every,
        reaches: &[],
    },
    Seed {
        name: "a fork created and team deleted at Library",
        workspace: team_library,
        keys: &[
            "down",
            "right",
            "f",
            "backspace",
            "ctrl+s",
            "down",
            "down",
            "down",
            "d",
            "y",
        ],
        ready: at_taxonomy_list,
        depth: 2,
        alphabet: Alphabet::Every,
        reaches: &[],
    },
    Seed {
        name: "team with a new property editor open",
        workspace: team_library,
        keys: &[
            "down", "right", "down", "down", "down", "e", "down", "enter", "ctrl+n",
        ],
        ready: adding_property,
        depth: 13,
        alphabet: Alphabet::Only(&["up", "down", "left", "right", "enter"]),
        reaches: &[Reach {
            name: "P4, the property editor reaches every policy",
            observe: editor_policy,
            expected: &[
                "required single",
                "required multiple",
                "recommended single",
                "recommended multiple",
                "conditional single",
                "conditional multiple",
                "optional single",
                "optional multiple",
            ],
        }],
    },
];

fn at_taxonomy_list(snapshot: &Snapshot) -> bool {
    matches!(snapshot.screen, Screen::Home)
        && snapshot.category == Category::Library
        && snapshot.home_focus == HomeFocus::Content
        && snapshot.library_focus == LibraryFocus::Taxonomies
}

fn team_selected(snapshot: &Snapshot) -> bool {
    at_taxonomy_list(snapshot) && snapshot.selected_taxonomy.as_deref() == Some("team")
}

fn creating(snapshot: &Snapshot) -> bool {
    matches!(snapshot.screen, Screen::TaxonomyEditor) && snapshot.editor_mode == EditorMode::Create
}

fn forking(snapshot: &Snapshot) -> bool {
    creating(snapshot) && snapshot.fork_source.is_some()
}

fn editing(snapshot: &Snapshot) -> bool {
    matches!(snapshot.screen, Screen::TaxonomyEditor) && snapshot.editor_mode == EditorMode::Edit
}

fn adding_property(snapshot: &Snapshot) -> bool {
    matches!(snapshot.screen, Screen::PropertyEditor)
        && snapshot
            .property_editor
            .as_ref()
            .is_some_and(|editor| editor.target.is_none())
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "every seed builds its workspace through one signature"
)]
fn built_ins() -> Result<Workspace, String> {
    Ok(Workspace::new())
}

fn team_library() -> Result<Workspace, String> {
    let workspace = Workspace::new();
    *workspace.global.borrow_mut() =
        ConfigurationSession::open_global(team_configuration().map_err(|error| error.to_string())?);
    Ok(workspace)
}

fn team_configuration() -> Result<GlobalConfiguration, Box<dyn std::error::Error>> {
    let team = Taxonomy::new(
        TaxonomyId::new("team")?,
        TaxonomyVersion::new(1)?,
        Description::new("Team taxonomy")?,
        None,
        vec![CommitTypeDefinition::new(
            SchemaVersion::new(1)?,
            CommitTypeId::new("release")?,
            "Release notes",
            vec![PropertyDefinition::new(
                PropertyKey::new("summary")?,
                "What shipped",
                PropertyRequirement::Required,
                PropertyMultiplicity::Single,
            )?],
        )?],
    )?;
    let conventional = TaxonomyId::new("conventional")?;
    Ok(GlobalConfiguration::new(
        1,
        conventional.clone(),
        vec![conventional],
        vec![team],
    )?)
}

#[derive(Debug)]
#[expect(dead_code, reason = "the fingerprint reads these fields through Debug")]
struct Text {
    lines: Vec<String>,
    cursor: (usize, usize),
}

impl Text {
    fn of(area: &TextArea<'_>) -> Self {
        Self {
            lines: area.lines().to_vec(),
            cursor: area.cursor(),
        }
    }
}

#[derive(Debug)]
#[expect(dead_code, reason = "the fingerprint reads these fields through Debug")]
struct TypeEditorView {
    target: Option<usize>,
    draft: TypeDraft,
    name: Text,
    description: Text,
    focus: CreateFocus,
    selected: usize,
    offset: usize,
    dirty: bool,
}

#[derive(Debug)]
#[expect(dead_code, reason = "the fingerprint reads these fields through Debug")]
struct PropertyEditorView {
    target: Option<usize>,
    name: Text,
    description: Text,
    condition_name: Text,
    condition_rationale: Text,
    focus: PropertyFocus,
    policy: [&'static str; 2],
    dirty: bool,
}

/// Everything a user can observe or that later input can affect, as a property sees it.
#[derive(Debug)]
#[expect(dead_code, reason = "the fingerprint reads these fields through Debug")]
struct Snapshot {
    screen: Screen,
    category: Category,
    home_focus: HomeFocus,
    project_section: ProjectSection,
    active_scope: ConfigurationDestination,
    library_focus: LibraryFocus,
    selected_taxonomy: Option<String>,
    library: String,
    overlay: Option<String>,
    child_selected: usize,
    create_focus: CreateFocus,
    create_type_offset: usize,
    scroll: u16,
    project_scroll: u16,
    status: Option<String>,
    editor_mode: EditorMode,
    draft: Option<TaxonomyDraft>,
    initial_draft: Option<TaxonomyDraft>,
    draft_dirty: bool,
    create_text: Option<[Text; 2]>,
    fork_source: Option<String>,
    type_editor: Option<TypeEditorView>,
    property_editor: Option<PropertyEditorView>,
    settings: Option<SettingsDraft>,
    global: Option<ConfigurationSession>,
    project: Option<ConfigurationSession>,
    load_errors: [String; 2],
    saved: [ConfigurationSession; 2],
    discard_dialog: String,
    too_small: bool,
    frame: String,
}

impl Snapshot {
    fn of(state: &State, workspace: &Workspace, terminal: &Terminal<TestBackend>) -> Self {
        let buffer = terminal.backend().buffer();
        Self {
            screen: state.screen.clone(),
            category: state.category,
            home_focus: state.home_focus,
            project_section: state.project_section,
            active_scope: state.active_scope,
            library_focus: state.library.focus,
            selected_taxonomy: state
                .selected_taxonomy()
                .map(|taxonomy| taxonomy.id().to_string()),
            library: format!("{:?}", state.library),
            overlay: state
                .home_overlay
                .as_ref()
                .map(|overlay| format!("{overlay:?}")),
            child_selected: state.child_selected,
            create_focus: state.create_focus,
            create_type_offset: state.create_type_offset,
            scroll: state.scroll,
            project_scroll: state.project_scroll,
            status: state.status.as_ref().map(|notice| match notice {
                Notice::Error(message) => format!("error: {message}"),
                Notice::Info(message) => format!("info: {message}"),
            }),
            editor_mode: state.editor_mode,
            draft: state.editor.clone(),
            initial_draft: state.initial_editor.clone(),
            draft_dirty: state.taxonomy_is_dirty(),
            create_text: state
                .create_text
                .as_ref()
                .map(|text| [Text::of(&text.name), Text::of(&text.description)]),
            fork_source: state
                .fork_source
                .as_ref()
                .map(|source| format!("{source:?}")),
            type_editor: state.type_editor.as_ref().map(|editor| TypeEditorView {
                target: editor.target,
                draft: editor.draft.clone(),
                name: Text::of(&editor.text.name),
                description: Text::of(&editor.text.description),
                focus: editor.focus,
                selected: editor.selected,
                offset: editor.offset,
                dirty: editor.is_dirty(),
            }),
            property_editor: state
                .property_editor
                .as_ref()
                .map(|editor| PropertyEditorView {
                    target: editor.target,
                    name: Text::of(&editor.text.name),
                    description: Text::of(&editor.text.description),
                    condition_name: Text::of(&editor.condition_name),
                    condition_rationale: Text::of(&editor.condition_rationale),
                    focus: editor.focus,
                    policy: editor.policy_labels(),
                    dirty: editor.is_dirty(),
                }),
            settings: state.settings.clone(),
            global: state.global.clone(),
            project: state.project.clone(),
            load_errors: [state.global_error.clone(), state.project_error.clone()],
            saved: [
                workspace.global.borrow().clone(),
                workspace.project.borrow().clone(),
            ],
            discard_dialog: format!("{:?}", state.discard_dialog),
            too_small: state.too_small,
            frame: buffer
                .content
                .chunks(usize::from(buffer.area.width))
                .map(|row| row.iter().map(ratatui::buffer::Cell::symbol).collect())
                .collect::<Vec<String>>()
                .join("\n"),
        }
    }

    fn staged_library(&self) -> Option<&GlobalConfiguration> {
        self.global.as_ref().map(ConfigurationSession::global)
    }
}

/// Judges one transition. `after` is `None` when the input quit the editor.
type Check = fn(&Snapshot, &Event, Option<&Snapshot>) -> Result<(), String>;

const PROPERTIES: &[(&str, Check)] = &[
    ("P1, no silent draft loss", no_silent_draft_loss),
    (
        "P2, an unchanged edit stages nothing",
        unchanged_edit_stages_nothing,
    ),
    (
        "P3, the review names every taxonomy change",
        review_names_every_taxonomy_change,
    ),
    ("P5, an edit keeps the taxonomy's name", edit_keeps_the_name),
    (
        "P6, no key grows an entry past its budget",
        no_key_grows_past_the_budget,
    ),
    (
        "P7, every alert is a whole sentence",
        every_alert_is_a_whole_sentence,
    ),
];

/// Words from the application's vocabulary that the editor never shows: it applies changes and
/// calls the global configuration the Library.
const FOREIGN_WORDS: [&str; 3] = ["session", "save ", "global configuration"];

fn every_alert_is_a_whole_sentence(
    _: &Snapshot,
    _: &Event,
    after: Option<&Snapshot>,
) -> Result<(), String> {
    let Some((after, status)) = after.and_then(|after| Some((after, after.status.as_deref()?)))
    else {
        return Ok(());
    };
    let text = status.split_once(": ").map_or(status, |(_, text)| text);
    let lowered = text.to_lowercase();
    let problem = if !text.chars().next().is_some_and(char::is_uppercase) {
        Some("does not start with a capital letter".to_owned())
    } else if !text.ends_with(['.', '?', '!']) {
        Some("does not end a sentence".to_owned())
    } else if let Some(word) = FOREIGN_WORDS.iter().find(|word| lowered.contains(*word)) {
        Some(format!("uses {:?}", word.trim()))
    } else {
        text.split_whitespace()
            .find(|word| !after.frame.contains(word))
            .map(|word| format!("is cut off before {word:?}"))
    };
    problem.map_or(Ok(()), |problem| {
        Err(format!("The alert {text:?} {problem}."))
    })
}

struct ChildEditor {
    name: &'static str,
    dirty: fn(&Snapshot) -> Option<bool>,
    parent: fn(&Snapshot) -> String,
}

const CHILD_EDITORS: [ChildEditor; 2] = [
    ChildEditor {
        name: "type editor",
        dirty: |snapshot| snapshot.type_editor.as_ref().map(|editor| editor.dirty),
        parent: |snapshot| format!("{:?}", snapshot.draft),
    },
    ChildEditor {
        name: "property editor",
        dirty: |snapshot| snapshot.property_editor.as_ref().map(|editor| editor.dirty),
        parent: |snapshot| format!("{:?}", snapshot.type_editor.as_ref().map(|e| &e.draft)),
    },
];

fn no_silent_draft_loss(
    before: &Snapshot,
    input: &Event,
    after: Option<&Snapshot>,
) -> Result<(), String> {
    let chose_discard =
        DiscardDialog::default().handle_event(input) == Some(DiscardChoice::Discard);
    let staged_nothing =
        after.is_some_and(|after| after.status.as_deref() == Some("info: No changes to stage."));
    if before.draft_dirty
        && after.is_none_or(|after| after.draft.is_none())
        && !(matches!(before.screen, Screen::ConfirmDiscardCreate(_)) && chose_discard)
        && !staged_nothing
        && after.is_none_or(|after| after.staged_library() == before.staged_library())
    {
        return Err(
            "The taxonomy draft had unsaved changes and closed without a confirmed \
             discard or a staged change."
                .into(),
        );
    }
    for child in CHILD_EDITORS {
        if (child.dirty)(before) == Some(true)
            && after.is_none_or(|after| (child.dirty)(after).is_none())
            && !(matches!(before.screen, Screen::ConfirmDiscardChild(_)) && chose_discard)
            && after.is_none_or(|after| (child.parent)(after) == (child.parent)(before))
        {
            return Err(format!(
                "The {} had unsaved changes and closed without a confirmed discard or a change \
                 to its parent draft.",
                child.name
            ));
        }
    }
    Ok(())
}

/// One editor's name and description, and the budget they share.
struct Budgeted {
    owner: &'static str,
    fields: fn(&Snapshot) -> Option<[&Text; 2]>,
    limit: usize,
}

const BUDGETED: [Budgeted; 3] = [
    Budgeted {
        owner: "taxonomy",
        fields: |snapshot| {
            snapshot
                .create_text
                .as_ref()
                .map(|[name, description]| [name, description])
        },
        limit: CREATE_TEXT_LIMIT,
    },
    Budgeted {
        owner: "type",
        fields: |snapshot| {
            snapshot
                .type_editor
                .as_ref()
                .map(|editor| [&editor.name, &editor.description])
        },
        limit: child_editor::TYPE_LIMIT,
    },
    Budgeted {
        owner: "property",
        fields: |snapshot| {
            snapshot
                .property_editor
                .as_ref()
                .map(|editor| [&editor.name, &editor.description])
        },
        limit: child_editor::PROPERTY_LIMIT,
    },
];

fn used(fields: [&Text; 2]) -> usize {
    fields
        .iter()
        .map(|field| field.lines.join("\n").graphemes(true).count())
        .sum()
}

fn no_key_grows_past_the_budget(
    before: &Snapshot,
    _: &Event,
    after: Option<&Snapshot>,
) -> Result<(), String> {
    let Some(after) = after else {
        return Ok(());
    };
    for Budgeted {
        owner,
        fields,
        limit,
    } in BUDGETED
    {
        if let (Some(old), Some(new)) = (fields(before), fields(after)) {
            let (old, new) = (used(old), used(new));
            if new > limit && new > old {
                return Err(format!(
                    "The {owner} name and description grew from {old} to {new} characters, past \
                     its budget of {limit}."
                ));
            }
        }
    }
    Ok(())
}

fn edit_keeps_the_name(_: &Snapshot, _: &Event, after: Option<&Snapshot>) -> Result<(), String> {
    let Some(after) = after.filter(|after| after.editor_mode == EditorMode::Edit) else {
        return Ok(());
    };
    if let (Some([name, _]), Some(initial)) = (&after.create_text, &after.initial_draft)
        && name.lines.join("\n") != initial.id
    {
        return Err(format!(
            "The edit editor's Name field reads {:?}, not the taxonomy's name {:?}.",
            name.lines.join("\n"),
            initial.id
        ));
    }
    Ok(())
}

fn unchanged_edit_stages_nothing(
    before: &Snapshot,
    input: &Event,
    after: Option<&Snapshot>,
) -> Result<(), String> {
    let stage = Event::Key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    if matches!(before.screen, Screen::TaxonomyEditor)
        && before.editor_mode == EditorMode::Edit
        && !before.draft_dirty
        && *input == stage
        && after.is_some_and(|after| after.staged_library() != before.staged_library())
    {
        return Err("Ctrl+S staged a change from an edit draft with no unsaved changes.".into());
    }
    Ok(())
}

fn find<'a>(library: &'a [Taxonomy], id: &TaxonomyId) -> Option<&'a Taxonomy> {
    library.iter().find(|taxonomy| taxonomy.id() == id)
}

fn review_names_every_taxonomy_change(
    _: &Snapshot,
    _: &Event,
    after: Option<&Snapshot>,
) -> Result<(), String> {
    let Some(after) = after.filter(|after| {
        matches!(
            after.screen,
            Screen::Review(ConfigurationDestination::Global)
        )
    }) else {
        return Ok(());
    };
    let saved = after.saved[0].global().custom_taxonomies();
    let staged = after
        .staged_library()
        .map_or(&[][..], GlobalConfiguration::custom_taxonomies);
    let changes = staged
        .iter()
        .filter_map(|taxonomy| match find(saved, taxonomy.id()) {
            None => Some(("created", taxonomy.id())),
            Some(before) if before != taxonomy => Some(("edited", taxonomy.id())),
            Some(_) => None,
        })
        .chain(
            saved
                .iter()
                .filter(|taxonomy| find(staged, taxonomy.id()).is_none())
                .map(|taxonomy| ("deleted", taxonomy.id())),
        );
    let rows = after
        .frame
        .lines()
        .map(|row| {
            row.replace('│', " ")
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    for (kind, id) in changes {
        let named = rows.iter().any(|words| {
            words
                .windows(2)
                .any(|pair| pair[0] == kind && pair[1] == id.as_str())
        });
        if !named {
            return Err(format!(
                "The Global review does not name {kind} taxonomy \"{id}\"."
            ));
        }
    }
    Ok(())
}

fn editor_policy(snapshot: &Snapshot) -> Option<String> {
    snapshot
        .property_editor
        .as_ref()
        .filter(|_| matches!(snapshot.screen, Screen::PropertyEditor))
        .map(|editor| editor.policy.join(" "))
}

/// Replays `keys` from a fresh workspace. Returns `None` when the last key quit the editor.
fn replay(seed: &Seed, keys: &[usize]) -> Result<Option<Snapshot>, String> {
    let workspace = (seed.workspace)()?;
    let mut state = State::new(&workspace);
    let mut terminal = Terminal::new(TestBackend::new(MINIMUM_WIDTH, MINIMUM_HEIGHT))
        .map_err(|error| error.to_string())?;
    let mut draw = |state: &mut State| {
        terminal
            .draw(|frame| state.render(frame))
            .map(|_| ())
            .map_err(|error| error.to_string())
    };
    draw(&mut state)?;
    for &key in keys {
        if state.handle_event(INPUTS[key].1.clone(), &workspace) {
            return Ok(None);
        }
        draw(&mut state)?;
    }
    Ok(Some(Snapshot::of(&state, &workspace, &terminal)))
}

fn labels(keys: &[usize]) -> String {
    keys.iter()
        .map(|&key| INPUTS[key].0)
        .collect::<Vec<_>>()
        .join(" ")
}

fn check(
    seed: &Seed,
    keys: &[usize],
    before: &Snapshot,
    after: &Result<Option<Snapshot>, String>,
) -> Result<(), String> {
    let seed_keys = seed.keys.len().min(keys.len() - 1);
    let report = |name: &str, reason: &str, after: &str| {
        format!(
            "{name} failed from seed \"{}\".\nSeed keys: {}\nKeys after the seed: {}\n\
             Screen before the last key: {:?}\nScreen after the last key: {after}\n{reason}",
            seed.name,
            labels(&keys[..seed_keys]),
            labels(&keys[seed_keys..]),
            before.screen,
        )
    };
    let after = match after {
        Ok(after) => after.as_ref(),
        Err(error) => return Err(report("Replay", error, "none")),
    };
    let after_screen = after.map_or_else(|| "quit".into(), |after| format!("{:?}", after.screen));
    for (name, holds) in PROPERTIES {
        holds(before, &INPUTS[keys[keys.len() - 1]].1, after)
            .map_err(|reason| report(name, &reason, &after_screen))?;
    }
    Ok(())
}

fn replay_all(seed: &Seed, jobs: &[Vec<usize>]) -> Vec<Result<Option<Snapshot>, String>> {
    let claimed = AtomicUsize::new(0);
    let workers = thread::available_parallelism().map_or(1, NonZeroUsize::get);
    let mut outcomes = thread::scope(|scope| {
        let handles = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    let mut done = Vec::new();
                    loop {
                        let index = claimed.fetch_add(1, Ordering::Relaxed);
                        let Some(keys) = jobs.get(index) else {
                            return done;
                        };
                        let outcome = panic::catch_unwind(AssertUnwindSafe(|| replay(seed, keys)))
                            .unwrap_or_else(|_| Err("The editor panicked.".into()));
                        done.push((index, outcome));
                    }
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .flat_map(|handle| {
                handle
                    .join()
                    .unwrap_or_else(|panic| panic::resume_unwind(panic))
            })
            .collect::<Vec<_>>()
    });
    outcomes.sort_unstable_by_key(|(index, _)| *index);
    outcomes.into_iter().map(|(_, outcome)| outcome).collect()
}

struct Coverage {
    states: usize,
    transitions: usize,
    elapsed: Duration,
}

fn input_indices(seed: &Seed, labels: &[&str]) -> Result<Vec<usize>, String> {
    labels
        .iter()
        .map(|label| {
            INPUTS
                .iter()
                .position(|(input, _)| input == label)
                .ok_or_else(|| format!("seed \"{}\" names unknown key {label}", seed.name))
        })
        .collect()
}

fn explore(seed: &Seed) -> Result<Coverage, String> {
    let started = Instant::now();
    let prefix = input_indices(seed, seed.keys)?;
    let alphabet = match seed.alphabet {
        Alphabet::Every => (0..INPUTS.len()).collect(),
        Alphabet::Only(labels) => input_indices(seed, labels)?,
    };
    let mut before = replay(seed, &[])?.ok_or("the editor quit before any key")?;
    for end in 1..=prefix.len() {
        let after = replay(seed, &prefix[..end]);
        check(seed, &prefix[..end], &before, &after)?;
        before = after?.ok_or_else(|| format!("seed \"{}\" quit the editor", seed.name))?;
    }
    if !(seed.ready)(&before) {
        return Err(format!(
            "seed \"{}\" does not reach its start state; it reached {:?}",
            seed.name, before.screen
        ));
    }
    let mut visited = HashSet::from([format!("{before:?}")]);
    let mut observed = seed
        .reaches
        .iter()
        .map(|reach| (reach.observe)(&before).into_iter().collect::<HashSet<_>>())
        .collect::<Vec<_>>();
    let mut frontier = vec![(prefix, before)];
    let mut transitions = 0;
    for level in 1..=seed.depth {
        let mut next = Vec::new();
        let edges = (0..frontier.len())
            .flat_map(|parent| alphabet.iter().map(move |&input| (parent, input)))
            .collect::<Vec<_>>();
        let jobs = edges
            .iter()
            .map(|&(parent, input)| {
                let mut keys = frontier[parent].0.clone();
                keys.push(input);
                keys
            })
            .collect::<Vec<_>>();
        let outcomes = replay_all(seed, &jobs);
        for ((&(parent, _), keys), after) in edges.iter().zip(jobs).zip(outcomes) {
            transitions += 1;
            check(seed, &keys, &frontier[parent].1, &after)?;
            if let Ok(Some(after)) = after
                && visited.insert(format!("{after:?}"))
            {
                for (reach, values) in seed.reaches.iter().zip(&mut observed) {
                    values.extend((reach.observe)(&after));
                }
                if level < seed.depth {
                    next.push((keys, after));
                }
            }
        }
        frontier = next;
    }
    for (reach, values) in seed.reaches.iter().zip(&observed) {
        let missing = reach
            .expected
            .iter()
            .filter(|value| !values.contains(**value))
            .copied()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            let mut values = values.iter().map(String::as_str).collect::<Vec<_>>();
            values.sort_unstable();
            return Err(format!(
                "{} failed from seed \"{}\".\nSeed keys: {}\nExplored to depth {} with: {}\n\
                 Never reached: {}\nReached: {}",
                reach.name,
                seed.name,
                seed.keys.join(" "),
                seed.depth,
                labels(&alphabet),
                missing.join(", "),
                values.join(", "),
            ));
        }
    }
    Ok(Coverage {
        states: visited.len(),
        transitions,
        elapsed: started.elapsed(),
    })
}

#[test]
#[expect(clippy::print_stderr, reason = "coverage shows under --nocapture")]
fn every_bounded_key_sequence_keeps_the_config_editor_properties() {
    let mut failures = Vec::new();
    for seed in SEEDS {
        match explore(seed) {
            Ok(coverage) => eprintln!(
                "{}: depth {}, {} states, {} transitions, {:.1?}",
                seed.name, seed.depth, coverage.states, coverage.transitions, coverage.elapsed
            ),
            Err(failure) => failures.push(failure),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
