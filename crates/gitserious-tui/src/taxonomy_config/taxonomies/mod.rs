//! The Taxonomies section: one outline of Taxonomy › Type › Property above an Inspector that
//! describes the selected row and edits it in place. The view keeps only the cursor, the
//! expanded nodes, scroll offsets, and NAV or EDIT with a text cursor; every edit goes straight
//! to the Library draft, and rows and field text come from it on every render and key.

use std::collections::BTreeSet;
use std::ops::Range;

use gitserious_app::{
    ChangeSet, DraftEdit, DraftField, EditRefusal, LibraryDraft, Mark, NodeId, NodeRef, Policy,
    TaxonomyOrigin,
};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use unicode_segmentation::UnicodeSegmentation;

use super::contains;
use super::naming::{refusal_sentence, single_line};
use crate::theme::{AMBER, JET_BLACK, ZEBRA_BACKGROUND, focus_frame_style, navigation_key_style};
use outline::Row;
pub(super) use outline::RowKey;

mod inspector;
mod outline;

const DIM: Style = Style::new().fg(Color::Gray);
const KEY_STYLE: Style = Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD);
const BADGE_STYLE: Style = Style::new().fg(Color::Gray).bg(ZEBRA_BACKGROUND);
const BAND: Style = Style::new().bg(Color::Rgb(40, 40, 40));
const EDIT_CHIP: Style = Style::new()
    .fg(JET_BLACK)
    .bg(AMBER)
    .add_modifier(Modifier::BOLD);
const INSPECTOR_PAGE: u16 = 8;

/// A node's state as its outline row, the Inspector header, and the footer show it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Chip {
    ReadOnly,
    New,
    Modified,
    #[expect(dead_code, reason = "problems reach the outline in the next change")]
    Invalid(usize),
}

impl Chip {
    /// The chip of a node's mark in `changes`.
    fn of(changes: &ChangeSet, id: &NodeId) -> Option<Self> {
        changes.mark(id).map(|mark| match mark {
            Mark::New => Self::New,
            Mark::Modified => Self::Modified,
        })
    }

    fn label(self) -> String {
        match self {
            Self::ReadOnly => "◇ read-only".into(),
            Self::New => "+ new".into(),
            Self::Modified => "~ modified".into(),
            Self::Invalid(count) => format!("▲ invalid {count}"),
        }
    }

    fn span(self) -> Span<'static> {
        let style = match self {
            Self::ReadOnly => BADGE_STYLE,
            Self::New => Style::new().fg(Color::Green).bg(Color::Rgb(20, 44, 24)),
            Self::Modified => Style::new().fg(AMBER).bg(Color::Rgb(52, 38, 12)),
            Self::Invalid(_) => Style::new().fg(Color::Red).bg(Color::Rgb(56, 18, 18)),
        };
        Span::styled(format!(" {} ", self.label()), style)
    }
}

#[derive(Debug, Default)]
pub(super) struct TaxonomiesView {
    /// `None` until the user picks a row; the first row shows as selected.
    cursor: Option<RowKey>,
    expanded: BTreeSet<NodeId>,
    mode: Mode,
    outline_offset: usize,
    inspector_offset: u16,
    outline_area: Rect,
}

/// NAV moves through the outline. EDIT types into one field of the selected node; `cursor` is
/// a grapheme index into that field's text in the draft.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) enum Mode {
    #[default]
    Nav,
    Edit {
        node: NodeId,
        field: DraftField,
        cursor: usize,
    },
}

/// A change to the Library a NAV key asks for. `State` runs it after its scope guard.
#[derive(Clone, Copy, Debug)]
pub(super) enum LibraryAction {
    Create,
    Fork,
    Edit,
    Add,
    Policy { forward: bool },
    Move { down: bool },
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
    pub(super) fn select(&mut self, node: NodeId) {
        self.set_cursor(RowKey::Node(node));
    }

    fn set_cursor(&mut self, key: RowKey) {
        if self.cursor.as_ref() != Some(&key) {
            self.inspector_offset = 0;
        }
        self.cursor = Some(key);
    }

    pub(super) const fn editing(&self) -> bool {
        matches!(self.mode, Mode::Edit { .. })
    }

    pub(super) fn leave_edit(&mut self) {
        self.mode = Mode::Nav;
    }

    fn selected_row<'a>(&self, library: &'a LibraryDraft) -> Option<Row<'a>> {
        let rows = outline::rows(library, &self.expanded);
        let index = outline::resolve(&rows, self.cursor.as_ref(), library);
        rows.get(index).copied()
    }

    /// The selected row's node, or for an add row the node it adds to.
    pub(super) fn selected<'a>(&self, library: &'a LibraryDraft) -> Option<NodeRef<'a>> {
        self.selected_row(library).map(Row::node)
    }

    /// NAV keys: move, expand, collapse, scroll the Inspector, and the keys that change the
    /// Library, which come back as an action.
    pub(super) fn key(&mut self, key: KeyEvent, library: &LibraryDraft) -> Nav {
        let rows = outline::rows(library, &self.expanded);
        let index = outline::resolve(&rows, self.cursor.as_ref(), library);
        let Some(&row) = rows.get(index) else {
            return Nav::Unhandled;
        };
        let node = row.node();
        let open = outline::has_children(row) && self.expanded.contains(node.id());
        let custom = node.taxonomy().origin() == TaxonomyOrigin::Custom;
        let plain = key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT;
        match key.code {
            KeyCode::Char('e') if plain => return Nav::Act(LibraryAction::Edit),
            KeyCode::Char('f') if plain => return Nav::Act(LibraryAction::Fork),
            KeyCode::Char('n') if plain => return Nav::Act(LibraryAction::Create),
            KeyCode::Left | KeyCode::Right
                if custom && matches!(row, Row::Node(NodeRef::Property { .. })) =>
            {
                return Nav::Act(LibraryAction::Policy {
                    forward: key.code == KeyCode::Right,
                });
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
            KeyCode::Enter if !matches!(row, Row::Node(_)) => return Nav::Act(LibraryAction::Add),
            KeyCode::Up => self.set_cursor(rows[index.saturating_sub(1)].key()),
            KeyCode::Down => self.set_cursor(rows[(index + 1).min(rows.len() - 1)].key()),
            KeyCode::Right if open => self.set_cursor(rows[index + 1].key()),
            KeyCode::Right | KeyCode::Enter if outline::has_children(row) && !open => {
                self.expanded.insert(node.id().clone());
                self.set_cursor(row.key());
            }
            KeyCode::Left | KeyCode::Enter if open => {
                self.expanded.remove(node.id());
                self.set_cursor(row.key());
            }
            KeyCode::Left => match row {
                Row::Node(node) => match library.parent(node.id()) {
                    Some(parent) => self.select(parent),
                    None => return Nav::Sidebar,
                },
                Row::AddType(_) | Row::AddProperty(..) => self.select(node.id().clone()),
            },
            KeyCode::PageUp => {
                self.inspector_offset = self.inspector_offset.saturating_sub(INSPECTOR_PAGE);
            }
            KeyCode::PageDown => {
                self.inspector_offset = self.inspector_offset.saturating_add(INSPECTOR_PAGE);
            }
            _ => return Nav::Unhandled,
        }
        Nav::Handled
    }

    /// Runs a NAV action on the selected row. Every action but a policy change or a move ends in
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
        let (key, node) = (row.key(), row.node().id().clone());
        let (node, field) = match (action, key) {
            (LibraryAction::Create, _) => (library.create_taxonomy(), DraftField::Name),
            (LibraryAction::Fork, _) => {
                let taxonomy = library
                    .node(&node)
                    .map(|node| node.taxonomy().id().clone())
                    .ok_or_else(|| refused(EditRefusal::Missing))?;
                (library.fork(&taxonomy).map_err(refused)?, DraftField::Name)
            }
            (LibraryAction::Edit, _) => {
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
            (LibraryAction::Policy { forward }, _) => {
                let Some(NodeRef::Property { property, .. }) = library.node(&node) else {
                    return Err(refused(EditRefusal::NotApplicable));
                };
                let policy = property.policy().cycle(forward);
                library
                    .edit(&node, DraftEdit::Policy(policy))
                    .map_err(refused)?;
                return Ok(());
            }
            (LibraryAction::Move { down }, _) => {
                let moved = library
                    .edit(&node, DraftEdit::Move { down })
                    .map_err(refused)?;
                self.select(moved);
                return Ok(());
            }
            (LibraryAction::Add, RowKey::Node(_)) => {
                return Err(refused(EditRefusal::NotApplicable));
            }
        };
        if matches!(action, LibraryAction::Create | LibraryAction::Fork) {
            self.expanded.insert(node.clone());
        }
        self.open(node, field, library);
        Ok(())
    }

    /// Selects `node`, shows it, and enters EDIT on `field` with the cursor after its text.
    fn open(&mut self, node: NodeId, field: DraftField, library: &LibraryDraft) {
        let mut ancestor = library.parent(&node);
        while let Some(id) = ancestor {
            ancestor = library.parent(&id);
            self.expanded.insert(id);
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
    /// returns to NAV with the draft kept, ←/→ cycle the policy or move the cursor, and text
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
                self.inspector_offset = self.inspector_offset.saturating_sub(INSPECTOR_PAGE);
                return Ok(());
            }
            KeyCode::PageDown => {
                self.inspector_offset = self.inspector_offset.saturating_add(INSPECTOR_PAGE);
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

    /// Stacks the outline above the Inspector, about 40:60, keeping at least 7 Inspector rows.
    pub(super) fn render(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        library: Option<&LibraryDraft>,
        focused: bool,
    ) {
        let outline_height = (area.height * 2 / 5)
            .max(5)
            .min(area.height.saturating_sub(7));
        let [outline_area, inspector_area] =
            Layout::vertical([Constraint::Length(outline_height), Constraint::Min(0)]).areas(area);
        let Some(library) = library else {
            self.outline_area = Rect::default();
            let block = Block::bordered()
                .border_style(focus_frame_style(focused))
                .title("Taxonomies");
            frame.render_widget(
                Paragraph::new("Taxonomy library unavailable."),
                block.inner(outline_area),
            );
            frame.render_widget(block, outline_area);
            return;
        };
        let rows = outline::rows(library, &self.expanded);
        let selected = outline::resolve(&rows, self.cursor.as_ref(), library);
        let outline_focused = focused && !self.editing();
        let changes = library.changes();
        self.outline_area = outline::render(
            frame,
            outline_area,
            &rows,
            &self.expanded,
            &changes,
            selected,
            &mut self.outline_offset,
            outline_focused,
        );
        if let Some(&row) = rows.get(selected) {
            let node = row.node();
            let focus = match &self.mode {
                Mode::Edit {
                    node: edited,
                    field,
                    cursor,
                } if edited == node.id() => Some(inspector::Focus {
                    field: *field,
                    cursor: Some(*cursor),
                }),
                Mode::Nav
                    if matches!(row, Row::Node(NodeRef::Property { .. }))
                        && !library.fields(node.id()).is_empty() =>
                {
                    Some(inspector::Focus {
                        field: DraftField::Policy,
                        cursor: None,
                    })
                }
                _ => None,
            };
            inspector::render(
                frame,
                inspector_area,
                library,
                node,
                Chip::of(&changes, node.id()),
                focus,
                &mut self.inspector_offset,
            );
        }
    }

    /// The footer: mode, the selected row's kind and chip, then its keys. The reorder and arrow
    /// hints go first and the trailing keys next, front to back, when the row is narrow.
    pub(super) fn status_line(
        &self,
        library: Option<&LibraryDraft>,
        embedded: bool,
        width: u16,
    ) -> Line<'static> {
        let row = library.and_then(|library| self.selected_row(library));
        let editing = self.editing();
        let mut chip = vec![if editing {
            Span::styled(" EDIT ", EDIT_CHIP)
        } else {
            Span::styled(" NAV ", navigation_key_style())
        }];
        let mut hints = Vec::new();
        let mut trailing = if editing {
            vec![("ctrl+s", "review")]
        } else {
            vec![("/", "cmds"), ("?", "help")]
        };
        if embedded {
            trailing.insert(0, ("tab", "tabs"));
        }
        if let Some(row) = row {
            let node = row.node();
            let kind = match node {
                NodeRef::Taxonomy(_) => "taxonomy",
                NodeRef::Type { .. } => "type",
                NodeRef::Property { .. } => "property",
            };
            chip.push(Span::raw(format!(" · {kind}")));
            let built_in = node.taxonomy().origin() == TaxonomyOrigin::BuiltIn;
            let state = if built_in {
                Some(Chip::ReadOnly)
            } else {
                library.and_then(|library| Chip::of(&library.changes(), node.id()))
            };
            if let Some(state) = state {
                chip.extend([Span::raw(" · "), state.span()]);
            }
            if editing {
                hints.extend([("enter", "next field"), ("esc", "done")]);
            } else if matches!(row, Row::Node(_)) {
                if !built_in {
                    hints.push(("e", "edit"));
                }
                hints.extend([("f", "fork"), ("n", "new")]);
                if !built_in {
                    hints.push(("d", "delete"));
                    if matches!(node, NodeRef::Property { .. }) {
                        hints.push(("←/→", "policy"));
                    }
                    if !matches!(node, NodeRef::Taxonomy(_)) {
                        hints.push(("alt+↑/↓", "reorder"));
                    }
                }
                hints.push(("↑/↓", "move"));
            } else {
                hints.extend([("enter", "add"), ("↑/↓", "move")]);
            }
        }
        let width = usize::from(width);
        let line = |hints: &[(&str, &str)], trailing: &[(&str, &str)]| {
            let mut left = chip.clone();
            left.extend(hint_spans(hints));
            let mut right = Vec::new();
            if !trailing.is_empty() {
                right.push(Span::styled("│", DIM));
                right.extend(hint_spans(trailing));
                right.push(Span::raw(" "));
            }
            (left, right)
        };
        let fits = |(left, right): &(Vec<Span<'static>>, Vec<Span<'static>>)| {
            Line::from(left.clone()).width() + Line::from(right.clone()).width() < width
        };
        let mut candidate = line(&hints, &trailing);
        for optional in ["alt+↑/↓", "↑/↓"] {
            if fits(&candidate) {
                break;
            }
            hints.retain(|(key, _)| *key != optional);
            candidate = line(&hints, &trailing);
        }
        while !fits(&candidate) && !trailing.is_empty() {
            trailing.remove(0);
            candidate = line(&hints, &trailing);
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
                    Row::AddType(_) | Row::AddProperty(..) => None,
                })
                .collect()
        });
        Observation {
            outline_area: self.outline_area,
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

/// The Config sidebar's last two rows while the Library has unapplied changes: their count, as
/// the Config tab shows it, and the key that reviews them.
pub(super) fn render_pending(frame: &mut Frame<'_>, menu: Rect, count: usize) {
    if count == 0 || menu.height <= 5 {
        return;
    }
    let area = Rect::new(
        menu.x.saturating_add(1),
        menu.bottom().saturating_sub(3),
        menu.width.saturating_sub(2),
        2,
    );
    let text = format!("● {count} unapplied");
    let pad = if text.chars().count() < usize::from(area.width) {
        " "
    } else {
        ""
    };
    let lines = vec![
        Line::styled(format!("{pad}{text}"), Style::new().fg(AMBER)),
        Line::from(vec![
            Span::raw(pad),
            Span::styled("ctrl+s", KEY_STYLE),
            Span::styled(" review", DIM),
        ]),
    ];
    frame.render_widget(Paragraph::new(lines), area);
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
#[expect(
    dead_code,
    reason = "the model checker's fingerprint reads these fields through Debug"
)]
pub(super) struct Observation {
    pub(super) outline_area: Rect,
    pub(super) visible: Vec<Option<NodeId>>,
    pub(super) cursor: Option<RowKey>,
    pub(super) cursor_path: Vec<String>,
    pub(super) expanded: Vec<NodeId>,
    pub(super) outline_offset: usize,
    pub(super) inspector_offset: u16,
    pub(super) mode: Mode,
}

fn hint_spans(hints: &[(&str, &str)]) -> Vec<Span<'static>> {
    hints
        .iter()
        .flat_map(|(key, label)| {
            [
                Span::raw("  "),
                Span::styled((*key).to_owned(), KEY_STYLE),
                Span::styled(format!(" {label}"), DIM),
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
        for character in span.content.chars() {
            if Span::raw(format!("{cut}{character}…")).width() + used > room {
                break;
            }
            cut.push(character);
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
