//! Bounded exhaustive check of the configuration editor's reachable states.
//!
//! The check drives the real [`State`] through every sequence of keys from each seed's
//! [`Alphabet`], up to the seed's depth bound, and checks every entry of [`PROPERTIES`] on every
//! transition, the keys that reach the seed's [`Start`] included. An outline start names the row
//! to start on by its names, and the check finds the keys: `2`, then `down` until the selected
//! row matches each name and `right` to expand it before the next, then `e` and `enter` until
//! EDIT sits on the seed's field. Other seeds start from literal keys and a readiness predicate.
//! A seed explores every key in [`INPUTS`] unless it names a smaller alphabet, which lets it
//! reach deeper for the same number of replays. After a seed's exploration, each of its
//! [`Reach`] goals must have seen every expected value in some explored state. A state is rebuilt
//! by replaying its keys into a fresh fake workspace and rendering into a [`TestBackend`] after
//! every event, as the terminal loop does. Two states merge only when their [`Snapshot`]s print
//! the same, and properties read nothing but snapshots, so a merge never hides a difference that
//! a property can see. A key that quits the editor ends its branch, and the properties judge it
//! as a transition that drops every draft. Exploration is breadth first, so a counterexample is a
//! shortest one for its seed.
//!
//! To add a property of every transition, write a [`Check`] and add it to [`PROPERTIES`]. To add a
//! property of what a seed can reach, add a [`Reach`] to that seed. Add to [`Snapshot`] any field
//! either one reads that the snapshot does not hold yet.
//!
//! P1, the unapplied draft is dropped only by apply or a confirmed discard. While the Library
//! has unapplied changes, a key clears them or quits only when it answers the Quit or Unapplied
//! Changes dialog, applies from the review, confirms a Remove dialog, or is an edit key that
//! returns the draft to what is stored (a text key in EDIT, ←/→ on a policy, or alt+↑/↓ in NAV).
//! Without it, esc, a tab, or a stray key throws away typed work without a warning.
//!
//! P2, a NAV/EDIT round trip with no edit leaves the session clean. From Taxonomies with no
//! unapplied changes and no overlay, only a key that creates or edits a draft (`n`, `f`, enter on
//! an add row, ←/→ on a policy, alt+↑/↓ in NAV, or a text key in EDIT) leaves unapplied changes. Without it,
//! entering EDIT, moving between fields, or esc advances a taxonomy's version and rewrites the
//! user's configuration on apply.
//!
//! P3, the review names every taxonomy change. While the Global review shows a staged Library
//! that builds, every custom taxonomy whose content differs between the saved and the staged
//! Library appears in the frame as `created`, `edited`, or `deleted` followed by its id. A staged
//! Library with problems cannot apply (P11), so its review is not judged here. Without it, a user
//! applies a change that the review did not show.
//!
//! P4, the Inspector's Policy reaches every policy. From a custom property in NAV, the arrow keys
//! and enter alone set the property to every policy (required, recommended, conditional,
//! optional). Without it, a property needs a hand edit of `config.toml` to take a policy the
//! editor cannot set.
//!
//! P5, a stored taxonomy's name never changes. Every custom taxonomy the configuration stores
//! keeps the name it was loaded with. Without it, a user could rename a taxonomy that projects pin
//! by name, and apply would address the wrong taxonomy.
//!
//! P6, no key grows a field past its own limit. Name and Condition hold at most 80 characters and
//! Description and Rationale at most 100, on every taxonomy, type, and property. No key leaves a
//! field of a custom node that existed before the key longer than its limit and longer than
//! before. Without it, a user can type text that misshapes the Inspector's rows.
//!
//! P7, every alert is a whole sentence. Every notice a reachable state shows, and every problem
//! under the field EDIT sits on, starts with a capital letter, ends a sentence, and avoids the
//! application's own words (`session`, `save`, `global configuration`). A notice appears in the
//! frame in full, and a field's problem appears right of the sidebar as `▲` and its words in
//! order, so the Inspector scrolls it into view with its field. Without it, raw domain errors,
//! clipped toasts, and problems scrolled out of sight reach the screen.
//!
//! P8, removals go through their dialog. A type or property leaves the Library only when the user
//! answers y on its Remove or Delete dialog, or discards on the Unapplied Changes dialog. Without
//! it, one stray d drops a type with all of its properties.
//!
//! P9, the footer shows the dialog's choices. While any confirmation is open, the footer row shows
//! each of its choices as `key: label`, the same pairs as the dialog's buttons. Without it, the
//! footer and the dialog disagree about which key does what.
//!
//! P10, unapplied changes and problems agree everywhere they show. After every key, the count the
//! Config tab badge shows equals the Library's `ChangeSet::count()`; on Home the sidebar reads `●
//! N unapplied` with that count, or no unapplied line at zero; every outline row shows `▲ invalid
//! N` exactly when N problems sit on its node, or on its node or beneath it while it is collapsed,
//! and otherwise `+ new` or `~ modified` exactly when its node carries that mark; and the footer
//! shows `▲ invalid N` for the selected node the same way. Without it, the tab, the sidebar, the
//! rows, and the footer disagree about what apply would write, a problem hides under a collapsed
//! row, or an expanded parent repeats the count its visible rows already show.
//!
//! P11, the saved Library never changes while problems exist. No key changes the Library the
//! workspace stores while the staged Library has a problem. Without it, enter on the review or
//! apply on the Unapplied Changes dialog writes a Library that does not build, and the fake
//! workspace, which saves whatever it is given, would show it.
//!
//! Scope limits:
//! - Only the keyboard events in [`INPUTS`]. Mouse, paste, and resize events are out of scope.
//! - One terminal of `MINIMUM_WIDTH` by `MINIMUM_HEIGHT` cells. Mouse hit areas that rendering
//!   records are not projected.
//! - `State` runs standalone, not inside the unified shell, so the shell's own quit dialog and its
//!   tab keys are not driven. P10 reads the count the shell's badge shows from the same method
//!   the shell calls; a shell test checks that the header draws it.
//! - Workspace saves always succeed, and the project starts uninitialized. The real workspace also
//!   refuses a Library with problems; an app test covers that, and P11 covers the editor's own
//!   gate.
//! - P3 reads the frame, so it sees only the review rows that fit the viewport. Every seed's review
//!   fits without scrolling.
//! - P4 explores only the up, down, left, right, and enter keys from one custom property. It reads
//!   the selected property's policy in the Library, not the frame.
//! - P6 checks fields of custom nodes that existed before the key; a fork copies its source's
//!   text as it is.
//! - The Taxonomies outline is seen through `taxonomies::Observation`: the raw cursor and expanded
//!   node ids, the selected row's names, the scroll offsets, and the mode with its field and text
//!   cursor. The Library is seen through the Global session, so a draft edit changes the snapshot.
//! - A pass covers the seeds to their depth bounds, not every reachable state. Every replay
//!   renders each frame in a debug build, so the bounds keep the check near a minute on six
//!   cores. A new taxonomy and the Description at its limit get the deeper bound because typing
//!   and the limit live there. The P4 seed's bound is deep enough to expand every state its five
//!   keys reach.

use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use gitserious_app::{DraftField, GlobalConfiguration, Mark, NodeRef, ProblemSite};
use gitserious_core::{
    CommitTypeDefinition, CommitTypeId, Description, PropertyDefinition, PropertyKey,
    PropertyRequirement, SchemaVersion, TaxonomyVersion,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use unicode_segmentation::UnicodeSegmentation;

use super::naming::problem_sentence;
use super::taxonomies::{Mode, Observation};
use super::tests::Workspace;
use super::*;
use crate::confirmation::{self, Answer, ConfirmDialog, DISCARD_AND_QUIT, REMOVE};

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
    ("1", plain(KeyCode::Char('1'))),
    ("2", plain(KeyCode::Char('2'))),
    ("space", plain(KeyCode::Char(' '))),
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
            ready: |snapshot| at_outline(snapshot) && snapshot.unapplied,
        },
        depth: 2,
        alphabet: Alphabet::Every,
        reaches: &[],
    },
    Seed {
        name: "a fork created and team deleted at Taxonomies",
        workspace: team_library,
        start: Start::Keys {
            keys: &["2", "f", "esc", "up", "d", "y"],
            ready: |snapshot| at_outline(snapshot) && snapshot.unapplied,
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
                "2", "down", "down", "down", "right", "right", "right", "down", "down", "down",
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
                "2", "down", "down", "down", "right", "right", "right", "down", "right", "right",
                "e", "enter", "enter", "enter",
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
        alphabet: Alphabet::Only(&["up", "down", "left", "right", "enter"]),
        reaches: &[Reach {
            name: "P4, the Inspector's Policy reaches every policy",
            observe: selected_policy,
            expected: &["required", "recommended", "conditional", "optional"],
        }],
    },
];

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

fn team_library() -> Result<Workspace, String> {
    let workspace = Workspace::new();
    *workspace.global.borrow_mut() =
        ConfigurationSession::open_global(team_configuration().map_err(|error| error.to_string())?);
    Ok(workspace)
}

/// `team` with a Description at its limit and one type with two properties, so a property can
/// move.
fn team_configuration() -> Result<GlobalConfiguration, Box<dyn std::error::Error>> {
    let description = format!("Team taxonomy{}", ".".repeat(87));
    let team = Taxonomy::new(
        TaxonomyId::new("team")?,
        TaxonomyVersion::new(1)?,
        Description::new(description)?,
        None,
        vec![CommitTypeDefinition::new(
            SchemaVersion::new(1)?,
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

/// Everything a user can observe or that later input can affect, as a property sees it.
#[derive(Debug)]
#[expect(dead_code, reason = "the fingerprint reads these fields through Debug")]
struct Snapshot {
    screen: Screen,
    category: Category,
    home_focus: HomeFocus,
    project_section: ProjectSection,
    active_scope: ConfigurationDestination,
    taxonomies: Observation,
    overlay: Option<String>,
    scroll: u16,
    project_scroll: u16,
    status: Option<String>,
    settings: Option<SettingsDraft>,
    global: Option<ConfigurationSession>,
    project: Option<ConfigurationSession>,
    /// Whether the Library has changes apply would write.
    unapplied: bool,
    /// The count of unapplied changes the shell's Config tab shows.
    pending: usize,
    load_errors: [String; 2],
    saved: [ConfigurationSession; 2],
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
            active_scope: state.active_scope,
            taxonomies: state.taxonomies.observe(state.library()),
            overlay: state
                .home_overlay
                .as_ref()
                .map(|overlay| format!("{overlay:?}")),
            scroll: state.scroll,
            project_scroll: state.project_scroll,
            status: state.status.as_ref().map(|notice| match notice {
                Notice::Error(message) => format!("error: {message}"),
                Notice::Info(message) => format!("info: {message}"),
            }),
            settings: state.settings.clone(),
            global: state.global.clone(),
            project: state.project.clone(),
            unapplied: state.scope_dirty(ConfigurationDestination::Global),
            pending: state.pending_count(),
            load_errors: [state.global_error.clone(), state.project_error.clone()],
            saved: [
                workspace.global.borrow().clone(),
                workspace.project.borrow().clone(),
            ],
            dialog: format!("{:?}", state.dialog),
            dialog_hints: state.confirmation().map(|question| {
                confirmation::hints(question.choices)
                    .into_iter()
                    .map(|(hint, label)| format!("{hint}: {label}"))
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
        self.global.as_ref().and_then(ConfigurationSession::library)
    }

    fn staged_library(&self) -> Option<GlobalConfiguration> {
        self.library().and_then(|library| library.build().ok())
    }

    /// The Library the fake workspace stores: the last applied session's Library.
    fn stored_library(&self) -> Option<GlobalConfiguration> {
        self.saved[0]
            .library()
            .and_then(|library| library.build().ok())
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
        "P1, the unapplied draft is dropped only by apply or a confirmed discard",
        no_silent_draft_loss,
    ),
    (
        "P2, a NAV/EDIT round trip with no edit leaves the session clean",
        round_trips_leave_the_session_clean,
    ),
    (
        "P3, the review names every taxonomy change",
        review_names_every_taxonomy_change,
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
        "P10, unapplied changes and problems agree everywhere they show",
        unapplied_changes_agree_everywhere,
    ),
    (
        "P11, the saved Library never changes while problems exist",
        problems_keep_the_saved_library,
    ),
];

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
        KeyCode::Left | KeyCode::Right => true,
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
        Screen::Leave => answer(DISCARD_AND_QUIT, input) == Some(Answer::Confirm),
        Screen::ScopeChange(_) => matches!(
            answer(APPLY_OR_DISCARD, input),
            Some(Answer::Confirm | Answer::Alternate)
        ),
        Screen::Review(_) => {
            after.is_some_and(|after| format!("{:?}", after.saved) != format!("{:?}", before.saved))
        }
        Screen::ConfirmRemove(_) => answer(REMOVE, input) == Some(Answer::Confirm),
        Screen::Home => edits(before, input),
        Screen::Settings(_) => false,
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
    let creates = matches!(before.taxonomies.mode, Mode::Nav)
        && matches!(
            input,
            Event::Key(KeyEvent {
                code: KeyCode::Char('n' | 'f') | KeyCode::Enter,
                ..
            })
        );
    if creates || edits(before, input) {
        return Ok(());
    }
    Err("A key that edits nothing left the Library with unapplied changes.".into())
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
        if let NodeId::Loaded(path) = taxonomy.id()
            && taxonomy.origin() == TaxonomyOrigin::Custom
            && path.names().first().map(String::as_str) != Some(taxonomy.name())
        {
            return Err(format!(
                "The stored taxonomy {:?} now reads {:?}.",
                path.names(),
                taxonomy.name()
            ));
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
    let confirmed = match before.screen {
        Screen::ConfirmRemove(_) => answer(REMOVE, input) == Some(Answer::Confirm),
        Screen::ScopeChange(_) => matches!(
            answer(APPLY_OR_DISCARD, input),
            Some(Answer::Confirm | Answer::Alternate)
        ),
        _ => false,
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
    let footer = after.frame.lines().last().unwrap_or_default();
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

fn unapplied_changes_agree_everywhere(
    _: &Snapshot,
    _: &Event,
    after: Option<&Snapshot>,
) -> Result<(), String> {
    let Some(after) = after else {
        return Ok(());
    };
    let changes = after
        .library()
        .map(LibraryDraft::changes)
        .unwrap_or_default();
    let count = changes.count();
    if after.pending != count {
        return Err(format!(
            "The Config tab counts {} unapplied changes and the Library {count}.",
            after.pending
        ));
    }
    if !matches!(after.screen, Screen::Home) || after.overlay.is_some() || after.too_small {
        return Ok(());
    }
    let rows = after.frame.lines().collect::<Vec<_>>();
    let sidebar = rows
        .iter()
        .map(|row| {
            row.chars()
                .take(usize::from(MENU_WIDTH))
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let line = format!("● {count} unapplied");
    if (count > 0 && !sidebar.contains(&line)) || (count == 0 && sidebar.contains("unapplied")) {
        return Err(format!("The sidebar does not read {line:?}:\n{sidebar}"));
    }
    let Some(library) = after.library() else {
        return Ok(());
    };
    if after.category != Category::Taxonomies {
        return Ok(());
    }
    let problems = library.problems();
    let invalid = |id: &NodeId| {
        let collapsed = !after.taxonomies.expanded.contains(id);
        problems
            .iter()
            .filter(|problem| {
                std::iter::successors(problem.node().cloned(), |node| library.parent(node))
                    .take(if collapsed { usize::MAX } else { 1 })
                    .any(|node| &node == id)
            })
            .count()
    };
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
        let count = id.as_ref().map_or(0, invalid);
        if row.contains("▲ invalid") != (count > 0)
            || (count > 0 && !row.contains(&format!("▲ invalid {count}")))
        {
            return Err(format!(
                "The outline row {:?} does not show the {count} problems its node counts.",
                row.trim_end()
            ));
        }
        let mark = id
            .as_ref()
            .and_then(|id| changes.mark(id))
            .filter(|_| count == 0);
        for (chip, shown) in [("+ new", Mark::New), ("~ modified", Mark::Modified)] {
            if row.contains(chip) != (mark == Some(shown)) {
                return Err(format!(
                    "The outline row {:?} and its node's mark {mark:?} disagree about {chip:?}.",
                    row.trim_end()
                ));
            }
        }
    }
    let footer = rows.last().copied().unwrap_or_default();
    if let Some(count) = library
        .find_path(&after.taxonomies.cursor_path)
        .map(|id| invalid(&id))
        && (footer.contains("▲ invalid") != (count > 0)
            || (count > 0 && !footer.contains(&format!("▲ invalid {count}"))))
    {
        return Err(format!(
            "The footer {:?} does not show the {count} problems the selected node counts.",
            footer.trim_end()
        ));
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
    if blocked && format!("{:?}", after.saved[0]) != format!("{:?}", before.saved[0]) {
        return Err("The saved Library changed while the staged Library had problems.".into());
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
        .problems()
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
                .skip(usize::from(MENU_WIDTH))
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
    let Some(staged) = after.staged_library() else {
        return Ok(());
    };
    let staged = staged.custom_taxonomies();
    let saved = after.stored_library().unwrap_or_default();
    let saved = saved.custom_taxonomies();
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
                    advance(seed, &mut keys, &mut state, "right")?;
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
        Alphabet::Every => (0..INPUTS.len()).collect(),
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
