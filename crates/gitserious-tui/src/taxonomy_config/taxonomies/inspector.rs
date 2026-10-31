use std::ops::Range;

use gitserious_app::{DraftField, NodeRef, Policy, ProblemSite, TaxonomyOrigin};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use unicode_segmentation::UnicodeSegmentation;

use super::super::hanging_wrap;
use super::{BADGE_STYLE, BAND, Chips, DIM, KEY_STYLE, fit, policy_style};
use crate::theme::focus_frame_style;

/// Labels and the values beside them start in these columns.
const LABEL_WIDTH: usize = 13;
/// Room for the widest counter, `100/100`, and the gap before it.
const COUNTER_WIDTH: usize = 8;
const CURSOR: Style = Style::new().add_modifier(Modifier::REVERSED);
const PROBLEM: Style = Style::new().fg(Color::Red);
const PLACEHOLDER: Style = DIM.add_modifier(Modifier::ITALIC);
const PLACEHOLDER_TEXT: &str = "when does this apply?";

/// The field a row is focused on and, while it is edited, the cursor's grapheme index.
#[derive(Clone, Copy)]
pub(super) struct Focus {
    pub(super) field: DraftField,
    pub(super) cursor: Option<usize>,
}

/// The rows of the focused field: `band` is the field itself, and `view` adds the problems
/// under it, which scrolling keeps in view with it.
struct Focused {
    band: Range<usize>,
    view: Range<usize>,
}

/// Describes `node` in words under a header with its chip, scrolled down by `offset` rows, and
/// keeps the focused field and its problems in view.
pub(super) fn render(
    frame: &mut Frame<'_>,
    area: Rect,
    chips: &Chips<'_>,
    node: NodeRef<'_>,
    focus: Option<Focus>,
    offset: &mut u16,
) {
    let editing = focus.is_some_and(|focus| focus.cursor.is_some());
    let block = Block::bordered()
        .border_style(focus_frame_style(editing))
        .title_style(focus_frame_style(editing))
        .title("Inspector");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let width = usize::from(inner.width).saturating_sub(2);
    let (lines, focused) = describe(chips, node, focus, width, inner.height > 10);
    let total = u16::try_from(lines.len()).unwrap_or(u16::MAX);
    if let Some(Focused { view, .. }) = &focused {
        let (start, end) = (
            u16::try_from(view.start).unwrap_or(u16::MAX),
            u16::try_from(view.end).unwrap_or(u16::MAX),
        );
        if end > offset.saturating_add(inner.height) {
            *offset = end.saturating_sub(inner.height);
        }
        *offset = (*offset).min(start);
    }
    *offset = (*offset).min(total.saturating_sub(inner.height));
    let lines = lines
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            let band = focused
                .as_ref()
                .is_some_and(|focused| focused.band.contains(&index));
            let gutter = if band {
                Span::styled("▌", Style::default().fg(Color::Yellow))
            } else {
                Span::raw(" ")
            };
            let mut spans = vec![gutter];
            spans.extend(line.spans);
            let line = Line::from(spans);
            if band { line.style(BAND) } else { line }
        })
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines).scroll((*offset, 0)), inner);
}

/// The lines and the rows the focused field takes.
fn describe(
    chips: &Chips<'_>,
    node: NodeRef<'_>,
    focus: Option<Focus>,
    width: usize,
    roomy: bool,
) -> (Vec<Line<'static>>, Option<Focused>) {
    let library = chips.library;
    let chip = chips.of(node.id());
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
    let badge = match (chip, node) {
        (Some(chip), _) => vec![chip.span()],
        (None, NodeRef::Taxonomy(taxonomy)) => vec![Span::styled(
            format!(" {} ", taxonomy.origin().as_str()),
            BADGE_STYLE,
        )],
        (None, _) => Vec::new(),
    };
    lines.push(fit(heading, badge, width));
    match node {
        NodeRef::Taxonomy(_) => {}
        NodeRef::Type { taxonomy, .. } => {
            lines.push(Line::styled(format!("in {}", taxonomy.name()), DIM));
        }
        NodeRef::Property {
            taxonomy,
            commit_type,
            ..
        } => lines.push(Line::styled(
            format!("in {} › {}", taxonomy.name(), commit_type.name()),
            DIM,
        )),
    }
    lines.push(Line::default());
    let id = node.id();
    let editable = library.fields(id);
    let mut focused = None;
    let mut show = |lines: &mut Vec<Line<'static>>, field: DraftField| {
        let start = lines.len();
        let focus = focus.filter(|focus| focus.field == field);
        let problems = chips.sentences(&ProblemSite::Field(id.clone(), field));
        if field == DraftField::Policy {
            if let NodeRef::Property { property, .. } = node {
                lines.extend(policy_lines(property.policy(), width));
            }
        } else {
            let text = library.text(id, field).unwrap_or_default();
            let limit = field.limit().filter(|_| editable.contains(&field));
            lines.extend(field_lines(
                field,
                text,
                limit,
                focus.and_then(|focus| focus.cursor),
                !problems.is_empty(),
                width,
            ));
        }
        let band = start..lines.len();
        for problem in &problems {
            lines.extend(problem_lines(problem, width));
        }
        if focus.is_some() {
            focused = Some(Focused {
                band,
                view: start..lines.len(),
            });
        }
    };
    let fact = |lines: &mut Vec<Line<'static>>, label: &str, value: &str| {
        lines.extend(fact_lines(label, value, width));
    };
    match node {
        NodeRef::Taxonomy(taxonomy) => {
            if editable.contains(&DraftField::Name) {
                show(&mut lines, DraftField::Name);
            }
            let version = library
                .version(id)
                .map_or_else(String::new, |version| version.to_string());
            fact(&mut lines, "Version", &version);
            show(&mut lines, DraftField::Description);
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
            show(&mut lines, DraftField::Name);
            show(&mut lines, DraftField::Description);
            fact(
                &mut lines,
                "Properties",
                &commit_type.properties().len().to_string(),
            );
        }
        NodeRef::Property { property, .. } => {
            show(&mut lines, DraftField::Name);
            show(&mut lines, DraftField::Description);
            show(&mut lines, DraftField::Policy);
            if property.policy() == Policy::Conditional {
                show(&mut lines, DraftField::Condition);
                show(&mut lines, DraftField::Rationale);
            }
        }
    }
    for problem in chips.sentences(&ProblemSite::Node(id.clone())) {
        lines.extend(problem_lines(&problem, width));
    }
    if let NodeRef::Taxonomy(taxonomy) = node {
        lines.push(Line::default());
        if taxonomy.origin() == TaxonomyOrigin::BuiltIn {
            lines.push(hint("f", "fork to customize · built-ins are read-only"));
        }
        lines.push(hint("n", "new blank taxonomy"));
    }
    (lines, focused)
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

/// A text field: its label, its text wrapped in the value column with room for the cursor,
/// `used/limit` at the end of the first row when the field is editable, and the cursor. An
/// empty Condition shows a placeholder, and a field with a problem draws its label and counter
/// red.
fn field_lines(
    field: DraftField,
    text: &str,
    limit: Option<usize>,
    cursor: Option<usize>,
    invalid: bool,
    width: usize,
) -> Vec<Line<'static>> {
    let style = if invalid { PROBLEM } else { DIM };
    let graphemes = text
        .graphemes(true)
        .map(|grapheme| {
            if grapheme.contains(['\n', '\r']) {
                " "
            } else {
                grapheme
            }
        })
        .collect::<Vec<_>>();
    let counter = limit.map(|limit| format!("{}/{limit}", graphemes.len()));
    let column = width
        .saturating_sub(LABEL_WIDTH + usize::from(counter.is_some()) * COUNTER_WIDTH + 1)
        .max(1);
    let rows = wrap(&graphemes, column);
    let last = rows.len() - 1;
    rows.iter()
        .enumerate()
        .map(|(index, &(start, end))| {
            let label = if index == 0 { field.label() } else { "" };
            let mut spans = vec![Span::styled(format!("{label:<LABEL_WIDTH$}"), style)];
            let at = cursor.filter(|&at| {
                (start..end).contains(&at)
                    || (at == end && (index == last || rows[index + 1].0 > at))
            });
            match at {
                _ if graphemes.is_empty() && field == DraftField::Condition => {
                    let (first, rest) = PLACEHOLDER_TEXT.split_at(1);
                    if at.is_some() {
                        spans.push(Span::styled(first, CURSOR.patch(PLACEHOLDER)));
                        spans.push(Span::styled(rest, PLACEHOLDER));
                    } else {
                        spans.push(Span::styled(PLACEHOLDER_TEXT, PLACEHOLDER));
                    }
                }
                Some(at) if at < end => {
                    spans.push(Span::raw(graphemes[start..at].concat()));
                    spans.push(Span::styled(graphemes[at].to_owned(), CURSOR));
                    spans.push(Span::raw(graphemes[at + 1..end].concat()));
                }
                Some(_) => {
                    spans.push(Span::raw(graphemes[start..end].concat()));
                    spans.push(Span::styled(" ", CURSOR));
                }
                None => spans.push(Span::raw(graphemes[start..end].concat())),
            }
            match counter.as_ref().filter(|_| index == 0) {
                Some(counter) => fit(spans, vec![Span::styled(counter.clone(), style)], width),
                None => Line::from(spans),
            }
        })
        .collect()
}

/// `▲ problem` in the value column, wrapped beneath its own first word.
fn problem_lines(problem: &str, width: usize) -> Vec<Line<'static>> {
    let graphemes = problem.graphemes(true).collect::<Vec<_>>();
    let column = width.saturating_sub(LABEL_WIDTH + 2).max(1);
    wrap(&graphemes, column)
        .into_iter()
        .enumerate()
        .map(|(index, (start, end))| {
            let mark = if index == 0 { "▲ " } else { "  " };
            Line::from(vec![
                Span::raw(" ".repeat(LABEL_WIDTH)),
                Span::styled(format!("{mark}{}", graphemes[start..end].concat()), PROBLEM),
            ])
        })
        .collect()
}

/// Splits `graphemes` into rows of at most `width` columns, breaking after the last space that
/// fits and dropping it, or inside a word longer than a row. Returns each row's range.
fn wrap(graphemes: &[&str], width: usize) -> Vec<(usize, usize)> {
    let cell = |grapheme: &str| Span::raw(grapheme.to_owned()).width();
    let mut rows = Vec::new();
    let mut start = 0;
    let mut used = 0;
    let mut index = 0;
    while index < graphemes.len() {
        let size = cell(graphemes[index]);
        if used + size > width && index > start {
            let space = if graphemes[index] == " " {
                Some(index)
            } else {
                (start + 1..index).rev().find(|&at| graphemes[at] == " ")
            };
            let (end, next) = space.map_or((index, index), |at| (at, at + 1));
            rows.push((start, end));
            start = next;
            index = index.max(next);
            used = graphemes[start..index].iter().map(|g| cell(g)).sum();
            continue;
        }
        used += size;
        index += 1;
    }
    rows.push((start, graphemes.len()));
    rows
}

/// Every policy in order, the current one highlighted, wrapping within the value column.
fn policy_lines(current: Policy, width: usize) -> Vec<Line<'static>> {
    let column = width.saturating_sub(LABEL_WIDTH);
    let mut lines = Vec::new();
    let mut spans = vec![Span::styled(
        format!("{:<LABEL_WIDTH$}", DraftField::Policy.label()),
        DIM,
    )];
    let mut used = 0;
    for policy in Policy::ORDER {
        let label = policy.label();
        if used > 0 && used + 1 + label.len() > column {
            lines.push(Line::from(std::mem::replace(
                &mut spans,
                vec![Span::raw(" ".repeat(LABEL_WIDTH))],
            )));
            used = 0;
        }
        if used > 0 {
            spans.push(Span::raw(" "));
            used += 1;
        }
        spans.push(if policy == current {
            Span::styled(
                label,
                policy_style(policy)
                    .bg(Color::Rgb(60, 60, 60))
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled(label, DIM)
        });
        used += label.len();
    }
    lines.push(Line::from(spans));
    lines
}

fn hint(key: &'static str, label: &'static str) -> Line<'static> {
    Line::from(vec![
        Span::styled(key, KEY_STYLE),
        Span::styled(format!(" {label}"), DIM),
    ])
}

#[cfg(test)]
mod tests {
    use super::wrap;

    #[test]
    fn wrap_breaks_after_the_last_space_that_fits_and_inside_long_words() {
        let words = "ab cd efghij"
            .split("")
            .filter(|g| !g.is_empty())
            .collect::<Vec<_>>();
        assert_eq!(wrap(&words, 5), [(0, 5), (6, 11), (11, 12)]);
        assert_eq!(wrap(&[], 5), [(0, 0)]);
    }
}
