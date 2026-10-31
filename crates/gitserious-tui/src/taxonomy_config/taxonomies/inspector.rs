use gitserious_app::{DraftField, LibraryDraft, NodeRef, Policy, TaxonomyOrigin};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use super::super::hanging_wrap;
use super::{BADGE_STYLE, DIM, KEY_STYLE, fit, policy_style};
use crate::theme::frame_style;

/// Labels and the values beside them start in these columns.
const LABEL_WIDTH: usize = 13;

/// Describes `node` in words, scrolled down by `offset` rows.
pub(super) fn render(
    frame: &mut Frame<'_>,
    area: Rect,
    library: &LibraryDraft,
    node: NodeRef<'_>,
    offset: &mut u16,
) {
    let block = Block::bordered()
        .border_style(frame_style())
        .title_style(frame_style())
        .title("Inspector");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let body = Rect::new(
        inner.x.saturating_add(1),
        inner.y,
        inner.width.saturating_sub(2),
        inner.height,
    );
    let lines = describe(library, node, usize::from(body.width), body.height > 10);
    let total = u16::try_from(lines.len()).unwrap_or(u16::MAX);
    *offset = (*offset).min(total.saturating_sub(body.height));
    frame.render_widget(Paragraph::new(lines).scroll((*offset, 0)), body);
}

fn describe(
    library: &LibraryDraft,
    node: NodeRef<'_>,
    width: usize,
    roomy: bool,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if roomy {
        lines.push(Line::default());
    }
    let (kind, name) = match node {
        NodeRef::Taxonomy(taxonomy) => ("Taxonomy", taxonomy.name()),
        NodeRef::Type { commit_type, .. } => ("Type", commit_type.name()),
        NodeRef::Property { property, .. } => ("Property", property.name()),
    };
    let heading = vec![
        Span::styled(format!("{kind} "), DIM),
        Span::styled(
            name.to_owned(),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
    ];
    let badge = match node {
        NodeRef::Taxonomy(taxonomy) => {
            vec![Span::styled(
                format!(" {} ", taxonomy.origin().as_str()),
                BADGE_STYLE,
            )]
        }
        _ => Vec::new(),
    };
    lines.push(fit(heading, badge, width));
    if !matches!(node, NodeRef::Taxonomy(_)) {
        let mut names = node.names();
        names.pop();
        lines.push(Line::styled(format!("in {}", names.join(" › ")), DIM));
    }
    lines.push(Line::default());
    let fact = |lines: &mut Vec<Line<'static>>, label: &str, value: &str| {
        lines.extend(fact_lines(label, value, width));
    };
    match node {
        NodeRef::Taxonomy(taxonomy) => {
            let version = library
                .version(taxonomy.id())
                .map_or_else(String::new, |version| version.to_string());
            fact(&mut lines, "Version", &version);
            fact(
                &mut lines,
                DraftField::Description.label(),
                taxonomy.description(),
            );
            fact(&mut lines, "Types", &taxonomy.types().len().to_string());
            if let Some(source) = taxonomy.derived_from() {
                fact(
                    &mut lines,
                    "Forked from",
                    &format!("{}@{}", source.id(), source.version()),
                );
            }
        }
        NodeRef::Type { commit_type, .. } => {
            fact(&mut lines, DraftField::Name.label(), commit_type.name());
            fact(
                &mut lines,
                DraftField::Description.label(),
                commit_type.description(),
            );
            fact(
                &mut lines,
                "Properties",
                &commit_type.properties().len().to_string(),
            );
        }
        NodeRef::Property { property, .. } => {
            fact(&mut lines, DraftField::Name.label(), property.name());
            fact(
                &mut lines,
                DraftField::Description.label(),
                property.description(),
            );
            lines.push(policy_line(property.policy()));
            if property.policy() == Policy::Conditional {
                fact(
                    &mut lines,
                    DraftField::Condition.label(),
                    property.condition(),
                );
                fact(
                    &mut lines,
                    DraftField::Rationale.label(),
                    property.rationale(),
                );
            }
        }
    }
    if let NodeRef::Taxonomy(taxonomy) = node {
        lines.push(Line::default());
        if taxonomy.origin() == TaxonomyOrigin::BuiltIn {
            lines.push(hint("f", "fork to customize · built-ins are read-only"));
        }
        lines.push(hint("n", "new blank taxonomy"));
    }
    lines
}

/// `label` in the label column and `value` wrapped beneath its own first column.
fn fact_lines(label: &str, value: &str, width: usize) -> Vec<Line<'static>> {
    let text = format!("{label:<LABEL_WIDTH$}{}", value.replace('\n', " "));
    hanging_wrap(&text, width)
        .into_iter()
        .map(|row| {
            let row = row.to_string();
            let split = row
                .char_indices()
                .nth(LABEL_WIDTH)
                .map_or(row.len(), |(at, _)| at);
            let (label, value) = row.split_at(split);
            Line::from(vec![
                Span::styled(label.to_owned(), DIM),
                Span::raw(value.to_owned()),
            ])
        })
        .collect()
}

/// Every policy in order, the current one highlighted.
fn policy_line(current: Policy) -> Line<'static> {
    let mut spans = vec![Span::styled(
        format!("{:<LABEL_WIDTH$}", DraftField::Policy.label()),
        DIM,
    )];
    for (index, policy) in Policy::ORDER.into_iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw(" "));
        }
        spans.push(if policy == current {
            Span::styled(
                policy.label(),
                policy_style(policy)
                    .bg(Color::Rgb(40, 40, 40))
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled(policy.label(), DIM)
        });
    }
    Line::from(spans)
}

fn hint(key: &'static str, label: &'static str) -> Line<'static> {
    Line::from(vec![
        Span::styled(key, KEY_STYLE),
        Span::styled(format!(" {label}"), DIM),
    ])
}
