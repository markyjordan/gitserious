//! The Taxonomies section: one outline of Taxonomy › Type › Property above an Inspector that
//! describes the selected row. The view keeps only the cursor, the expanded nodes, and scroll
//! offsets; rows come from the Library on every render and key.

use std::collections::BTreeSet;

use gitserious_app::{LibraryDraft, NodeId, NodeRef, Policy, TaxonomyOrigin};
use ratatui::Frame;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use super::contains;
use crate::theme::{ZEBRA_BACKGROUND, focus_frame_style, navigation_key_style};

mod inspector;
mod outline;

const DIM: Style = Style::new().fg(Color::Gray);
const KEY_STYLE: Style = Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD);
const BADGE_STYLE: Style = Style::new().fg(Color::Gray).bg(ZEBRA_BACKGROUND);
const BAND: Style = Style::new().bg(Color::Rgb(40, 40, 40));
const INSPECTOR_PAGE: u16 = 8;

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum RowKey {
    Node(NodeId),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum Mode {
    #[default]
    Nav,
}

impl TaxonomiesView {
    pub(super) fn select(&mut self, node: NodeId) {
        if self.cursor.as_ref() != Some(&RowKey::Node(node.clone())) {
            self.inspector_offset = 0;
        }
        self.cursor = Some(RowKey::Node(node));
    }

    fn cursor_id(&self) -> Option<&NodeId> {
        self.cursor.as_ref().map(|RowKey::Node(id)| id)
    }

    /// The selected row's node.
    pub(super) fn selected<'a>(&self, library: &'a LibraryDraft) -> Option<NodeRef<'a>> {
        let rows = outline::rows(library, &self.expanded);
        let index = outline::resolve(&rows, self.cursor_id(), library);
        rows.get(index).copied()
    }

    /// Moves through the outline. Returns true when the key hands focus to the sidebar.
    pub(super) fn key(&mut self, code: KeyCode, library: &LibraryDraft) -> bool {
        let rows = outline::rows(library, &self.expanded);
        let index = outline::resolve(&rows, self.cursor_id(), library);
        let Some(&row) = rows.get(index) else {
            return false;
        };
        let open = self.expanded.contains(row.id()) && outline::has_children(row);
        match code {
            KeyCode::Up => self.select(rows[index.saturating_sub(1)].id().clone()),
            KeyCode::Down => self.select(rows[(index + 1).min(rows.len() - 1)].id().clone()),
            KeyCode::Right if open => self.select(rows[index + 1].id().clone()),
            KeyCode::Right | KeyCode::Enter if outline::has_children(row) && !open => {
                self.expanded.insert(row.id().clone());
                self.select(row.id().clone());
            }
            KeyCode::Left | KeyCode::Enter if open => {
                self.expanded.remove(row.id());
                self.select(row.id().clone());
            }
            KeyCode::Left => match library.parent(row.id()) {
                Some(parent) => self.select(parent),
                None => return true,
            },
            KeyCode::PageUp => {
                self.inspector_offset = self.inspector_offset.saturating_sub(INSPECTOR_PAGE);
            }
            KeyCode::PageDown => {
                self.inspector_offset = self.inspector_offset.saturating_add(INSPECTOR_PAGE);
            }
            _ => {}
        }
        false
    }

    /// Selects the outline row under a click. Returns true when the click hit a row.
    pub(super) fn click(&mut self, x: u16, y: u16, library: &LibraryDraft) -> bool {
        if !contains(self.outline_area, x, y) {
            return false;
        }
        let rows = outline::rows(library, &self.expanded);
        let index = self.outline_offset + usize::from(y - self.outline_area.y);
        let Some(row) = rows.get(index) else {
            return false;
        };
        self.select(row.id().clone());
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
        let selected = outline::resolve(&rows, self.cursor_id(), library);
        self.outline_area = outline::render(
            frame,
            outline_area,
            &rows,
            &self.expanded,
            selected,
            &mut self.outline_offset,
            focused,
        );
        if let Some(&node) = rows.get(selected) {
            inspector::render(
                frame,
                inspector_area,
                library,
                node,
                &mut self.inspector_offset,
            );
        }
    }

    /// The footer: mode, the selected row's kind, read-only on built-ins, then its keys. Arrow
    /// hints go first and the trailing group second when the row is narrow.
    pub(super) fn status_line(
        &self,
        library: Option<&LibraryDraft>,
        embedded: bool,
        width: u16,
    ) -> Line<'static> {
        let node = library.and_then(|library| self.selected(library));
        let mut chip = vec![Span::styled(
            format!(" {} ", self.mode.label()),
            navigation_key_style(),
        )];
        let mut hints = Vec::new();
        if let Some(node) = node {
            let kind = match node {
                NodeRef::Taxonomy(_) => "taxonomy",
                NodeRef::Type { .. } => "type",
                NodeRef::Property { .. } => "property",
            };
            chip.push(Span::raw(format!(" · {kind}")));
            let built_in = node.taxonomy().origin() == TaxonomyOrigin::BuiltIn;
            if built_in {
                chip.push(Span::raw(" · "));
                chip.push(Span::styled(" ◇ read-only ", BADGE_STYLE));
            } else {
                hints.push(("e", "edit"));
            }
            hints.extend([("f", "fork"), ("n", "new")]);
            if !built_in {
                hints.push(("d", "delete"));
            }
            hints.push(("↑/↓", "move"));
        }
        let mut trailing = vec![("/", "cmds"), ("?", "help")];
        if embedded {
            trailing.insert(0, ("tab", "tabs"));
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
        if !fits(&candidate) {
            hints.retain(|(key, _)| *key != "↑/↓");
            candidate = line(&hints, &trailing);
        }
        if !fits(&candidate) {
            candidate = line(&hints, &[]);
        }
        fit(candidate.0, candidate.1, width)
    }

    #[cfg(test)]
    pub(super) fn observe(&self, library: Option<&LibraryDraft>) -> Observation {
        Observation {
            cursor: self.cursor.clone(),
            cursor_path: library
                .and_then(|library| self.selected(library))
                .map(|node| node.names().into_iter().map(str::to_owned).collect())
                .unwrap_or_default(),
            expanded: self.expanded.iter().cloned().collect(),
            outline_offset: self.outline_offset,
            inspector_offset: self.inspector_offset,
            mode: self.mode,
        }
    }
}

impl Mode {
    const fn label(self) -> &'static str {
        match self {
            Self::Nav => "NAV",
        }
    }
}

/// What the model checker sees of this section: the raw cursor and expanded ids, the selected
/// row's names, scroll offsets, and the mode.
#[cfg(test)]
#[derive(Debug)]
#[expect(
    dead_code,
    reason = "the model checker's fingerprint reads these fields through Debug"
)]
pub(super) struct Observation {
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
