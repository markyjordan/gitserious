use std::ops::Range;

use gitserious_app::{DraftField, NodeRef, Policy, ProblemSite, TaxonomyOrigin};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use unicode_segmentation::UnicodeSegmentation;

use super::super::hanging_wrap;
use super::{BAND, CUSTOM_STYLE, Chips, DIM, KEY_STYLE, READ_ONLY_STYLE, fit, policy_style};
use crate::theme::{ACCENT, AMBER, block_title, focus_frame_style};

/// Labels and the values beside them start in these columns.
const LABEL_WIDTH: usize = 13;
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
    cursor: Option<usize>,
}

/// Describes `node` in words under a header with its chip, scrolled down by `offset` rows. The
/// focused field never sits below the view, and while it is edited it stays in view with its
/// problems; browsing can scroll through the remaining details.
pub(super) fn render(
    frame: &mut Frame<'_>,
    area: Rect,
    chips: &Chips<'_>,
    node: NodeRef<'_>,
    focus: Option<Focus>,
    active: bool,
    offset: &mut u16,
) {
    let editing = focus.is_some_and(|focus| focus.cursor.is_some());
    let frame_style = if active && editing {
        Style::new().fg(AMBER)
    } else {
        focus_frame_style(active)
    };
    let block = Block::bordered()
        .border_style(frame_style)
        .title_style(frame_style)
        .title(block_title("Inspector"));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let width = usize::from(inner.width).saturating_sub(2);
    let (lines, focused) = describe(chips, node, focus, width, inner.height > 10);
    let total = u16::try_from(lines.len()).unwrap_or(u16::MAX);
    if let Some(Focused { view, cursor, .. }) = &focused {
        let (start, end) = (
            u16::try_from(view.start).unwrap_or(u16::MAX),
            u16::try_from(view.end).unwrap_or(u16::MAX),
        );
        if end > offset.saturating_add(inner.height) {
            *offset = end.saturating_sub(inner.height);
        }
        if editing {
            if end.saturating_sub(start) <= inner.height {
                *offset = (*offset).min(start);
            } else if let Some(cursor) = cursor {
                let cursor = u16::try_from(*cursor).unwrap_or(u16::MAX);
                if cursor < *offset {
                    *offset = cursor;
                } else if cursor >= offset.saturating_add(inner.height) {
                    *offset = cursor.saturating_add(1).saturating_sub(inner.height);
                }
            }
        }
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
                Span::styled("▌", Style::default().fg(ACCENT))
            } else {
                Span::raw(" ")
            };
            let style = if band { BAND } else { line.style };
            let mut spans = vec![gutter];
            spans.extend(line.spans);
            Line::from(spans).style(style)
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
        _ if node.taxonomy().origin() == TaxonomyOrigin::BuiltIn => {
            vec![Span::styled(" ◇ read-only ", READ_ONLY_STYLE)]
        }
        (Some(chip), _) => vec![chip.span()],
        (None, NodeRef::Taxonomy(taxonomy)) => vec![Span::styled(
            format!(
                " {} ",
                super::outline::origin_label(library, taxonomy.origin())
            ),
            CUSTOM_STYLE,
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
            property,
        } => {
            let mut context = format!("in {} › {}", taxonomy.name(), commit_type.name());
            if let Some(NodeRef::Type { commit_type, .. }) = property
                .copied_from()
                .and_then(|source| library.node(source))
            {
                context.push_str(&format!("  · copied from {}", commit_type.name()));
            }
            lines.push(Line::styled(context, DIM));
        }
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
            let cursor = lines[start..band.end]
                .iter()
                .position(|line| {
                    line.spans
                        .iter()
                        .any(|span| span.style.add_modifier.contains(Modifier::REVERSED))
                })
                .map(|row| start + row);
            focused = Some(Focused {
                cursor,
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
            if chip.is_some() {
                lines.extend(problem_lines(
                    "Add a type before this taxonomy can be included in a project.",
                    width,
                ));
            }
            if let Some(source) = taxonomy.derived_from() {
                fact(&mut lines, "Forked from", &source.versioned().to_string());
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
    let column = value_width(width, limit);
    let rows = wrap(&graphemes, column);
    let caret_row = cursor.map(|cursor| super::text_layout::cursor_row(&rows, cursor));
    rows.iter()
        .enumerate()
        .map(|(index, &(start, end))| {
            let label = if index == 0 { field.label() } else { "" };
            let mut spans = vec![Span::styled(format!("{label:<LABEL_WIDTH$}"), style)];
            let at = cursor.filter(|_| caret_row == Some(index));
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

pub(super) fn value_width(width: usize, limit: Option<usize>) -> usize {
    let counter = limit.map_or(0, |limit| format!("{limit}/{limit}").len().max(7) + 1);
    width.saturating_sub(LABEL_WIDTH + counter + 1).max(1)
}

use super::text_layout::wrap;

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
    use crate::library_lookup::LibraryLookup;

    use super::wrap;

    #[test]
    fn property_details_have_no_example_or_preview_in_browsing_or_editing()
    -> Result<(), Box<dyn std::error::Error>> {
        use super::{Chips, Focus, describe};
        use gitserious_app::{DraftField, GlobalConfiguration, LibraryDraft};
        use std::collections::BTreeSet;
        let mut library = LibraryDraft::from_configuration(GlobalConfiguration::default());
        let source = library.find_path(&["conventional"]).ok_or("source")?;
        library.fork(&source)?;
        for owner in ["conventional", "conventional-fork"] {
            let id = library
                .find_path(&[owner, "feat", "intent"])
                .ok_or("property")?;
            let node = library.node(&id).ok_or("node")?;
            for width in [28, 80] {
                for focus in [
                    None,
                    Some(Focus {
                        field: DraftField::Name,
                        cursor: Some(0),
                    }),
                ] {
                    let expanded = BTreeSet::new();
                    let (lines, _) =
                        describe(&Chips::new(&library, &expanded), node, focus, width, true);
                    let text = lines
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join("\n");
                    assert!(text.contains("Description"));
                    assert!(text.contains("Policy"));
                    assert!(!text.contains("Example"));
                    assert!(!text.contains("Preview"));
                    for action in ["e edit", "c copy", "d delete"] {
                        assert!(!text.contains(action), "{text}");
                    }
                }
            }
        }
        Ok(())
    }

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
