//! The Taxonomies section: one outline of Taxonomy › Type › Property above an Inspector that
//! describes the selected row and edits it in place. The view keeps only the cursor, the
//! expanded nodes, scroll offsets, and NAV, EDIT with a text cursor, or the copy picker with its
//! choices; every edit goes straight to the Library draft, and rows and field text come from it
//! on every render and key.

use std::collections::BTreeSet;
use std::ops::Range;
use std::time::Duration;

use gitserious_app::{
    DraftEdit, DraftField, EditRefusal, LibraryDraft, NodeId, NodeRef, Policy, Problem,
    ProblemSite, TaxonomyOrigin,
};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_segmentation::UnicodeSegmentation;

use super::naming::{problem_sentence, refusal_sentence, single_line};
use super::{ShellKeys, contains};
use crate::theme::{
    ACCENT, AMBER, JET_BLACK, footer_hints, footer_key_style, footer_label_style,
    navigation_key_style,
};
use outline::Row;
pub(super) use outline::RowKey;

mod inspector;
#[cfg(test)]
mod layout_tests;
mod outline;
mod panes;
mod picker;
mod summary_scroll;
mod text_layout;

const DIM: Style = Style::new().fg(Color::Gray);
const KEY_STYLE: Style = Style::new().fg(ACCENT).add_modifier(Modifier::BOLD);
const READ_ONLY_STYLE: Style = Style::new().fg(Color::Black).bg(Color::Rgb(211, 211, 211));
const CUSTOM_STYLE: Style = Style::new().fg(Color::Black).bg(Color::Rgb(144, 238, 144));
pub(super) const BAND: Style = Style::new().bg(Color::Rgb(40, 40, 40));
const EDIT_CHIP: Style = Style::new()
    .fg(JET_BLACK)
    .bg(AMBER)
    .add_modifier(Modifier::BOLD);

/// A node's change or problem status in the outline and Inspector.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Chip {
    Invalid,
}

/// Saved eligibility badges and current inline field errors are independent.
pub(super) struct Chips<'a> {
    library: &'a LibraryDraft,
    problems: Vec<Problem>,
}

impl<'a> Chips<'a> {
    fn new(library: &'a LibraryDraft, _expanded: &'a BTreeSet<NodeId>) -> Self {
        Self {
            library,
            problems: library.problems(),
        }
    }

    fn of(&self, id: &NodeId) -> Option<Chip> {
        (matches!(self.library.node(id), Some(NodeRef::Taxonomy(_)))
            && self
                .library
                .saved_taxonomy(id)
                .is_some_and(|record| record.commit_types().is_empty()))
        .then_some(Chip::Invalid)
    }

    fn sentences(&self, site: &ProblemSite) -> Vec<String> {
        let errors = match site {
            ProblemSite::Field(node, DraftField::Name)
                if self.problems.iter().any(|problem| {
                    matches!(problem.kind, gitserious_app::ProblemKind::Taken(_))
                }) =>
            {
                self.library.field_problems(node)
            }
            _ => self.problems.clone(),
        };
        errors
            .iter()
            .filter(|problem| &problem.site == site)
            .map(|problem| problem_sentence(self.library, problem))
            .collect()
    }
}

impl Chip {
    fn label(self) -> &'static str {
        "▲ invalid"
    }

    fn span(self) -> Span<'static> {
        Span::styled(
            format!(" {} ", self.label()),
            Style::new().fg(Color::Red).bg(Color::Rgb(56, 18, 18)),
        )
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct TaxonomiesView {
    /// `None` until the user picks a row; the first row shows as selected.
    cursor: Option<RowKey>,
    expanded: BTreeSet<NodeId>,
    mode: Mode,
    outline_offset: usize,
    inspector_offset: u16,
    outline_area: Rect,
    /// The Inspector's rows of text, without the row that counts the rows below; pgup and pgdn
    /// page by its height.
    inspector_area: Rect,
    summary_scroll: summary_scroll::SummaryScroll,
}

/// NAV moves through the outline. EDIT types into one field of the selected node; `cursor` is
/// a grapheme index into that field's text in the draft. `CopyTo` picks the types the selected
/// property is copied to; `cursor` indexes its `copy_targets`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) enum Mode {
    #[default]
    Nav,
    Edit {
        node: NodeId,
        field: DraftField,
        cursor: usize,
    },
    CopyTo {
        property: NodeId,
        cursor: usize,
        chosen: BTreeSet<NodeId>,
    },
}

/// A change to the Library a NAV key asks for. `State` runs it after its scope guard.
#[derive(Clone, Copy, Debug)]
pub(super) enum LibraryAction {
    Create,
    Fork,
    Edit,
    Add,
    Move { down: bool },
    Copy,
    CopyChosen,
}

/// What a NAV key did.
pub(super) enum Nav {
    Handled,
    /// Left on a top-level row hands focus to the sidebar.
    Sidebar,
    Act(LibraryAction),
    Unhandled,
}

impl TaxonomiesView {
    fn inspector_page(&self) -> u16 {
        self.inspector_area.height.max(1)
    }

    pub(super) fn select(&mut self, node: NodeId) {
        self.set_cursor(RowKey::Node(node));
    }

    fn set_cursor(&mut self, key: RowKey) {
        if self.cursor.as_ref() != Some(&key) {
            self.inspector_offset = 0;
            self.summary_scroll.reset();
        }
        self.cursor = Some(key);
    }

    pub(super) const fn editing(&self) -> bool {
        matches!(self.mode, Mode::Edit { .. })
    }

    pub(super) const fn navigating(&self) -> bool {
        matches!(self.mode, Mode::Nav)
    }

    pub(super) fn reset_animation(&mut self) {
        self.summary_scroll.reset();
    }

    pub(super) fn advance(&mut self, elapsed: Duration) {
        self.summary_scroll.advance(elapsed);
    }

    pub(super) fn next_frame_in(&self) -> Option<Duration> {
        self.summary_scroll.next_frame_in()
    }

    pub(super) const fn shell_keys(&self) -> ShellKeys {
        match self.mode {
            Mode::Nav => ShellKeys::Navigation,
            Mode::Edit { .. } | Mode::CopyTo { .. } => ShellKeys::None,
        }
    }

    pub(super) fn picker_hints(&self) -> Option<Vec<(&'static str, &'static str)>> {
        matches!(self.mode, Mode::CopyTo { .. }).then(|| {
            std::iter::once(("↑/↓", "move"))
                .chain(picker::CHOICES)
                .collect()
        })
    }

    fn selected_row<'a>(&self, library: &'a LibraryDraft) -> Option<Row<'a>> {
        let rows = outline::rows(library, &self.expanded);
        let index = outline::resolve(&rows, self.cursor.as_ref(), library);
        rows.get(index).copied()
    }

    /// The selected row's node, or for an add row the node it adds to.
    pub(super) fn selected<'a>(&self, library: &'a LibraryDraft) -> Option<NodeRef<'a>> {
        self.selected_row(library).and_then(Row::node)
    }

    /// NAV keys: move, expand, collapse, scroll the Inspector, and the keys that change the
    /// Library, which come back as an action. The open picker takes every key.
    pub(super) fn key(&mut self, key: KeyEvent, library: &LibraryDraft) -> Nav {
        if let Mode::CopyTo {
            property,
            cursor,
            chosen,
        } = &mut self.mode
        {
            let targets = library.copy_targets(property);
            match key.code {
                KeyCode::Esc => self.mode = Mode::Nav,
                KeyCode::Enter => return Nav::Act(LibraryAction::CopyChosen),
                KeyCode::Up => *cursor = cursor.saturating_sub(1),
                KeyCode::Down => *cursor = (*cursor + 1).min(targets.len().saturating_sub(1)),
                KeyCode::Char(' ') => {
                    if let Some(target) = targets.get(*cursor)
                        && !chosen.remove(target.id())
                    {
                        chosen.insert(target.id().clone());
                    }
                }
                _ => {}
            }
            return Nav::Handled;
        }
        let rows = outline::rows(library, &self.expanded);
        let index = outline::resolve(&rows, self.cursor.as_ref(), library);
        let Some(&row) = rows.get(index) else {
            return Nav::Unhandled;
        };
        let node = row.node();
        let open = outline::has_children(row)
            && node.is_some_and(|node| self.expanded.contains(node.id()));
        let custom = node.is_some_and(|node| node.taxonomy().origin() == TaxonomyOrigin::Custom);
        let plain = key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT;
        match key.code {
            KeyCode::Char('e') if plain => return Nav::Act(LibraryAction::Edit),
            KeyCode::Char('f') if plain => return Nav::Act(LibraryAction::Fork),
            KeyCode::Char('n') if plain => return Nav::Act(LibraryAction::Create),
            KeyCode::Char('c') if plain && matches!(row, Row::Node(NodeRef::Property { .. })) => {
                return Nav::Act(LibraryAction::Copy);
            }
            KeyCode::Up | KeyCode::Down
                if key.modifiers.contains(KeyModifiers::ALT)
                    && custom
                    && matches!(
                        row,
                        Row::Node(NodeRef::Type { .. } | NodeRef::Property { .. })
                    ) =>
            {
                return Nav::Act(LibraryAction::Move {
                    down: key.code == KeyCode::Down,
                });
            }
            KeyCode::Up => self.set_cursor(rows[index.saturating_sub(1)].key()),
            KeyCode::Down => self.set_cursor(rows[(index + 1).min(rows.len() - 1)].key()),
            KeyCode::Enter => match row {
                Row::NewTaxonomy => return Nav::Act(LibraryAction::Create),
                Row::AddType(_) | Row::AddProperty(..) => return Nav::Act(LibraryAction::Add),
                Row::Node(node) if outline::has_children(row) => {
                    self.summary_scroll.reset();
                    if open {
                        self.expanded.remove(node.id());
                    } else {
                        self.expand(node.id(), library);
                    }
                    self.set_cursor(row.key());
                }
                Row::Node(_) => return Nav::Unhandled,
            },
            KeyCode::Left => return Nav::Sidebar,
            KeyCode::Right => return Nav::Handled,
            KeyCode::PageUp => {
                self.inspector_offset = self.inspector_offset.saturating_sub(self.inspector_page());
            }
            KeyCode::PageDown => {
                self.inspector_offset = self.inspector_offset.saturating_add(self.inspector_page());
            }
            _ => return Nav::Unhandled,
        }
        Nav::Handled
    }

    /// Runs a NAV action on the selected row. Every action but a move or copy ends in
    /// EDIT on the field it opens.
    pub(super) fn act(
        &mut self,
        action: LibraryAction,
        library: &mut LibraryDraft,
    ) -> Result<(), String> {
        let refused = |refusal| refusal_sentence(refusal, None);
        let row = self
            .selected_row(library)
            .ok_or_else(|| refused(EditRefusal::Missing))?;
        // The new taxonomy row stands for no node, so only creating applies to it.
        let selected = || {
            row.node()
                .map(|node| node.id().clone())
                .ok_or_else(|| refused(EditRefusal::NotApplicable))
        };
        let (node, field) = match (action, row.key()) {
            (LibraryAction::Create, _) => (library.create_taxonomy(), DraftField::Name),
            (LibraryAction::Fork, _) => {
                let taxonomy = library
                    .node(&selected()?)
                    .map(|node| node.taxonomy().id().clone())
                    .ok_or_else(|| refused(EditRefusal::Missing))?;
                (library.fork(&taxonomy).map_err(refused)?, DraftField::Name)
            }
            (LibraryAction::Edit, _) => {
                let node = selected()?;
                let field = *library
                    .fields(&node)
                    .first()
                    .ok_or_else(|| refused(EditRefusal::ReadOnly))?;
                (node, field)
            }
            (LibraryAction::Add, RowKey::AddType(parent)) => (
                library.edit(&parent, DraftEdit::AddType).map_err(refused)?,
                DraftField::Name,
            ),
            (LibraryAction::Add, RowKey::AddProperty(parent)) => (
                library
                    .edit(&parent, DraftEdit::AddProperty)
                    .map_err(refused)?,
                DraftField::Name,
            ),
            (LibraryAction::Move { down }, _) => {
                let moved = library
                    .edit(&selected()?, DraftEdit::Move { down })
                    .map_err(refused)?;
                self.select(moved);
                return Ok(());
            }
            (LibraryAction::Copy, _) => {
                let node = selected()?;
                if library.copy_targets(&node).is_empty() {
                    let taxonomy = library.node(&node).map(|node| node.taxonomy().name());
                    return Err(format!(
                        "Taxonomy \"{}\" has no other type to copy to.",
                        taxonomy.unwrap_or_default()
                    ));
                }
                self.mode = Mode::CopyTo {
                    property: node,
                    cursor: 0,
                    chosen: BTreeSet::new(),
                };
                return Ok(());
            }
            (LibraryAction::CopyChosen, _) => return self.copy_chosen(library),
            (LibraryAction::Add, RowKey::Node(_) | RowKey::NewTaxonomy) => {
                return Err(refused(EditRefusal::NotApplicable));
            }
        };
        if matches!(action, LibraryAction::Create | LibraryAction::Fork) {
            self.expand(&node, library);
        }
        self.open(node, field, library);
        Ok(())
    }

    fn copy_chosen(&mut self, library: &mut LibraryDraft) -> Result<(), String> {
        let Mode::CopyTo {
            property, chosen, ..
        } = &self.mode
        else {
            return Ok(());
        };
        let targets = library
            .copy_targets(property)
            .into_iter()
            .map(|target| target.id().clone())
            .filter(|target| chosen.contains(target))
            .collect::<Vec<_>>();
        if targets.is_empty() {
            return Err("Choose at least one type with space.".into());
        }
        match library.edit(property, DraftEdit::CopyTo(&targets)) {
            Ok(_) => {
                self.expanded.extend(targets);
                self.mode = Mode::Nav;
                Ok(())
            }
            Err(EditRefusal::Taken { target }) => {
                let name = |id: &NodeId| library.node(id).and_then(|node| node.names().pop());
                Err(format!(
                    "Type \"{}\" already has a property \"{}\"; nothing was copied.",
                    name(&targets[target]).unwrap_or_default(),
                    name(property).unwrap_or_default(),
                ))
            }
            Err(refusal) => Err(refusal_sentence(refusal, None)),
        }
    }

    fn expand(&mut self, node: &NodeId, library: &LibraryDraft) {
        if matches!(library.node(node), Some(NodeRef::Taxonomy(_))) {
            for taxonomy in library
                .taxonomies()
                .iter()
                .filter(|taxonomy| taxonomy.id() != node)
            {
                self.expanded.remove(taxonomy.id());
            }
        }
        self.expanded.insert(node.clone());
    }

    /// Selects `node`, shows it, and enters EDIT on `field` with the cursor after its text.
    fn open(&mut self, node: NodeId, field: DraftField, library: &LibraryDraft) {
        let mut ancestor = library.parent(&node);
        while let Some(id) = ancestor {
            ancestor = library.parent(&id);
            self.expand(&id, library);
        }
        let cursor = library
            .text(&node, field)
            .map_or(0, |text| text.graphemes(true).count());
        self.select(node.clone());
        self.mode = Mode::Edit {
            node,
            field,
            cursor,
        };
    }

    /// EDIT keys: enter and ↓ go to the next field (enter wraps), ↑ to the previous one, esc
    /// focuses invalid fields or finishes a valid edit. ←/→ cycle policy or move the cursor; text
    /// keys change the field in the draft.
    pub(super) fn edit_key(
        &mut self,
        key: KeyEvent,
        library: &mut LibraryDraft,
    ) -> Result<(), String> {
        let Mode::Edit {
            node,
            field,
            cursor,
        } = &mut self.mode
        else {
            return Ok(());
        };
        let fields = library.fields(node);
        let Some(position) = fields.iter().position(|candidate| candidate == field) else {
            self.mode = Mode::Nav;
            return Ok(());
        };
        let text = library.text(node, *field).unwrap_or_default();
        let length = text.graphemes(true).count();
        let next = match key.code {
            KeyCode::Esc => {
                if library.untouched_blank(node) {
                    let landing = library
                        .edit(node, DraftEdit::Remove)
                        .map_err(|refusal| refusal_sentence(refusal, None))?;
                    self.mode = Mode::Nav;
                    self.select(landing);
                    return Ok(());
                }
                let errors = library.field_problems(node);
                if let Some(problem) = errors.first() {
                    if let ProblemSite::Field(_, invalid) = problem.site {
                        *field = invalid;
                        *cursor = library
                            .text(node, invalid)
                            .map_or(0, |text| text.graphemes(true).count());
                    }
                    return Ok(());
                }
                self.mode = Mode::Nav;
                return Ok(());
            }
            KeyCode::Enter => Some((position + 1) % fields.len()),
            KeyCode::Down => Some((position + 1).min(fields.len() - 1)),
            KeyCode::Up => Some(position.saturating_sub(1)),
            _ => None,
        };
        if let Some(next) = next {
            *field = fields[next];
            *cursor = library
                .text(node, *field)
                .map_or(0, |text| text.graphemes(true).count());
            return Ok(());
        }
        let (range, insert) = match key.code {
            KeyCode::PageUp => {
                self.inspector_offset = self.inspector_offset.saturating_sub(self.inspector_page());
                return Ok(());
            }
            KeyCode::PageDown => {
                self.inspector_offset = self.inspector_offset.saturating_add(self.inspector_page());
                return Ok(());
            }
            KeyCode::Left | KeyCode::Right if *field == DraftField::Policy => {
                let Some(NodeRef::Property { property, .. }) = library.node(node) else {
                    return Ok(());
                };
                let policy = property.policy().cycle(key.code == KeyCode::Right);
                return library
                    .edit(node, DraftEdit::Policy(policy))
                    .map(|_| ())
                    .map_err(|refusal| refusal_sentence(refusal, Some(*field)));
            }
            _ if *field == DraftField::Policy => return Ok(()),
            KeyCode::Left => {
                *cursor = cursor.saturating_sub(1);
                return Ok(());
            }
            KeyCode::Right => {
                *cursor = (*cursor + 1).min(length);
                return Ok(());
            }
            KeyCode::Home => {
                *cursor = 0;
                return Ok(());
            }
            KeyCode::End => {
                *cursor = length;
                return Ok(());
            }
            KeyCode::Backspace if *cursor > 0 => (*cursor - 1..*cursor, String::new()),
            KeyCode::Delete if *cursor < length => (*cursor..*cursor + 1, String::new()),
            KeyCode::Char(character)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                (*cursor..*cursor, character.to_string())
            }
            _ => return Ok(()),
        };
        let (text, at) = splice(text, range, &insert);
        library
            .edit(node, DraftEdit::Text(*field, &text))
            .map_err(|refusal| refusal_sentence(refusal, Some(*field)))?;
        *cursor = at;
        Ok(())
    }

    /// Inserts pasted text at the cursor in EDIT, as much of it as the field's limit allows.
    pub(super) fn paste(&mut self, pasted: &str, library: &mut LibraryDraft) -> Result<(), String> {
        let Mode::Edit {
            node,
            field,
            cursor,
        } = &mut self.mode
        else {
            return Ok(());
        };
        let (Some(text), Some(limit)) = (library.text(node, *field), field.limit()) else {
            return Ok(());
        };
        let pasted = single_line(pasted);
        let room = limit.saturating_sub(text.graphemes(true).count());
        let insert = pasted.graphemes(true).take(room).collect::<String>();
        let (text, at) = splice(text, *cursor..*cursor, &insert);
        let applied = library.edit(node, DraftEdit::Text(*field, &text));
        if applied.is_ok() {
            *cursor = at;
        }
        if applied.is_err() || insert.len() < pasted.len() {
            return Err(refusal_sentence(EditRefusal::TooLong, Some(*field)));
        }
        Ok(())
    }

    /// Selects the outline row under a click and returns to NAV. Returns true when the click
    /// hit a row.
    pub(super) fn click(&mut self, x: u16, y: u16, library: &LibraryDraft) -> bool {
        if !contains(self.outline_area, x, y) {
            return false;
        }
        let rows = outline::rows(library, &self.expanded);
        let index = self.outline_offset + usize::from(y - self.outline_area.y);
        let Some(row) = rows.get(index) else {
            return false;
        };
        self.set_cursor(row.key());
        self.mode = Mode::Nav;
        true
    }

    /// Places the outline and the Inspector by terminal width, with a dim rule between them,
    /// and returns the `lead` rows it leaves above the outline for a header.
    pub(super) fn render(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        lead: u16,
        library: Option<&LibraryDraft>,
        focused: bool,
        animate: bool,
    ) -> Rect {
        let rows = library.map(|library| outline::rows(library, &self.expanded));
        let panes = panes::Panes::new(area, frame.area(), lead, rows.as_ref().map_or(1, Vec::len));
        panes.render_rule(frame);
        self.inspector_area = Rect::default();
        let (Some(library), Some(rows)) = (library, rows) else {
            self.summary_scroll.reset();
            self.outline_area = Rect::default();
            frame.render_widget(
                Paragraph::new("Taxonomy library unavailable."),
                panes.outline.inner(Margin::new(1, 0)),
            );
            return panes.lead;
        };
        let selected = outline::resolve(&rows, self.cursor.as_ref(), library);
        // EDIT and the copy picker hold focus away from the outline.
        let outline_focused = focused && self.navigating();
        let animate = animate && self.navigating();
        let chips = Chips::new(library, &self.expanded);
        self.outline_area = outline::render(
            frame,
            panes.outline,
            &rows,
            &self.expanded,
            &chips,
            selected,
            &mut self.outline_offset,
            outline_focused,
            &mut self.summary_scroll,
            animate.then_some(area),
        );
        if let Some(node) = rows.get(selected).and_then(|row| row.node()) {
            let focus = match &self.mode {
                Mode::Edit {
                    node: edited,
                    field,
                    cursor,
                    ..
                } if edited == node.id() => Some(inspector::Focus {
                    field: *field,
                    cursor: Some(*cursor),
                }),
                _ => None,
            };
            let shown = inspector::render(
                frame,
                panes.inspector,
                &chips,
                node,
                focus,
                &mut self.inspector_offset,
            );
            self.inspector_area = Rect {
                height: shown,
                ..panes.inspector
            };
        }
        if let Mode::CopyTo {
            property,
            cursor,
            chosen,
        } = &self.mode
        {
            picker::render(frame, area, library, property, *cursor, chosen);
        }
        panes.lead
    }

    pub(super) fn status_line(
        &self,
        library: Option<&LibraryDraft>,
        embedded: bool,
        width: u16,
    ) -> Line<'static> {
        if let Some(hints) = self.picker_hints() {
            return footer_hints(&hints).style(footer_label_style());
        }
        let row = library.and_then(|library| self.selected_row(library));
        let editing = self.editing();
        let property_actions = !editing
            && row.and_then(Row::node).is_some_and(|node| {
                matches!(node, NodeRef::Property { .. })
                    && node.taxonomy().origin() == TaxonomyOrigin::Custom
            });
        let chip = vec![if editing {
            Span::styled(" EDIT ", EDIT_CHIP)
        } else {
            Span::styled(" NAV ", navigation_key_style())
        }];
        let mut hints = Vec::new();
        let mut trailing = if editing {
            Vec::new()
        } else {
            vec![("/", "commands")]
        };
        if embedded && !editing {
            trailing.insert(0, ("tab", "nav bar"));
        }
        if matches!(row, Some(Row::NewTaxonomy)) {
            hints.extend([("←/→", "focus"), ("↑/↓", "move"), ("enter", "new")]);
        } else if let Some(row) = row
            && let Some(node) = row.node()
        {
            let built_in = node.taxonomy().origin() == TaxonomyOrigin::BuiltIn;
            if editing {
                hints.extend([("enter", "next field"), ("esc", "save")]);
            } else if !built_in && matches!(node, NodeRef::Property { .. }) {
                hints.extend([("←/→", "focus"), ("↑/↓", "move")]);
                hints.extend([
                    ("e", "edit"),
                    ("c", "copy property to other types"),
                    ("d", "delete"),
                ]);
            } else if matches!(row, Row::Node(_)) {
                hints.extend([("←/→", "focus"), ("↑/↓", "move")]);
                if outline::has_children(row) {
                    hints.push(if self.expanded.contains(node.id()) {
                        ("enter", "close")
                    } else {
                        ("enter", "open")
                    });
                }
                hints.extend([("n", "new"), ("f", "fork")]);
                if !built_in {
                    hints.push(("e", "edit"));
                    hints.push(("d", "delete"));
                    if !matches!(node, NodeRef::Taxonomy(_)) {
                        hints.push(("alt+↑/↓", "reorder"));
                    }
                }
            } else {
                hints.extend([("←/→", "focus"), ("↑/↓", "move"), ("enter", "add")]);
            }
        }
        let width = usize::from(width);
        let line = |hints: &[(&str, &str)], trailing: &[(&str, &str)], padded: bool| {
            let mut left = chip.clone();
            let mode_width = Line::from(chip.clone()).width();
            let hint_spacing = if padded { 1 } else { 2 };
            let inset = usize::from(super::CONTENT_X).saturating_sub(mode_width + hint_spacing);
            left.push(Span::raw(" ".repeat(inset)));
            left.extend(hint_spans(hints, padded));
            let mut right = Vec::new();
            if !trailing.is_empty() {
                right.extend(hint_spans(trailing, padded).into_iter().skip(1));
            }
            (left, right)
        };
        let fits = |(left, right): &(Vec<Span<'static>>, Vec<Span<'static>>)| {
            Line::from(left.clone()).width() + Line::from(right.clone()).width() < width
        };
        let mut candidate = line(&hints, &trailing, false);
        if property_actions && !fits(&candidate) {
            if let Some((_, label)) = hints.iter_mut().find(|(key, _)| *key == "c") {
                *label = "copy property";
            }
            candidate = line(&hints, &trailing, false);
        }
        if !fits(&candidate) {
            hints.retain(|(key, _)| *key != "alt+↑/↓");
            candidate = line(&hints, &trailing, false);
        }
        while !fits(&candidate) && !trailing.is_empty() {
            trailing.pop();
            candidate = line(&hints, &trailing, false);
        }
        let optional: &[&str] = if property_actions {
            &["↑/↓"]
        } else {
            &["d", "e", "f", "n"]
        };
        for optional in optional {
            if fits(&candidate) {
                break;
            }
            hints.retain(|(key, _)| key != optional);
            candidate = line(&hints, &trailing, false);
        }
        // Choose hints using their original widths, then spend spare space on chip padding.
        let padded = line(&hints, &trailing, true);
        if fits(&padded) {
            candidate = padded;
        }
        fit(candidate.0, candidate.1, width)
    }

    #[cfg(test)]
    pub(super) fn observe(&self, library: Option<&LibraryDraft>) -> Observation {
        let visible = library.map_or_else(Vec::new, |library| {
            outline::rows(library, &self.expanded)
                .into_iter()
                .skip(self.outline_offset)
                .take(usize::from(self.outline_area.height))
                .map(|row| match row {
                    Row::Node(node) => Some(node.id().clone()),
                    Row::AddType(_) | Row::AddProperty(..) | Row::NewTaxonomy => None,
                })
                .collect()
        });
        Observation {
            outline_area: self.outline_area,
            inspector_area: self.inspector_area,
            visible,
            cursor: self.cursor.clone(),
            cursor_path: library
                .and_then(|library| self.selected_row(library))
                .map(Row::names)
                .unwrap_or_default(),
            expanded: self.expanded.iter().cloned().collect(),
            outline_offset: self.outline_offset,
            inspector_offset: self.inspector_offset,
            mode: self.mode.clone(),
        }
    }
}

/// Replaces the graphemes in `range` of `text` with `insert`. Returns the new text and the
/// grapheme index just past the insertion.
fn splice(text: &str, range: Range<usize>, insert: &str) -> (String, usize) {
    let graphemes = text.graphemes(true).collect::<Vec<_>>();
    let head = format!("{}{insert}", graphemes[..range.start].concat());
    let at = head.graphemes(true).count();
    (format!("{head}{}", graphemes[range.end..].concat()), at)
}

/// What the model checker sees of this section: the raw cursor and expanded ids, the selected
/// row's names, scroll offsets, the mode with its field and text cursor, and where the outline
/// drew which nodes, an add row as none.
#[cfg(test)]
#[derive(Debug)]
pub(super) struct Observation {
    pub(super) outline_area: Rect,
    pub(super) inspector_area: Rect,
    pub(super) visible: Vec<Option<NodeId>>,
    pub(super) cursor: Option<RowKey>,
    pub(super) cursor_path: Vec<String>,
    pub(super) expanded: Vec<NodeId>,
    pub(super) outline_offset: usize,
    pub(super) inspector_offset: u16,
    pub(super) mode: Mode,
}

fn hint_spans(hints: &[(&str, &str)], padded: bool) -> Vec<Span<'static>> {
    hints
        .iter()
        .flat_map(|(key, label)| {
            [
                Span::raw(if padded { " " } else { "  " }),
                Span::styled(
                    if padded {
                        format!(" {key} ")
                    } else {
                        (*key).to_owned()
                    },
                    footer_key_style(),
                ),
                Span::styled(format!(" {label}"), footer_label_style()),
            ]
        })
        .collect()
}

const fn policy_style(policy: Policy) -> Style {
    Style::new().fg(match policy {
        Policy::Required => Color::Red,
        Policy::Recommended => Color::White,
        Policy::Conditional => Color::Cyan,
        Policy::Optional => Color::DarkGray,
    })
}

/// `left` then `right` flush to the end of `width` columns, cutting `left` short with `…` when
/// both do not fit.
fn fit(left: Vec<Span<'static>>, right: Vec<Span<'static>>, width: usize) -> Line<'static> {
    let right_width = Line::from(right.clone()).width();
    let room = width.saturating_sub(right_width + usize::from(right_width > 0));
    let mut spans = Vec::with_capacity(left.len() + right.len() + 1);
    let mut used = 0;
    for span in left {
        let span_width = span.width();
        if used + span_width <= room {
            used += span_width;
            spans.push(span);
            continue;
        }
        let mut cut = String::new();
        for grapheme in span.content.graphemes(true) {
            if Span::raw(format!("{cut}{grapheme}…")).width() + used > room {
                break;
            }
            cut.push_str(grapheme);
        }
        if room > used {
            cut.push('…');
            used += Span::raw(cut.as_str()).width();
            spans.push(Span::styled(cut, span.style));
        }
        break;
    }
    spans.push(Span::raw(
        " ".repeat(width.saturating_sub(used + right_width)),
    ));
    spans.extend(right);
    Line::from(spans)
}
