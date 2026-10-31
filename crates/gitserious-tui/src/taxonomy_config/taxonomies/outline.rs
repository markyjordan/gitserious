use std::collections::BTreeSet;

use gitserious_app::{LibraryDraft, NodeId, NodeRef};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use super::{BADGE_STYLE, BAND, DIM, fit, policy_style};
use crate::theme::{focus_frame_style, frame_style};

/// Visible rows in display order: every taxonomy, the types of each expanded taxonomy, and the
/// properties of each expanded type.
pub(super) fn rows<'a>(library: &'a LibraryDraft, expanded: &BTreeSet<NodeId>) -> Vec<NodeRef<'a>> {
    let mut rows = Vec::new();
    for taxonomy in library.taxonomies() {
        rows.push(NodeRef::Taxonomy(taxonomy));
        if !expanded.contains(taxonomy.id()) {
            continue;
        }
        for commit_type in taxonomy.types() {
            rows.push(NodeRef::Type {
                taxonomy,
                commit_type,
            });
            if expanded.contains(commit_type.id()) {
                rows.extend(
                    commit_type
                        .properties()
                        .iter()
                        .map(|property| NodeRef::Property {
                            taxonomy,
                            commit_type,
                            property,
                        }),
                );
            }
        }
    }
    rows
}

pub(super) fn has_children(node: NodeRef<'_>) -> bool {
    match node {
        NodeRef::Taxonomy(taxonomy) => !taxonomy.types().is_empty(),
        NodeRef::Type { commit_type, .. } => !commit_type.properties().is_empty(),
        NodeRef::Property { .. } => false,
    }
}

/// The cursor's row, else the row of its nearest visible ancestor, else the first row.
pub(super) fn resolve(
    rows: &[NodeRef<'_>],
    cursor: Option<&NodeId>,
    library: &LibraryDraft,
) -> usize {
    let mut id = cursor.cloned();
    while let Some(current) = id {
        if let Some(index) = rows.iter().position(|row| row.id() == &current) {
            return index;
        }
        id = library.parent(&current);
    }
    0
}

/// Draws the rows and returns the area they fill, for mouse hit-testing.
pub(super) fn render(
    frame: &mut Frame<'_>,
    area: Rect,
    rows: &[NodeRef<'_>],
    expanded: &BTreeSet<NodeId>,
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
            let open = expanded.contains(row.id()) && has_children(row);
            row_line(row, open, index == selected, focused, width)
        })
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines), inner);
    inner
}

fn row_line(
    row: NodeRef<'_>,
    open: bool,
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
        NodeRef::Taxonomy(taxonomy) => (
            vec![
                bar,
                marker,
                Span::styled(taxonomy.name().to_owned(), name_style),
            ],
            vec![Span::styled(
                format!(" {} ", taxonomy.origin().as_str()),
                BADGE_STYLE,
            )],
        ),
        NodeRef::Type {
            taxonomy,
            commit_type,
        } => {
            let column = taxonomy
                .types()
                .iter()
                .map(|sibling| sibling.name().len())
                .max()
                .unwrap_or(0)
                + 2;
            let mut left = vec![
                bar,
                Span::raw("  "),
                marker,
                Span::styled(format!("{:<column$}", commit_type.name()), name_style),
            ];
            if !open {
                let summary = commit_type
                    .properties()
                    .iter()
                    .map(gitserious_app::PropertyNode::name)
                    .collect::<Vec<_>>()
                    .join(" · ");
                left.push(Span::styled(summary, DIM));
            }
            let count = Span::styled(commit_type.properties().len().to_string(), DIM);
            (left, vec![count])
        }
        NodeRef::Property {
            commit_type,
            property,
            ..
        } => {
            let siblings = commit_type.properties();
            let column = siblings
                .iter()
                .map(|sibling| sibling.name().len())
                .max()
                .unwrap_or(0)
                + 2;
            let last = siblings
                .last()
                .is_some_and(|last| last.id() == property.id());
            (
                vec![
                    bar,
                    Span::raw("    "),
                    Span::styled(if last { "└ " } else { "├ " }, frame_style()),
                    Span::styled(format!("{:<column$}", property.name()), name_style),
                    Span::styled(property.policy().label(), policy_style(property.policy())),
                ],
                Vec::new(),
            )
        }
    };
    let line = fit(left, right, width);
    if selected { line.style(BAND) } else { line }
}
