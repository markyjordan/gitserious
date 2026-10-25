//! Bounded exhaustive check of the configuration editor's reachable states.
//!
//! The check drives the real [`State`] through every sequence of keys from [`INPUTS`], up to each
//! seed's depth bound, and checks every entry of [`PROPERTIES`] on every transition, the seed's
//! own keys included. A state is rebuilt by replaying its keys into a fresh fake workspace and
//! rendering into a [`TestBackend`] after every event, as the terminal loop does. Two states merge
//! only when their [`Snapshot`]s print the same, and properties read nothing but snapshots, so a
//! merge never hides a difference that a property can see. A key that quits the editor ends its
//! branch, and the properties judge it as a transition that drops every draft. Exploration is
//! breadth first, so a counterexample is a shortest one for its seed.
//!
//! To add a property, write a [`Check`], add it to [`PROPERTIES`], and add to [`Snapshot`] any
//! field it reads that the snapshot does not hold yet.
//!
//! P1, no silent draft loss. A taxonomy draft, type editor, property editor, or taxonomy metadata
//! form with unsaved changes closes only after the user confirms the discard or after its changes
//! reach the parent draft or the staged Library. Without it, a user loses typed work without a
//! warning.
//!
//! Scope limits:
//! - Only the keyboard events in [`INPUTS`]. Mouse, paste, and resize events are out of scope.
//! - One terminal of `MINIMUM_WIDTH` by `MINIMUM_HEIGHT` cells. Mouse hit areas that rendering
//!   records are not projected.
//! - The property editor's policy, multiplicity, and scroll position and the child editors'
//!   initial drafts are private to `child_editor`. The snapshot sees them only through the rendered
//!   frame and each editor's `is_dirty`.
//! - `State` runs standalone, not inside the unified shell, so `ConfigView::request_leave_editor`
//!   is not driven.
//! - Workspace saves always succeed, and the project starts uninitialized.
//! - A pass covers the four seeds to their depth bounds, not every reachable state. Every replay
//!   renders each frame in a debug build, so the bounds keep the check near 45 seconds on six
//!   cores. The editor seeds get the deeper bound because P1 lives in the editors.

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
}

const SEEDS: &[Seed] = &[
    Seed {
        name: "built-ins at Library",
        workspace: built_ins,
        keys: &["down", "right"],
        ready: at_taxonomy_list,
        depth: 2,
    },
    Seed {
        name: "team at Library",
        workspace: team_library,
        keys: &["down", "right", "down", "down", "down"],
        ready: team_selected,
        depth: 2,
    },
    Seed {
        name: "built-ins with the create editor open",
        workspace: built_ins,
        keys: &["down", "right", "n"],
        ready: creating,
        depth: 3,
    },
    Seed {
        name: "team with the edit editor open",
        workspace: team_library,
        keys: &["down", "right", "down", "down", "down", "e"],
        ready: editing,
        depth: 3,
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
    matches!(snapshot.screen, Screen::TaxonomyEditor(EditorMode::Create))
}

fn editing(snapshot: &Snapshot) -> bool {
    matches!(snapshot.screen, Screen::TaxonomyEditor(EditorMode::Edit))
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
    form_scroll: u16,
    form_viewport_height: u16,
    status: Option<String>,
    editor_mode: EditorMode,
    draft: Option<TaxonomyDraft>,
    initial_draft: Option<TaxonomyDraft>,
    draft_dirty: bool,
    create_text: Option<[Text; 2]>,
    fork_source: Option<String>,
    form: Option<FormState>,
    form_dirty: bool,
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
            form_scroll: state.form_scroll,
            form_viewport_height: state.form_viewport_height,
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
            form: state.form.clone(),
            form_dirty: state.form.as_ref().zip(state.editor.as_ref()).is_some_and(
                |(form, draft)| form.fields != [draft.id.as_str(), draft.description.as_str()],
            ),
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

const PROPERTIES: &[(&str, Check)] = &[("P1, no silent draft loss", no_silent_draft_loss)];

struct ChildEditor {
    name: &'static str,
    dirty: fn(&Snapshot) -> Option<bool>,
    parent: fn(&Snapshot) -> String,
}

const CHILD_EDITORS: [ChildEditor; 3] = [
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
    ChildEditor {
        name: "taxonomy metadata form",
        dirty: |snapshot| snapshot.form.as_ref().map(|_| snapshot.form_dirty),
        parent: |snapshot| format!("{:?}", snapshot.draft),
    },
];

fn no_silent_draft_loss(
    before: &Snapshot,
    input: &Event,
    after: Option<&Snapshot>,
) -> Result<(), String> {
    let chose_discard =
        DiscardDialog::default().handle_event(input) == Some(DiscardChoice::Discard);
    if before.draft_dirty
        && after.is_none_or(|after| after.draft.is_none())
        && !(matches!(before.screen, Screen::ConfirmDiscardCreate(_)) && chose_discard)
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

fn explore(seed: &Seed) -> Result<Coverage, String> {
    let started = Instant::now();
    let prefix = seed
        .keys
        .iter()
        .map(|label| {
            INPUTS
                .iter()
                .position(|(input, _)| input == label)
                .ok_or_else(|| format!("seed \"{}\" names unknown key {label}", seed.name))
        })
        .collect::<Result<Vec<_>, _>>()?;
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
    let mut frontier = vec![(prefix, before)];
    let mut transitions = 0;
    for level in 1..=seed.depth {
        let mut next = Vec::new();
        let edges = (0..frontier.len())
            .flat_map(|parent| (0..INPUTS.len()).map(move |input| (parent, input)))
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
                && level < seed.depth
            {
                next.push((keys, after));
            }
        }
        frontier = next;
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
