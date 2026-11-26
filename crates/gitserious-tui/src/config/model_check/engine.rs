//! The key alphabet and the breadth-first search that replays every key sequence from a seed.

use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use super::diagnostics::{check, check_reaches, labels};
use super::seeds::{Alphabet, Seed, Start, StartMode};
use super::snapshot::{Snapshot, at_outline, editing, selects};
use crate::config::State;
use crate::theme::{MINIMUM_HEIGHT, MINIMUM_WIDTH};

const fn press(code: KeyCode, modifiers: KeyModifiers) -> Event {
    Event::Key(KeyEvent::new(code, modifiers))
}

const fn plain(code: KeyCode) -> Event {
    press(code, KeyModifiers::NONE)
}

pub(super) const INPUTS: &[(&str, Event)] = &[
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

pub(super) struct Coverage {
    pub(super) states: usize,
    pub(super) transitions: usize,
    pub(super) elapsed: Duration,
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

pub(super) fn explore(seed: &Seed) -> Result<Coverage, String> {
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
    check_reaches(seed, &observed, &start_keys, &alphabet)?;
    Ok(Coverage {
        states: visited.len(),
        transitions,
        elapsed: started.elapsed(),
    })
}
