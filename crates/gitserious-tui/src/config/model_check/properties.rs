//! The promises every transition keeps, one named function per entry in `PROPERTIES`.

use std::collections::{HashMap, HashSet};

use gitserious_app::{
    DraftField, LibraryDraft, NodeId, NodeRef, ProblemSite, TaxonomyNode, TaxonomyOrigin, TypeNode,
};
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use unicode_segmentation::UnicodeSegmentation;

use super::snapshot::Snapshot;
use crate::config::library_lookup::LibraryLookup;
use crate::config::naming::problem_sentence;
use crate::config::taxonomies::{self, ObservedMode as Mode};
use crate::config::{Category, HomeFocus, RAIL_WIDTH, Screen};
use crate::shared::components::confirmation_dialog::{self, Answer, ConfirmationDialog, REMOVE};

/// Judges one transition. `after` is `None` when the input quit the editor.
type Check = fn(&Snapshot, &Event, Option<&Snapshot>) -> Result<(), String>;

pub(super) const PROPERTIES: &[(&str, Check)] = &[
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

fn answer(choices: &[confirmation_dialog::Choice], input: &Event) -> Option<Answer> {
    ConfirmationDialog::default().handle_event(choices, input)
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
