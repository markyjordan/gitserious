use std::collections::BTreeSet;

use gitserious_app::{
    ChangeSet, LibraryDraft, NodeId, NodeRef, TaxonomyNode, TaxonomyOrigin, TypeNode,
};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use super::{BADGE_STYLE, BAND, Chip, DIM, fit, policy_style};
use crate::theme::{focus_frame_style, frame_style};

pub(super) const ADD_TYPE: &str = "+ add type";
pub(super) const ADD_PROPERTY: &str = "+ add property";
const ADD_STYLE: Style = Style::new().fg(Color::LightBlue);

/// What a row stands for. The add rows are not drafts, so they name the node they add to.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in super::super) enum RowKey {
    Node(NodeId),
    AddType(NodeId),
    AddProperty(NodeId),
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Row<'a> {
    Node(NodeRef<'a>),
    AddType(&'a TaxonomyNode),
    AddProperty(&'a TaxonomyNode, &'a TypeNode),
}

impl<'a> Row<'a> {
    pub(super) fn key(self) -> RowKey {
        match self {
            Self::Node(node) => RowKey::Node(node.id().clone()),
            Self::AddType(taxonomy) => RowKey::AddType(taxonomy.id().clone()),
            Self::AddProperty(_, commit_type) => RowKey::AddProperty(commit_type.id().clone()),
        }
    }

    /// The row's node, or for an add row the node it adds to.
    pub(super) const fn node(self) -> NodeRef<'a> {
        match self {
            Self::Node(node) => node,
            Self::AddType(taxonomy) => NodeRef::Taxonomy(taxonomy),
            Self::AddProperty(taxonomy, commit_type) => NodeRef::Type {
                taxonomy,
                commit_type,
            },
        }
    }

    /// Names from the taxonomy down to this row; an add row ends with its label.
    #[cfg(test)]
    pub(super) fn names(self) -> Vec<String> {
        let mut names = self
            .node()
            .names()
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        match self {
            Self::Node(_) => {}
            Self::AddType(_) => names.push(ADD_TYPE.into()),
            Self::AddProperty(..) => names.push(ADD_PROPERTY.into()),
        }
        names
    }
}

/// Visible rows in display order: every taxonomy, the types of each expanded taxonomy, and the
/// properties of each expanded type. A custom taxonomy or type ends with its add row.
pub(super) fn rows<'a>(library: &'a LibraryDraft, expanded: &BTreeSet<NodeId>) -> Vec<Row<'a>> {
    let mut rows = Vec::new();
    for taxonomy in library.taxonomies() {
        rows.push(Row::Node(NodeRef::Taxonomy(taxonomy)));
        if !expanded.contains(taxonomy.id()) {
            continue;
        }
        let custom = taxonomy.origin() == TaxonomyOrigin::Custom;
        for commit_type in taxonomy.types() {
            rows.push(Row::Node(NodeRef::Type {
                taxonomy,
                commit_type,
            }));
            if expanded.contains(commit_type.id()) {
                rows.extend(commit_type.properties().iter().map(|property| {
                    Row::Node(NodeRef::Property {
                        taxonomy,
                        commit_type,
                        property,
                    })
                }));
                if custom {
                    rows.push(Row::AddProperty(taxonomy, commit_type));
                }
            }
        }
        if custom {
            rows.push(Row::AddType(taxonomy));
        }
    }
    rows
}

/// Whether the row opens: a custom taxonomy or type always does, to show its add row.
pub(super) fn has_children(row: Row<'_>) -> bool {
    let Row::Node(node) = row else {
        return false;
    };
    let custom = node.taxonomy().origin() == TaxonomyOrigin::Custom;
    match node {
        NodeRef::Taxonomy(taxonomy) => custom || !taxonomy.types().is_empty(),
        NodeRef::Type { commit_type, .. } => custom || !commit_type.properties().is_empty(),
        NodeRef::Property { .. } => false,
    }
}

/// The cursor's row, else the row of its nearest visible ancestor, else the first row.
pub(super) fn resolve(rows: &[Row<'_>], cursor: Option<&RowKey>, library: &LibraryDraft) -> usize {
    let Some(cursor) = cursor else {
        return 0;
    };
    if let Some(index) = rows.iter().position(|row| &row.key() == cursor) {
        return index;
    }
    let mut id = Some(match cursor {
        RowKey::Node(id) | RowKey::AddType(id) | RowKey::AddProperty(id) => id.clone(),
    });
    while let Some(current) = id {
        if let Some(index) = rows
            .iter()
            .position(|row| row.key() == RowKey::Node(current.clone()))
        {
            return index;
        }
        id = library.parent(&current);
    }
    0
}

/// Draws the rows, each with the chip of its mark, and returns the area they fill, for mouse
/// hit-testing.
#[expect(
    clippy::too_many_arguments,
    reason = "the outline draws from the view's state and the Library's derived rows and changes"
)]
pub(super) fn render(
    frame: &mut Frame<'_>,
    area: Rect,
    rows: &[Row<'_>],
    expanded: &BTreeSet<NodeId>,
    changes: &ChangeSet,
    selected: usize,
    offset: &mut usize,
    focused: bool,
) -> Rect {
    let mut block = Block::bordered()
        .border_style(focus_frame_style(focused))
        .title_style(focus_frame_style(focused))
        .title("Taxonomies");
    let inner = block.inner(area);
    let height = usize::from(inner.height);
    if selected < *offset {
        *offset = selected;
    } else if height > 0 && selected >= *offset + height {
        *offset = selected + 1 - height;
    }
    *offset = (*offset).min(rows.len().saturating_sub(height));
    let below = rows.len().saturating_sub(*offset + height);
    if *offset > 0 {
        block = block.title_top(Line::styled(format!(" ↑ {} more ", *offset), DIM).right_aligned());
    }
    if below > 0 {
        block = block.title_bottom(Line::styled(format!(" ↓ {below} more "), DIM).right_aligned());
    }
    frame.render_widget(block, area);
    let width = usize::from(inner.width).saturating_sub(1);
    let lines = rows
        .iter()
        .enumerate()
        .skip(*offset)
        .take(height)
        .map(|(index, &row)| {
            let open = has_children(row) && expanded.contains(row.node().id());
            let chip = match row {
                Row::Node(node) => Chip::of(changes, node.id()),
                Row::AddType(_) | Row::AddProperty(..) => None,
            };
            row_line(row, open, chip, index == selected, focused, width)
        })
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines), inner);
    inner
}

/// Pads `name` to the widest of `names` plus two spaces.
fn column<'a>(name: &str, names: impl Iterator<Item = &'a str>) -> String {
    let width = names.map(str::len).max().unwrap_or(0) + 2;
    format!("{name:<width$}")
}

/// One row. A taxonomy without a chip shows where it lives instead.
fn row_line(
    row: Row<'_>,
    open: bool,
    chip: Option<Chip>,
    selected: bool,
    focused: bool,
    width: usize,
) -> Line<'static> {
    let bar = match (selected, focused) {
        (false, _) => Span::raw(" "),
        (true, true) => Span::styled("▌", Style::default().fg(Color::Yellow)),
        (true, false) => Span::styled("▌", frame_style()),
    };
    let name_style = if selected {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };
    let marker = Span::styled(if open { "▾ " } else { "▸ " }, frame_style());
    let (left, right) = match row {
        Row::AddType(_) => (
            vec![bar, Span::raw("  "), Span::styled(ADD_TYPE, ADD_STYLE)],
            Vec::new(),
        ),
        Row::AddProperty(..) => (
            vec![
                bar,
                Span::raw("    "),
                Span::styled("└ ", frame_style()),
                Span::styled(ADD_PROPERTY, ADD_STYLE),
            ],
            Vec::new(),
        ),
        Row::Node(NodeRef::Taxonomy(taxonomy)) => (
            vec![
                bar,
                marker,
                Span::styled(taxonomy.name().to_owned(), name_style),
            ],
            vec![chip.map_or_else(
                || Span::styled(format!(" {} ", taxonomy.origin().as_str()), BADGE_STYLE),
                Chip::span,
            )],
        ),
        Row::Node(NodeRef::Type {
            taxonomy,
            commit_type,
        }) => {
            let name = column(
                commit_type.name(),
                taxonomy.types().iter().map(TypeNode::name),
            );
            let mut left = vec![bar, Span::raw("  "), marker, Span::styled(name, name_style)];
            if !open {
                let summary = commit_type
                    .properties()
                    .iter()
                    .map(gitserious_app::PropertyNode::name)
                    .collect::<Vec<_>>()
                    .join(" · ");
                left.push(Span::styled(summary, DIM));
            }
            let mut right = vec![Span::styled(
                commit_type.properties().len().to_string(),
                DIM,
            )];
            if let Some(chip) = chip {
                right.extend([Span::raw("  "), chip.span()]);
            }
            (left, right)
        }
        Row::Node(NodeRef::Property {
            taxonomy,
            commit_type,
            property,
        }) => {
            let siblings = commit_type.properties();
            let last = taxonomy.origin() == TaxonomyOrigin::BuiltIn
                && siblings
                    .last()
                    .is_some_and(|last| last.id() == property.id());
            let name = column(
                property.name(),
                siblings.iter().map(gitserious_app::PropertyNode::name),
            );
            (
                vec![
                    bar,
                    Span::raw("    "),
                    Span::styled(if last { "└ " } else { "├ " }, frame_style()),
                    Span::styled(name, name_style),
                    Span::styled(property.policy().label(), policy_style(property.policy())),
                ],
                chip.map(Chip::span).into_iter().collect(),
            )
        }
    };
    let line = fit(left, right, width);
    if selected { line.style(BAND) } else { line }
}
