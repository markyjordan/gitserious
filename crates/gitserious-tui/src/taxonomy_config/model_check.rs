//! Bounded exhaustive exploration of real Config events and rendered frames.
//!
//! Seeds cover blank creation, forks, existing fields, conditional policy, limits and copies.
//! Every transition checks modal editing, direct persistence, deletion confirmation, saved
//! eligibility badges, the new taxonomy row that ends the Library, name locking, field limits,
//! inline messages and copy independence. Esc restores the checkpoint taken before the
//! active edit and writes nothing. Ctrl+W finishes that edit only after storage succeeds.
//! At most one taxonomy root is expanded, while type expansion can be remembered independently.
//! The existing breadth-first runner deduplicates snapshots and reports shortest counterexamples.
//! Keys the shell takes before Config still reach it here. Mouse, resize, write failures and the shell are covered by interaction
//! tests.

use gitserious_app::test_support::LibraryLookup;
use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use gitserious_app::{
    DraftField, GlobalConfiguration, NodeRef, ProblemSite, TaxonomyNode, TypeNode,
};
use gitserious_core::{
    CommitTypeDefinition, CommitTypeId, Description, PropertyDefinition, PropertyKey,
    PropertyRequirement, SchemaVersion, TaxonomyVersion,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use unicode_segmentation::UnicodeSegmentation;

use super::naming::problem_sentence;
use super::taxonomies::{Observation, ObservedMode as Mode};
use super::tests::Workspace;
use super::*;
use crate::confirmation::{self, Answer, ConfirmDialog, REMOVE};

const fn press(code: KeyCode, modifiers: KeyModifiers) -> Event {
    Event::Key(KeyEvent::new(code, modifiers))
}

const fn plain(code: KeyCode) -> Event {
    press(code, KeyModifiers::NONE)
}

const INPUTS: &[(&str, Event)] = &[
    ("esc", plain(KeyCode::Esc)),
    ("ctrl+w", press(KeyCode::Char('w'), KeyModifiers::CONTROL)),
    ("enter", plain(KeyCode::Enter)),
    ("alt+enter", press(KeyCode::Enter, KeyModifiers::ALT)),
    ("tab", plain(KeyCode::Tab)),
    ("backtab", press(KeyCode::BackTab, KeyModifiers::SHIFT)),
    ("up", plain(KeyCode::Up)),
    ("down", plain(KeyCode::Down)),
    ("alt+up", press(KeyCode::Up, KeyModifiers::ALT)),
    ("alt+down", press(KeyCode::Down, KeyModifiers::ALT)),
    ("left", plain(KeyCode::Left)),
    ("right", plain(KeyCode::Right)),
    ("home", plain(KeyCode::Home)),
    ("end", plain(KeyCode::End)),
    ("backspace", plain(KeyCode::Backspace)),
    ("delete", plain(KeyCode::Delete)),
    ("ctrl+s", press(KeyCode::Char('s'), KeyModifiers::CONTROL)),
    ("/", plain(KeyCode::Char('/'))),
    ("?", plain(KeyCode::Char('?'))),
    ("n", plain(KeyCode::Char('n'))),
    ("e", plain(KeyCode::Char('e'))),
    ("f", plain(KeyCode::Char('f'))),
    ("d", plain(KeyCode::Char('d'))),
    ("y", plain(KeyCode::Char('y'))),
    ("q", plain(KeyCode::Char('q'))),
    ("a", plain(KeyCode::Char('a'))),
    ("c", plain(KeyCode::Char('c'))),
    ("1", plain(KeyCode::Char('1'))),
    ("2", plain(KeyCode::Char('2'))),
    ("space", plain(KeyCode::Char(' '))),
];

const NAMED_ONLY: &[&str] = &["c"];

const COPY_KEYS: &[&str] = &[
    "esc",
    "enter",
    "up",
    "down",
    "left",
    "right",
    "e",
    "a",
    "backspace",
    "space",
    "c",
    "ctrl+s",
    "ctrl+w",
];

struct Seed {
    name: &'static str,
    workspace: fn() -> Result<Workspace, String>,
    start: Start,
    depth: usize,
    alphabet: Alphabet,
    reaches: &'static [Reach],
}

/// Where a seed's exploration starts.
enum Start {
    /// The Taxonomies outline with the row these names lead to selected, in `mode`.
    Outline {
        path: &'static [&'static str],
        mode: StartMode,
    },
    /// The state these keys reach, which `ready` must accept.
    Keys {
        keys: &'static [&'static str],
        ready: fn(&Snapshot) -> bool,
    },
}

#[derive(Clone, Copy, PartialEq)]
enum StartMode {
    Nav,
    Edit(DraftField),
}

/// The keys a seed explores after its start.
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

const SUMMARY: &[&str] = &["team", "release", "summary"];

const SEEDS: &[Seed] = &[
    Seed {
        name: "an earlier root selected while a later root is expanded",
        workspace: built_ins,
        start: Start::Keys {
            keys: &["2", "down", "enter", "up"],
            ready: |snapshot| {
                at_outline(snapshot)
                    && selects(snapshot, &["conventional"])
                    && snapshot.taxonomies.expanded.len() == 1
            },
        },
        depth: 2,
        alphabet: Alphabet::Every,
        reaches: &[],
    },
    Seed {
        name: "built-ins at Taxonomies",
        workspace: built_ins,
        start: Start::Outline {
            path: &["conventional"],
            mode: StartMode::Nav,
        },
        depth: 2,
        alphabet: Alphabet::Every,
        reaches: &[],
    },
    Seed {
        name: "team at Taxonomies",
        workspace: team_library,
        start: Start::Outline {
            path: &["team"],
            mode: StartMode::Nav,
        },
        depth: 2,
        alphabet: Alphabet::Every,
        reaches: &[],
    },
    Seed {
        name: "team release expanded at Taxonomies",
        workspace: team_library,
        start: Start::Outline {
            path: SUMMARY,
            mode: StartMode::Nav,
        },
        depth: 2,
        alphabet: Alphabet::Every,
        reaches: &[],
    },
    Seed {
        name: "a saved empty taxonomy in NAV",
        workspace: empty_library,
        start: Start::Outline {
            path: &["unfinished"],
            mode: StartMode::Nav,
        },
        depth: 2,
        alphabet: Alphabet::Every,
        reaches: &[],
    },
    Seed {
        name: "a saved empty taxonomy with an unlocked name in EDIT",
        workspace: empty_library,
        start: Start::Outline {
            path: &["unfinished"],
            mode: StartMode::Edit(DraftField::Name),
        },
        depth: 2,
        alphabet: Alphabet::Every,
        reaches: &[],
    },
    Seed {
        name: "a new taxonomy in EDIT on its Name",
        workspace: built_ins,
        start: Start::Keys {
            keys: &["2", "n"],
            ready: |snapshot| editing(snapshot) == Some(DraftField::Name),
        },
        depth: 3,
        alphabet: Alphabet::Every,
        reaches: &[],
    },
    Seed {
        name: "a fork of conventional in EDIT on its Name",
        workspace: built_ins,
        start: Start::Keys {
            keys: &["2", "f"],
            ready: |snapshot| editing(snapshot) == Some(DraftField::Name),
        },
        depth: 2,
        alphabet: Alphabet::Every,
        reaches: &[],
    },
    Seed {
        name: "team in EDIT on its Description at the limit",
        workspace: team_library,
        start: Start::Outline {
            path: &["team"],
            mode: StartMode::Edit(DraftField::Description),
        },
        depth: 3,
        alphabet: Alphabet::Every,
        reaches: &[],
    },
    Seed {
        name: "team summary in EDIT on its Name",
        workspace: team_library,
        start: Start::Outline {
            path: SUMMARY,
            mode: StartMode::Edit(DraftField::Name),
        },
        depth: 2,
        alphabet: Alphabet::Every,
        reaches: &[],
    },
    Seed {
        name: "team edited at Taxonomies",
        workspace: team_library,
        start: Start::Keys {
            keys: &["2", "down", "down", "down", "e", "backspace", "esc"],
            ready: |snapshot| at_outline(snapshot) && !snapshot.unapplied,
        },
        depth: 2,
        alphabet: Alphabet::Every,
        reaches: &[],
    },
    Seed {
        name: "a saved fork at Taxonomies",
        workspace: team_library,
        start: Start::Keys {
            keys: &["2", "f", "esc", "up", "d", "y"],
            ready: |snapshot| at_outline(snapshot) && !snapshot.unapplied,
        },
        depth: 2,
        alphabet: Alphabet::Every,
        reaches: &[],
    },
    Seed {
        name: "a new property from its add row in EDIT on its Name",
        workspace: team_library,
        start: Start::Keys {
            keys: &[
                "2", "down", "down", "down", "enter", "down", "enter", "down", "down", "down",
                "enter",
            ],
            ready: |snapshot| editing(snapshot) == Some(DraftField::Name),
        },
        depth: 2,
        alphabet: Alphabet::Every,
        reaches: &[],
    },
    Seed {
        name: "team summary conditional with an empty condition in EDIT on its Condition",
        workspace: team_library,
        start: Start::Keys {
            keys: &[
                "2", "down", "down", "down", "enter", "down", "enter", "down", "e", "enter",
                "enter", "right", "right", "enter",
            ],
            ready: |snapshot| editing(snapshot) == Some(DraftField::Condition),
        },
        depth: 2,
        alphabet: Alphabet::Every,
        reaches: &[],
    },
    Seed {
        name: "team summary's policy in NAV",
        workspace: team_library,
        start: Start::Outline {
            path: SUMMARY,
            mode: StartMode::Nav,
        },
        depth: 5,
        alphabet: Alphabet::Only(&["e", "up", "down", "left", "right", "enter"]),
        reaches: &[Reach {
            name: "P4, the Inspector's Policy reaches every policy",
            observe: selected_policy,
            expected: &["required", "recommended", "conditional", "optional"],
        }],
    },
    Seed {
        name: "summary copied to hotfix, on the source in NAV",
        workspace: team_pair_library,
        start: Start::Keys {
            keys: COPY_SUMMARY,
            ready: |snapshot| copied(snapshot) && selects(snapshot, SUMMARY),
        },
        depth: 3,
        alphabet: Alphabet::Only(COPY_KEYS),
        reaches: &[],
    },
    Seed {
        name: "summary copied to hotfix, on the copy in NAV",
        workspace: team_pair_library,
        start: Start::Keys {
            keys: &[
                "2", "down", "down", "down", "enter", "down", "enter", "down", "c", "space",
                "enter", "down", "down", "down", "down", "down",
            ],
            ready: |snapshot| copied(snapshot) && selects(snapshot, &["team", "hotfix", "summary"]),
        },
        depth: 3,
        alphabet: Alphabet::Only(COPY_KEYS),
        reaches: &[],
    },
];

const COPY_SUMMARY: &[&str] = &[
    "2", "down", "down", "down", "enter", "down", "enter", "down", "c", "space", "enter",
];

fn copied(snapshot: &Snapshot) -> bool {
    at_outline(snapshot)
        && matches!(snapshot.taxonomies.mode, Mode::Nav)
        && snapshot.library().is_some_and(|library| {
            library
                .taxonomies()
                .iter()
                .flat_map(TaxonomyNode::types)
                .flat_map(TypeNode::properties)
                .any(|property| property.copied_from().is_some())
        })
}

fn at_outline(snapshot: &Snapshot) -> bool {
    matches!(snapshot.screen, Screen::Home)
        && snapshot.category == Category::Taxonomies
        && snapshot.home_focus == HomeFocus::Content
}

fn selects(snapshot: &Snapshot, path: &[&str]) -> bool {
    snapshot
        .taxonomies
        .cursor_path
        .iter()
        .map(String::as_str)
        .eq(path.iter().copied())
}

/// The field EDIT sits on, if the outline is in EDIT.
fn editing(snapshot: &Snapshot) -> Option<DraftField> {
    match &snapshot.taxonomies.mode {
        Mode::Edit { field, .. } if at_outline(snapshot) => Some(*field),
        _ => None,
    }
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "every seed builds its workspace through one signature"
)]
fn built_ins() -> Result<Workspace, String> {
    Ok(Workspace::new())
}

fn empty_library() -> Result<Workspace, String> {
    let workspace = Workspace::new();
    let record = gitserious_app::LibraryTaxonomy::new(
        TaxonomyId::new("unfinished").map_err(|error| error.to_string())?,
        Some(TaxonomyVersion::V1),
        Description::new("Awaiting its first type").map_err(|error| error.to_string())?,
        None,
        Vec::new(),
        false,
    )
    .map_err(|error| error.to_string())?;
    let default = GlobalConfiguration::default();
    *workspace.global.borrow_mut() = LibraryDraft::from_configuration(
        GlobalConfiguration::new(
            1,
            default.default_taxonomy().clone(),
            default.available_taxonomies().to_vec(),
            vec![record],
        )
        .map_err(|error| error.to_string())?,
    );
    Ok(workspace)
}

fn team_library() -> Result<Workspace, String> {
    workspace_of(team_configuration(Vec::new()))
}

fn team_pair_library() -> Result<Workspace, String> {
    let hotfix = || -> Result<CommitTypeDefinition, Box<dyn std::error::Error>> {
        Ok(CommitTypeDefinition::new(
            Some(SchemaVersion::new(1)?),
            CommitTypeId::new("hotfix")?,
            "Urgent fixes",
            vec![PropertyDefinition::new(
                PropertyKey::new("cause")?,
                "Why it broke",
                PropertyRequirement::Required,
            )?],
        )?)
    };
    workspace_of(hotfix().and_then(|hotfix| team_configuration(vec![hotfix])))
}

fn workspace_of(
    configuration: Result<GlobalConfiguration, Box<dyn std::error::Error>>,
) -> Result<Workspace, String> {
    let workspace = Workspace::new();
    *workspace.global.borrow_mut() =
        LibraryDraft::from_configuration(configuration.map_err(|error| error.to_string())?);
    Ok(workspace)
}

/// `team` with a Description at its limit and one type with two properties, so a property can
/// move, then the `more` types.
fn team_configuration(
    more: Vec<CommitTypeDefinition>,
) -> Result<GlobalConfiguration, Box<dyn std::error::Error>> {
    let description = format!("Team taxonomy{}", ".".repeat(87));
    let team = Taxonomy::new(
        TaxonomyId::new("team")?,
        Some(TaxonomyVersion::new(1)?),
        Description::new(description)?,
        None,
        std::iter::once(CommitTypeDefinition::new(
            Some(SchemaVersion::new(1)?),
            CommitTypeId::new("release")?,
            "Release notes",
            vec![
                PropertyDefinition::new(
                    PropertyKey::new("summary")?,
                    "What shipped",
                    PropertyRequirement::Required,
                )?,
                PropertyDefinition::new(
                    PropertyKey::new("notes")?,
                    "Anything else",
                    PropertyRequirement::Optional,
                )?,
            ],
        )?)
        .chain(more)
        .collect(),
    )?;
    let conventional = TaxonomyId::new("conventional")?;
    Ok(GlobalConfiguration::new(
        1,
        conventional.clone(),
        vec![conventional],
        vec![team.into()],
    )?)
}

/// Everything a user can observe or that later input can affect, as a property sees it.
#[derive(Debug)]
#[expect(dead_code, reason = "the fingerprint reads these fields through Debug")]
struct Snapshot {
    screen: Screen,
    category: Category,
    home_focus: HomeFocus,
    project_section: ProjectSection,
    taxonomies: Observation,
    overlay: Option<String>,
    status: Option<String>,
    global: Option<LibraryDraft>,
    project: Option<ProjectSnapshot>,
    /// Whether the Library has changes apply would write.
    unapplied: bool,
    /// The count of unapplied changes the shell's Config tab shows.
    load_errors: [String; 2],
    /// The Library the fake workspace stores. Config never writes the project.
    saved: LibraryDraft,
    dialog: String,
    dialog_hints: Option<Vec<String>>,
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
            taxonomies: state.taxonomies.observe(state.library()),
            overlay: state
                .home_overlay
                .as_ref()
                .map(|overlay| format!("{overlay:?}")),
            status: state.status.as_ref().map(|notice| match notice {
                Notice::Error(message) => format!("error: {message}"),
            }),
            global: state.global.clone(),
            project: state.project.clone(),
            unapplied: state.library_dirty(),
            load_errors: [state.global_error.clone(), state.project_error.clone()],
            saved: workspace.global.borrow().clone(),
            dialog: format!("{:?}", state.dialog),
            dialog_hints: state
                .confirmation()
                .map(|question| confirmation::hints(question.choices))
                .or_else(|| state.taxonomies.picker_hints())
                .map(|hints| {
                    hints
                        .into_iter()
                        .map(|(hint, label)| format!("{hint} {label}"))
                        .collect()
                }),
            too_small: state.too_small,
            frame: buffer
                .content
                .chunks(usize::from(buffer.area.width))
                .map(|row| row.iter().map(ratatui::buffer::Cell::symbol).collect())
                .collect::<Vec<String>>()
                .join("\n"),
        }
    }

    fn library(&self) -> Option<&LibraryDraft> {
        self.global.as_ref()
    }

    fn footer(&self) -> String {
        super::tests::footer_text(self.frame.lines().last().unwrap_or_default())
    }

    fn staged_library(&self) -> Option<GlobalConfiguration> {
        self.library().and_then(|library| library.build().ok())
    }

    /// The Library the fake workspace stores: the last applied session's Library.
    fn stored_library(&self) -> Option<GlobalConfiguration> {
        self.saved.build().ok()
    }

    /// Whether the Taxonomies outline takes `input` with no overlay in the way.
    fn outline_takes_keys(&self) -> bool {
        at_outline(self) && self.overlay.is_none()
    }
}

/// Judges one transition. `after` is `None` when the input quit the editor.
type Check = fn(&Snapshot, &Event, Option<&Snapshot>) -> Result<(), String>;

const PROPERTIES: &[(&str, Check)] = &[
    (
        "Esc restores the edit checkpoint without writing",
        cancellation_restores_without_writing,
    ),
    (
        "P14, at most one taxonomy root is expanded",
        at_most_one_taxonomy_root_is_expanded,
    ),
    (
        "P13, browsing arrows never edit a policy",
        browsing_arrows_keep_policies,
    ),
    (
        "P1, the unapplied draft is dropped only by apply or a confirmed discard",
        no_silent_draft_loss,
    ),
    (
        "P2, a NAV/EDIT round trip with no edit leaves the session clean",
        round_trips_leave_the_session_clean,
    ),
    (
        "P3, navigation exposes only persisted changes",
        navigation_is_persisted,
    ),
    (
        "P5, a stored taxonomy's name never changes",
        stored_names_never_change,
    ),
    (
        "P6, no key grows a field past its own limit",
        no_key_grows_a_field_past_its_limit,
    ),
    (
        "P7, every alert is a whole sentence",
        every_alert_is_a_whole_sentence,
    ),
    (
        "P8, removals go through their dialog",
        removals_go_through_their_dialog,
    ),
    (
        "P9, the footer shows the dialog's choices",
        the_footer_shows_the_dialogs_choices,
    ),
    (
        "P10, only saved empty taxonomies show invalid badges",
        saved_eligibility_badges,
    ),
    (
        "P11, the saved Library never changes while problems exist",
        problems_keep_the_saved_library,
    ),
    (
        "P12, a copy and its source change apart",
        a_copy_and_its_source_change_apart,
    ),
];

fn at_most_one_taxonomy_root_is_expanded(
    _: &Snapshot,
    _: &Event,
    after: Option<&Snapshot>,
) -> Result<(), String> {
    let Some(after) = after else {
        return Ok(());
    };
    let Some(library) = after.library() else {
        return Ok(());
    };
    let expanded = library
        .taxonomies()
        .iter()
        .filter(|taxonomy| after.taxonomies.expanded.contains(taxonomy.id()))
        .map(TaxonomyNode::name)
        .collect::<Vec<_>>();
    if expanded.len() > 1 {
        return Err(format!(
            "Multiple taxonomy roots are expanded: {expanded:?}."
        ));
    }
    Ok(())
}

fn answer(choices: &[confirmation::Choice], input: &Event) -> Option<Answer> {
    ConfirmDialog::default().handle_event(choices, input)
}

/// Whether `input` is a key that edits a draft in place from `before`: a text key in EDIT, ←/→,
/// which cycle a policy, or alt+↑/↓ in NAV, which reorder a type or property.
fn edits(before: &Snapshot, input: &Event) -> bool {
    let Event::Key(key) = input else {
        return false;
    };
    if !before.outline_takes_keys() {
        return false;
    }
    match key.code {
        KeyCode::Left | KeyCode::Right => matches!(
            before.taxonomies.mode,
            Mode::Edit {
                field: DraftField::Policy,
                ..
            }
        ),
        KeyCode::Up | KeyCode::Down => {
            matches!(before.taxonomies.mode, Mode::Nav) && key.modifiers.contains(KeyModifiers::ALT)
        }
        KeyCode::Char(_) | KeyCode::Backspace | KeyCode::Delete => {
            matches!(before.taxonomies.mode, Mode::Edit { .. })
                && !key.modifiers.contains(KeyModifiers::CONTROL)
        }
        _ => false,
    }
}

fn no_silent_draft_loss(
    before: &Snapshot,
    input: &Event,
    after: Option<&Snapshot>,
) -> Result<(), String> {
    if !before.unapplied || after.is_some_and(|after| after.unapplied) {
        return Ok(());
    }
    let legitimate = match &before.screen {
        Screen::ConfirmRemove(_) => answer(REMOVE, input) == Some(Answer::Confirm),
        Screen::Home => {
            cancelled_edit(before, input, after)
                || edits(before, input)
                || matches!(
                    input,
                    Event::Key(KeyEvent {
                        code: KeyCode::Char('w'),
                        modifiers: KeyModifiers::CONTROL,
                        ..
                    })
                ) && matches!(before.taxonomies.mode, Mode::Edit { .. })
        }
    };
    if legitimate {
        return Ok(());
    }
    Err(match after {
        None => "The editor quit with unapplied Library changes and no confirmed discard.".into(),
        Some(_) => "The Library's unapplied changes went away without apply, a confirmed \
                    discard, or an edit."
            .into(),
    })
}

fn round_trips_leave_the_session_clean(
    before: &Snapshot,
    input: &Event,
    after: Option<&Snapshot>,
) -> Result<(), String> {
    if before.unapplied
        || !before.outline_takes_keys()
        || !after.is_some_and(|after| after.unapplied)
    {
        return Ok(());
    }
    let mode = &before.taxonomies.mode;
    let creates = match input {
        Event::Key(KeyEvent {
            code: KeyCode::Char('n' | 'f'),
            ..
        }) => matches!(mode, Mode::Nav),
        Event::Key(KeyEvent {
            code: KeyCode::Enter,
            ..
        }) => matches!(mode, Mode::Nav | Mode::CopyTo { .. }),
        _ => false,
    };
    if creates || edits(before, input) {
        return Ok(());
    }
    Err("A key that edits nothing left the Library with unapplied changes.".into())
}

fn browsing_arrows_keep_policies(
    before: &Snapshot,
    input: &Event,
    after: Option<&Snapshot>,
) -> Result<(), String> {
    let Event::Key(key) = input else {
        return Ok(());
    };
    if !before.outline_takes_keys()
        || !matches!(before.taxonomies.mode, Mode::Nav)
        || !matches!(key.code, KeyCode::Left | KeyCode::Right)
    {
        return Ok(());
    }
    let after = after.ok_or("A browsing arrow quit the editor.")?;
    if format!("{:?}", before.library()) != format!("{:?}", after.library()) {
        return Err("A browsing arrow changed the Library.".into());
    }
    if before.taxonomies.expanded != after.taxonomies.expanded
        || before.taxonomies.cursor_path != after.taxonomies.cursor_path
    {
        return Err("A browsing arrow changed the tree.".into());
    }
    let focus = if key.code == KeyCode::Left {
        HomeFocus::Menu
    } else {
        HomeFocus::Content
    };
    if after.home_focus != focus {
        return Err("A browsing arrow moved focus in the wrong direction.".into());
    }
    Ok(())
}

fn stored_names_never_change(
    _: &Snapshot,
    _: &Event,
    after: Option<&Snapshot>,
) -> Result<(), String> {
    let Some(library) = after.and_then(Snapshot::library) else {
        return Ok(());
    };
    for taxonomy in library.taxonomies() {
        if let Some(saved) = library.saved_taxonomy(taxonomy.id())
            && saved.name_locked()
            && taxonomy.name() != saved.id().as_str()
        {
            return Err("A finalized taxonomy name changed.".into());
        }
    }
    Ok(())
}

const TEXT_FIELDS: [DraftField; 4] = [
    DraftField::Name,
    DraftField::Description,
    DraftField::Condition,
    DraftField::Rationale,
];

/// Every custom node's text fields and their lengths in characters.
fn field_lengths(library: &LibraryDraft) -> HashMap<(NodeId, &'static str), usize> {
    let mut lengths = HashMap::new();
    for taxonomy in library
        .taxonomies()
        .iter()
        .filter(|taxonomy| taxonomy.origin() == TaxonomyOrigin::Custom)
    {
        let ids =
            std::iter::once(taxonomy.id()).chain(taxonomy.types().iter().flat_map(|commit_type| {
                std::iter::once(commit_type.id()).chain(
                    commit_type
                        .properties()
                        .iter()
                        .map(gitserious_app::PropertyNode::id),
                )
            }));
        for id in ids {
            for field in TEXT_FIELDS {
                if let Some(text) = library.text(id, field) {
                    lengths.insert((id.clone(), field.label()), text.graphemes(true).count());
                }
            }
        }
    }
    lengths
}

fn no_key_grows_a_field_past_its_limit(
    before: &Snapshot,
    _: &Event,
    after: Option<&Snapshot>,
) -> Result<(), String> {
    let (Some(old), Some(new)) = (before.library(), after.and_then(Snapshot::library)) else {
        return Ok(());
    };
    let old = field_lengths(old);
    for ((id, label), length) in field_lengths(new) {
        let limit = TEXT_FIELDS
            .iter()
            .find(|field| field.label() == label)
            .and_then(|field| field.limit())
            .unwrap_or(usize::MAX);
        if let Some(&previous) = old.get(&(id.clone(), label))
            && length > limit
            && length > previous
        {
            return Err(format!(
                "{label} of {id:?} grew from {previous} to {length} characters, past its limit \
                 of {limit}."
            ));
        }
    }
    Ok(())
}

/// The ids of every custom type and property.
fn children(library: &LibraryDraft) -> HashSet<NodeId> {
    library
        .taxonomies()
        .iter()
        .filter(|taxonomy| taxonomy.origin() == TaxonomyOrigin::Custom)
        .flat_map(gitserious_app::TaxonomyNode::types)
        .flat_map(|commit_type| {
            std::iter::once(commit_type.id().clone()).chain(
                commit_type
                    .properties()
                    .iter()
                    .map(|property| property.id().clone()),
            )
        })
        .collect()
}

fn cancelled_edit(before: &Snapshot, input: &Event, after: Option<&Snapshot>) -> bool {
    let Some(baseline) = &before.taxonomies.edit_baseline else {
        return false;
    };
    let Some(after) = after else {
        return false;
    };
    before.outline_takes_keys()
        && matches!(
            input,
            Event::Key(KeyEvent {
                code: KeyCode::Esc,
                ..
            })
        )
        && matches!(after.taxonomies.mode, Mode::Nav)
        && format!("{:?}", after.library()) == format!("{:?}", Some(&baseline.library))
        && after.taxonomies.cursor == baseline.cursor
        && after.taxonomies.expanded == baseline.expanded
        && after.taxonomies.outline_offset == baseline.outline_offset
        && after.taxonomies.inspector_offset == baseline.inspector_offset
        && format!("{:?}", after.saved) == format!("{:?}", before.saved)
}

fn cancellation_restores_without_writing(
    before: &Snapshot,
    input: &Event,
    after: Option<&Snapshot>,
) -> Result<(), String> {
    if before.outline_takes_keys()
        && before.taxonomies.edit_baseline.is_some()
        && matches!(
            input,
            Event::Key(KeyEvent {
                code: KeyCode::Esc,
                ..
            })
        )
        && !cancelled_edit(before, input, after)
    {
        return Err("Esc did not restore the edit checkpoint without writing storage.".into());
    }
    Ok(())
}

fn removals_go_through_their_dialog(
    before: &Snapshot,
    input: &Event,
    after: Option<&Snapshot>,
) -> Result<(), String> {
    let (Some(old), Some(new)) = (before.library(), after.and_then(Snapshot::library)) else {
        return Ok(());
    };
    let new = children(new);
    if children(old).is_subset(&new) {
        return Ok(());
    }
    if cancelled_edit(before, input, after) {
        return Ok(());
    }
    let confirmed = match before.screen {
        Screen::ConfirmRemove(_) => answer(REMOVE, input) == Some(Answer::Confirm),
        Screen::Home => false,
    };
    if confirmed {
        return Ok(());
    }
    Err("A type or property left the Library without a confirmed Remove dialog.".into())
}

fn the_footer_shows_the_dialogs_choices(
    _: &Snapshot,
    _: &Event,
    after: Option<&Snapshot>,
) -> Result<(), String> {
    let Some((after, hints)) = after.and_then(|after| Some((after, after.dialog_hints.as_ref()?)))
    else {
        return Ok(());
    };
    let footer = after.footer();
    hints
        .iter()
        .find(|hint| !footer.contains(hint.as_str()))
        .map_or(Ok(()), |hint| {
            Err(format!(
                "The footer {:?} does not show the dialog's {hint:?}.",
                footer.trim_end()
            ))
        })
}

fn saved_eligibility_badges(
    _: &Snapshot,
    _: &Event,
    after: Option<&Snapshot>,
) -> Result<(), String> {
    let Some(after) = after else {
        return Ok(());
    };
    if !matches!(after.screen, Screen::Home) || after.overlay.is_some() || after.too_small {
        return Ok(());
    }
    let Some(library) = after.library() else {
        return Ok(());
    };
    if after.category != Category::Taxonomies || after.dialog_hints.is_some() {
        return Ok(());
    }
    for obsolete in ["unapplied", "~ modified", "ctrl+s", "review changes"] {
        if after.frame.contains(obsolete) {
            return Err(format!("Library shows {obsolete:?}."));
        }
    }
    let rows = after.frame.lines().collect::<Vec<_>>();
    let area = after.taxonomies.outline_area;
    for (index, id) in after.taxonomies.visible.iter().enumerate() {
        let row = rows
            .get(usize::from(area.y) + index)
            .map(|row| {
                row.chars()
                    .skip(usize::from(area.x))
                    .take(usize::from(area.width))
                    .collect::<String>()
            })
            .unwrap_or_default();
        let built_in_root = id.as_ref().and_then(|id| library.node(id)).is_some_and(|node| {
            matches!(node, NodeRef::Taxonomy(taxonomy) if taxonomy.origin() == gitserious_app::TaxonomyOrigin::BuiltIn)
        });
        if row.split_whitespace().any(|word| word == "built-in") != built_in_root
            || row.contains("read-only")
        {
            return Err(format!(
                "The outline origin badge disagrees with its node: {row:?}"
            ));
        }
        let new_taxonomy = row.trim() == "+ new taxonomy";
        if row.contains("+ new") != new_taxonomy
            || (new_taxonomy && (id.is_some() || index + 1 != after.taxonomies.visible.len()))
        {
            return Err(format!(
                "Only the Library's last row creates a taxonomy: {row:?}"
            ));
        }
        let invalid = id.as_ref().is_some_and(|id| {
            matches!(library.node(id), Some(NodeRef::Taxonomy(_)))
                && library
                    .saved_taxonomy(id)
                    .is_some_and(|record| record.commit_types().is_empty())
        });
        if row.contains("▲ invalid") != invalid {
            return Err(format!("Saved eligibility disagrees with row {row:?}."));
        }
    }
    let new_rows = rows.iter().filter(|row| row.contains("+ new")).count();
    let shown = after
        .taxonomies
        .visible
        .iter()
        .enumerate()
        .filter(|(index, id)| {
            id.is_none()
                && rows
                    .get(usize::from(area.y) + index)
                    .is_some_and(|row| row.contains("+ new taxonomy"))
        })
        .count();
    if new_rows != shown {
        return Err("\"+ new\" shows outside the outline's new taxonomy row.".into());
    }
    let footer = after.footer();
    if [
        "▲ invalid",
        "+ new",
        "~ modified",
        "◇ read-only",
        "built-in",
    ]
    .iter()
    .any(|label| footer.contains(label))
    {
        return Err(format!("The footer duplicates node status: {footer:?}"));
    }
    let selected = library.find_path(
        &after
            .taxonomies
            .cursor_path
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
    );
    let built_in = selected
        .as_ref()
        .and_then(|id| library.node(id))
        .is_some_and(|node| node.taxonomy().origin() == gitserious_app::TaxonomyOrigin::BuiltIn);
    if after.frame.contains("◇ read-only") != built_in {
        return Err("The Inspector read-only badge disagrees with the selected node.".into());
    }
    if let Some(NodeRef::Property { taxonomy, .. }) =
        selected.as_ref().and_then(|id| library.node(id))
    {
        let actions = matches!(after.taxonomies.mode, taxonomies::ObservedMode::Nav)
            && taxonomy.origin() == gitserious_app::TaxonomyOrigin::Custom;
        for label in ["e edit", "c copy property", "d delete"] {
            if footer.contains(label) != actions {
                return Err(format!(
                    "Property NAV commands disagree with its mode: {footer:?}"
                ));
            }
        }
        let area = after.taxonomies.inspector_area;
        let inspector = rows
            .iter()
            .skip(usize::from(area.y))
            .take(usize::from(area.height))
            .map(|row| {
                row.chars()
                    .skip(usize::from(area.x))
                    .take(usize::from(area.width))
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        if inspector.trim().is_empty() {
            return Err("The Inspector shows nothing for the selected property.".into());
        }
        if inspector.contains("copy to other types") || inspector.contains("d delete") {
            return Err("Property actions remain in the Inspector.".into());
        }
    }
    Ok(())
}

fn problems_keep_the_saved_library(
    before: &Snapshot,
    _: &Event,
    after: Option<&Snapshot>,
) -> Result<(), String> {
    let Some(after) = after else {
        return Ok(());
    };
    let blocked = before
        .library()
        .is_some_and(|library| !library.problems().is_empty());
    if blocked && format!("{:?}", after.saved) != format!("{:?}", before.saved) {
        return Err("The saved Library changed while the staged Library had problems.".into());
    }
    Ok(())
}

fn property_contents(library: &LibraryDraft) -> HashMap<NodeId, String> {
    library
        .taxonomies()
        .iter()
        .flat_map(TaxonomyNode::types)
        .flat_map(TypeNode::properties)
        .map(|property| {
            let fields = (
                property.name(),
                property.description(),
                property.policy(),
                library.text(property.id(), DraftField::Condition),
                library.text(property.id(), DraftField::Rationale),
            );
            (property.id().clone(), format!("{fields:?}"))
        })
        .collect()
}

fn a_copy_and_its_source_change_apart(
    before: &Snapshot,
    _: &Event,
    after: Option<&Snapshot>,
) -> Result<(), String> {
    let (Some(old), Some(new)) = (before.library(), after.and_then(Snapshot::library)) else {
        return Ok(());
    };
    if let Mode::Edit { node, .. } = &before.taxonomies.mode
        && !matches!(
            before.library().and_then(|library| library.node(node)),
            Some(NodeRef::Property { .. })
        )
    {
        return Ok(());
    }
    if !matches!(before.screen, Screen::Home) {
        return Ok(());
    }
    let old = property_contents(old);
    let changed = property_contents(new)
        .into_iter()
        .filter(|(id, fields)| old.get(id).is_some_and(|before| before != fields))
        .map(|(id, _)| id)
        .collect::<Vec<_>>();
    if changed.len() > 1 {
        return Err(format!(
            "One key changed the fields of {} properties at once: {changed:?}.",
            changed.len()
        ));
    }
    Ok(())
}

/// Words from the application's vocabulary that the editor never shows: it applies changes and
/// calls the global configuration the Library.
const FOREIGN_WORDS: [&str; 3] = ["session", "save ", "global configuration"];

fn every_alert_is_a_whole_sentence(
    _: &Snapshot,
    _: &Event,
    after: Option<&Snapshot>,
) -> Result<(), String> {
    let Some(after) = after else {
        return Ok(());
    };
    if let Some(status) = after.status.as_deref() {
        let text = status.split_once(": ").map_or(status, |(_, text)| text);
        let problem = not_a_sentence(text).or_else(|| {
            text.split_whitespace()
                .find(|word| !after.frame.contains(word))
                .map(|word| format!("is cut off before {word:?}"))
        });
        if let Some(problem) = problem {
            return Err(format!("The alert {text:?} {problem}."));
        }
    }
    for text in field_problems(after) {
        let problem = not_a_sentence(&text).or_else(|| {
            (!shows_in_order(&after.frame, &format!("▲ {text}")))
                .then(|| "is not shown in full under its field".to_owned())
        });
        if let Some(problem) = problem {
            return Err(format!("The problem {text:?} {problem}."));
        }
    }
    Ok(())
}

/// Why `text` does not read as one of the editor's sentences, if it does not.
fn not_a_sentence(text: &str) -> Option<String> {
    let lowered = text.to_lowercase();
    if !text.chars().next().is_some_and(char::is_uppercase) {
        Some("does not start with a capital letter".to_owned())
    } else if !text.ends_with(['.', '?', '!']) {
        Some("does not end a sentence".to_owned())
    } else {
        FOREIGN_WORDS
            .iter()
            .find(|word| lowered.contains(*word))
            .map(|word| format!("uses {:?}", word.trim()))
    }
}

/// The sentences of the problems on the field EDIT sits on, while the outline shows.
fn field_problems(snapshot: &Snapshot) -> Vec<String> {
    let (Some(library), Mode::Edit { node, field, .. }) =
        (snapshot.library(), &snapshot.taxonomies.mode)
    else {
        return Vec::new();
    };
    if !snapshot.outline_takes_keys() || snapshot.too_small {
        return Vec::new();
    }
    let site = ProblemSite::Field(node.clone(), *field);
    library
        .field_problems(node)
        .iter()
        .filter(|problem| problem.site == site)
        .map(|problem| problem_sentence(library, problem))
        .collect()
}

/// Whether the words of `text` follow one another in the frame right of the sidebar, read row by
/// row with borders and the focus gutter dropped.
fn shows_in_order(frame: &str, text: &str) -> bool {
    let words = frame
        .lines()
        .flat_map(|row| {
            row.chars()
                .skip(usize::from(RAIL_WIDTH))
                .map(|character| match character {
                    '│' | '▌' => ' ',
                    other => other,
                })
                .collect::<String>()
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let wanted = text.split_whitespace().collect::<Vec<_>>();
    words.windows(wanted.len()).any(|window| {
        window
            .iter()
            .zip(&wanted)
            .all(|(shown, word)| shown == word)
    })
}

fn navigation_is_persisted(
    _: &Snapshot,
    _: &Event,
    after: Option<&Snapshot>,
) -> Result<(), String> {
    let Some(after) = after else {
        return Ok(());
    };
    if !matches!(after.taxonomies.mode, Mode::Edit { .. })
        && let Some(candidate) = after.staged_library()
        && Some(candidate) != after.stored_library()
    {
        return Err("A completed Library interaction was not persisted.".into());
    }
    Ok(())
}

/// The policy of the property the outline has selected.
fn selected_policy(snapshot: &Snapshot) -> Option<String> {
    let library = snapshot.library()?;
    let node = library.find_path(&snapshot.taxonomies.cursor_path)?;
    match library.node(&node)? {
        NodeRef::Property { property, .. } => Some(property.policy().label().to_owned()),
        _ => None,
    }
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

/// Judges the last of `keys`, of which the first `seed_keys` reached the seed's start.
fn check(
    seed: &Seed,
    seed_keys: usize,
    keys: &[usize],
    before: &Snapshot,
    after: &Result<Option<Snapshot>, String>,
) -> Result<(), String> {
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

/// Presses `label` after `keys`, checking the transition, and moves `state` to the result.
fn advance(
    seed: &Seed,
    keys: &mut Vec<usize>,
    state: &mut Snapshot,
    label: &str,
) -> Result<(), String> {
    keys.extend(input_indices(seed, &[label])?);
    let after = replay(seed, keys);
    check(seed, keys.len() - 1, keys, state, &after)?;
    *state = after?.ok_or_else(|| format!("seed \"{}\" quit the editor", seed.name))?;
    Ok(())
}

/// Finds and checks the keys to the seed's start, and returns them with the state they reach.
fn start(seed: &Seed) -> Result<(Vec<usize>, Snapshot), String> {
    let mut keys = Vec::new();
    let mut state = replay(seed, &[])?.ok_or("the editor quit before any key")?;
    let ready = match seed.start {
        Start::Keys {
            keys: labels,
            ready,
        } => {
            for label in labels {
                advance(seed, &mut keys, &mut state, label)?;
            }
            ready(&state)
        }
        Start::Outline { path, mode } => {
            advance(seed, &mut keys, &mut state, "2")?;
            for depth in 1..=path.len() {
                if depth > 1 {
                    advance(seed, &mut keys, &mut state, "enter")?;
                }
                for _ in 0..64 {
                    let names = state.taxonomies.cursor_path.iter().map(String::as_str);
                    if names.take(depth).eq(path[..depth].iter().copied()) {
                        break;
                    }
                    advance(seed, &mut keys, &mut state, "down")?;
                }
            }
            if let StartMode::Edit(field) = mode {
                advance(seed, &mut keys, &mut state, "e")?;
                for _ in 0..8 {
                    if editing(&state) == Some(field) {
                        break;
                    }
                    advance(seed, &mut keys, &mut state, "enter")?;
                }
            }
            let reached = match mode {
                StartMode::Nav => editing(&state).is_none(),
                StartMode::Edit(field) => editing(&state) == Some(field),
            };
            at_outline(&state) && selects(&state, path) && reached
        }
    };
    if !ready {
        return Err(format!(
            "seed \"{}\" does not reach its start state; it reached {:?}",
            seed.name, state.screen
        ));
    }
    Ok((keys, state))
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
    let alphabet = match seed.alphabet {
        Alphabet::Every => (0..INPUTS.len())
            .filter(|&input| !NAMED_ONLY.contains(&INPUTS[input].0))
            .collect(),
        Alphabet::Only(labels) => input_indices(seed, labels)?,
    };
    let (prefix, before) = start(seed)?;
    let seed_keys = prefix.len();
    let start_keys = labels(&prefix);
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
            check(seed, seed_keys, &keys, &frontier[parent].1, &after)?;
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
                start_keys,
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
