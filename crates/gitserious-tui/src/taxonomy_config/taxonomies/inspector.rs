use std::ops::Range;

use gitserious_app::{DraftField, NodeRef, Policy, ProblemSite, PropertyNode, TaxonomyOrigin};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_segmentation::UnicodeSegmentation;

use super::super::hanging_wrap;
use super::{BAND, Chips, DIM, KEY_STYLE, fit, panes, policy_style};
use crate::theme::theme;

/// Labels and the values beside them start in these columns.
const LABEL_WIDTH: usize = 13;
const CURSOR: Style = Style::new().add_modifier(Modifier::REVERSED);
const PROBLEM: Style = Style::new().fg(theme().blocks);
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

/// Describes `node` in words under a header with its badge, scrolled down by `offset` rows, and
/// returns how many rows it shows. The focused field never sits below the view, and while it is
/// edited it stays in view with its problems; browsing can scroll through the remaining
/// details. When they overflow, the last row counts the rows below.
pub(super) fn render(
    frame: &mut Frame<'_>,
    area: Rect,
    chips: &Chips<'_>,
    node: NodeRef<'_>,
    focus: Option<Focus>,
    offset: &mut u16,
) -> u16 {
    let editing = focus.is_some_and(|focus| focus.cursor.is_some());
    let width = usize::from(area.width).saturating_sub(2);
    let (lines, focused) = describe(chips, node, focus, width);
    let total = u16::try_from(lines.len()).unwrap_or(u16::MAX);
    let inner = Rect {
        height: panes::shown(lines.len(), area.height),
        ..area
    };
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
    panes::render_more_below(
        frame,
        area,
        usize::from(total.saturating_sub(offset.saturating_add(inner.height))),
    );
    let lines = lines
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            let band = focused
                .as_ref()
                .is_some_and(|focused| focused.band.contains(&index));
            let gutter = if band {
                Span::styled("▌", Style::default().fg(theme().pending))
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
    inner.height
}

/// The lines and the rows the focused field takes. A focus means EDIT: a type or property lays
/// out every field to edit in turn. Browsing drops the name the heading already shows; a
/// property leads with its policy, and a type lists its property guidance.
fn describe(
    chips: &Chips<'_>,
    node: NodeRef<'_>,
    focus: Option<Focus>,
    width: usize,
) -> (Vec<Line<'static>>, Option<Focused>) {
    let library = chips.library;
    let chip = chips.of(node.id());
    let editing = focus.is_some();
    let built_in = node.taxonomy().origin() == TaxonomyOrigin::BuiltIn;
    let mut lines = Vec::new();
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
                .fg(theme().text)
                .add_modifier(Modifier::BOLD),
        ),
    ];
    let badge = match chip {
        _ if editing => vec![Span::styled("EDIT", Style::new().fg(theme().pending))],
        _ if chips.read_only || built_in => vec![Span::styled("◇ read-only", DIM)],
        Some(chip) => vec![chip.span()],
        None => Vec::new(),
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
                lines.extend(policy_lines(property.policy(), editing, width));
            }
        } else {
            let text = library.text(id, field).unwrap_or_default();
            lines.extend(field_lines(
                field,
                text,
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
            let creating = editing
                && editable.contains(&DraftField::Name)
                && library.saved_taxonomy(id).is_none()
                && taxonomy.derived_from().is_none();
            if !chips.read_only && editable.contains(&DraftField::Name) {
                show(&mut lines, DraftField::Name);
            }
            show(&mut lines, DraftField::Description);
            if !creating {
                fact(&mut lines, "Types", &taxonomy.types().len().to_string());
            }
            if let Some(source) = taxonomy.derived_from() {
                fact(&mut lines, "Forked from", &source.reference().to_string());
            }
            if !creating && !built_in && !library.is_project() {
                let record = library.saved_taxonomy(id);
                for (label, date) in [
                    (
                        "Created on",
                        record.and_then(gitserious_app::LibraryTaxonomy::created_on),
                    ),
                    (
                        "Modified on",
                        record.and_then(gitserious_app::LibraryTaxonomy::modified_on),
                    ),
                ] {
                    fact(
                        &mut lines,
                        label,
                        &date.map_or_else(|| "Unknown".into(), |date| date.to_string()),
                    );
                }
            }
            if !creating && let Some(version) = library.version(id) {
                fact(&mut lines, "Version", &version.to_string());
            }
            if chip.is_some() {
                let sentence = "Add a type before this taxonomy can be included in a project."
                    .graphemes(true)
                    .collect::<Vec<_>>();
                lines.push(Line::default());
                lines.extend(
                    wrap(&sentence, width.max(1))
                        .into_iter()
                        .map(|(start, end)| Line::styled(sentence[start..end].concat(), PROBLEM)),
                );
            }
        }
        NodeRef::Type { commit_type, .. } => {
            if editing {
                show(&mut lines, DraftField::Name);
                show(&mut lines, DraftField::Description);
                fact(
                    &mut lines,
                    "Properties",
                    &commit_type.properties().len().to_string(),
                );
            } else {
                show(&mut lines, DraftField::Description);
                lines.extend(property_guidance_lines(commit_type.properties(), width));
            }
        }
        NodeRef::Property { property, .. } => {
            let conditional = property.policy() == Policy::Conditional;
            if editing {
                show(&mut lines, DraftField::Name);
                show(&mut lines, DraftField::Description);
                show(&mut lines, DraftField::Policy);
                if conditional {
                    show(&mut lines, DraftField::Condition);
                    show(&mut lines, DraftField::Rationale);
                }
            } else {
                show(&mut lines, DraftField::Policy);
                if conditional {
                    show(&mut lines, DraftField::Condition);
                }
                show(&mut lines, DraftField::Description);
                if conditional {
                    show(&mut lines, DraftField::Rationale);
                }
            }
        }
    }
    if !chips.read_only && built_in && !matches!(node, NodeRef::Property { .. }) {
        lines.push(Line::default());
        lines.push(hint("f", "fork to customize"));
    }
    (lines, focused)
}

fn property_guidance_lines(properties: &[PropertyNode], width: usize) -> Vec<Line<'static>> {
    if properties.is_empty() {
        return Vec::new();
    }
    let available = width.saturating_sub(6);
    let name_width = properties
        .iter()
        .map(|property| Span::raw(property.name()).width())
        .max()
        .unwrap_or(8)
        .max(8)
        .min((available / 4).max(1));
    let policy_width = "recommended".len().min((available / 3).max(1));
    let widths = [
        name_width,
        policy_width,
        available.saturating_sub(name_width + policy_width).max(1),
    ];
    let mut lines = vec![Line::default()];
    let header = (["Property", "Policy", "Description"], [DIM; 3]);
    let rows = std::iter::once(header).chain(properties.iter().map(|property| {
        (
            [
                property.name(),
                property.policy().label(),
                property.description(),
            ],
            [Style::default(), policy_style(property.policy()), DIM],
        )
    }));
    for (index, (cells, styles)) in rows.enumerate() {
        let wrapped = std::array::from_fn::<_, 3, _>(|column| {
            let text = cells[column].replace(['\n', '\r'], " ");
            let graphemes = text.graphemes(true).collect::<Vec<_>>();
            wrap(&graphemes, widths[column])
                .into_iter()
                .map(|(start, end)| graphemes[start..end].concat())
                .collect::<Vec<_>>()
        });
        let height = wrapped.iter().map(Vec::len).max().unwrap_or(1);
        for row in 0..height {
            let mut spans = Vec::new();
            for column in 0..3 {
                if column > 0 {
                    spans.push(Span::styled(" │ ", DIM));
                }
                let text = wrapped[column].get(row).cloned().unwrap_or_default();
                let padding = widths[column].saturating_sub(Span::raw(&text).width());
                spans.push(Span::styled(text, styles[column]));
                spans.push(Span::raw(" ".repeat(padding)));
            }
            lines.push(Line::from(spans));
        }
        if index == 0 {
            lines.push(Line::styled(
                widths.map(|width| "─".repeat(width)).join("─┼─"),
                DIM,
            ));
        }
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

/// A text field: its label and value, wrapped with room for the cursor. An empty Condition shows a placeholder, and a field with a problem draws
/// its label red. The footer owns the focused field's character count.
fn field_lines(
    field: DraftField,
    text: &str,
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
    let column = width.saturating_sub(LABEL_WIDTH + 1).max(1);
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
            Line::from(spans)
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

use super::text_layout::wrap;

/// The current policy as plain text, or in EDIT every policy in order with the current one bold
/// and underlined, wrapping within the value column.
fn policy_lines(current: Policy, editing: bool, width: usize) -> Vec<Line<'static>> {
    let mut spans = vec![Span::styled(
        format!("{:<LABEL_WIDTH$}", DraftField::Policy.label()),
        DIM,
    )];
    if !editing {
        spans.push(Span::styled(current.label(), policy_style(current)));
        return vec![Line::from(spans)];
    }
    let column = width.saturating_sub(LABEL_WIDTH);
    let mut lines = Vec::new();
    let mut used = 0;
    for policy in Policy::ORDER {
        let label = policy.label();
        if used > 0 && used + 2 + label.len() > column {
            lines.push(Line::from(std::mem::replace(
                &mut spans,
                vec![Span::raw(" ".repeat(LABEL_WIDTH))],
            )));
            used = 0;
        }
        if used > 0 {
            spans.push(Span::raw("  "));
            used += 2;
        }
        spans.push(if policy == current {
            Span::styled(
                label,
                policy_style(policy).add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )
        } else {
            Span::styled(label, policy_style(policy))
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
    fn taxonomy_details_end_with_the_version_after_description_types_and_lineage()
    -> Result<(), Box<dyn std::error::Error>> {
        use super::{Chips, Focus, describe};
        use gitserious_app::{DraftField, GlobalConfiguration, LibraryDraft};
        let mut library = LibraryDraft::from_configuration(GlobalConfiguration::default());
        let source = library.find_path(&["conventional"]).ok_or("source")?;
        library.fork(&source)?;
        for owner in ["conventional", "conventional-fork"] {
            let id = library.find_path(&[owner]).ok_or("taxonomy")?;
            let node = library.node(&id).ok_or("node")?;
            for width in [28, 80] {
                for read_only in [false, true] {
                    for focus in [
                        None,
                        Some(Focus {
                            field: DraftField::Description,
                            cursor: Some(0),
                        }),
                    ] {
                        let (lines, _) =
                            describe(&Chips::new(&library, read_only), node, focus, width);
                        let rows = lines.iter().map(ToString::to_string).collect::<Vec<_>>();
                        let at = |label: &str| rows.iter().position(|row| row.starts_with(label));
                        let description = at("Description").ok_or("description")?;
                        let types = at("Types").ok_or("types")?;
                        let version = at("Version");
                        if owner == "conventional-fork" {
                            let created = at("Created on").ok_or("created date")?;
                            let modified = at("Modified on").ok_or("modified date")?;
                            assert!(types < created && created < modified, "{rows:?}");
                            assert!(rows[created].ends_with("Unknown"));
                            assert!(rows[modified].ends_with("Unknown"));
                        } else {
                            assert_eq!(at("Created on"), None);
                            assert_eq!(at("Modified on"), None);
                            assert!(version.is_some_and(|version| types < version));
                        }
                        assert!(description < types, "{rows:?}");
                        if owner == "conventional-fork" {
                            assert!(
                                at("Forked from").ok_or("lineage")?
                                    < at("Created on").ok_or("created")?
                            );
                            assert_eq!(version, None);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    #[test]
    fn saved_library_dates_display_the_recorded_calendar_days()
    -> Result<(), Box<dyn std::error::Error>> {
        use super::{Chips, describe};
        use gitserious_app::{DraftEdit, DraftField, GlobalConfiguration, LibraryDraft};
        let mut library = LibraryDraft::from_configuration(GlobalConfiguration::default());
        let source = library.find_path(&["conventional"]).ok_or("source")?;
        let id = library.fork(&source)?;
        let mut library = library
            .saved_on("2024-02-29".parse()?)
            .map_err(|problems| format!("{problems:?}"))?;
        library.edit(
            &id,
            DraftEdit::Text(DraftField::Description, "Updated purpose"),
        )?;
        let library = library
            .saved_on("2026-11-07".parse()?)
            .map_err(|problems| format!("{problems:?}"))?;
        for width in [28, 80] {
            let (lines, _) = describe(
                &Chips::new(&library, false),
                library.node(&id).ok_or("taxonomy")?,
                None,
                width,
            );
            let text = lines
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n");
            assert!(
                text.lines()
                    .any(|line| line.starts_with("Created on") && line.ends_with("2024-02-29")),
                "{text}"
            );
            assert!(
                text.lines()
                    .any(|line| line.starts_with("Modified on") && line.ends_with("2026-11-07")),
                "{text}"
            );
        }
        Ok(())
    }

    #[test]
    fn property_details_have_no_example_or_preview_in_browsing_or_editing()
    -> Result<(), Box<dyn std::error::Error>> {
        use super::{Chips, Focus, describe};
        use gitserious_app::{DraftField, GlobalConfiguration, LibraryDraft};
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
                    let (lines, _) = describe(&Chips::new(&library, false), node, focus, width);
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
    fn type_property_table_keeps_each_description_in_its_column()
    -> Result<(), Box<dyn std::error::Error>> {
        use gitserious_app::{GlobalConfiguration, LibraryDraft, NodeRef};
        let library = LibraryDraft::from_configuration(GlobalConfiguration::default());
        let id = library.find_path(&["conventional", "feat"]).ok_or("type")?;
        let NodeRef::Type { commit_type, .. } = library.node(&id).ok_or("node")? else {
            return Err("expected type".into());
        };
        assert!(super::property_guidance_lines(&[], 80).is_empty());
        for width in [40, 52, 80] {
            for property in commit_type.properties() {
                let lines = super::property_guidance_lines(std::slice::from_ref(property), width);
                assert!(lines.iter().all(|line| line.width() <= width));
                let rows = lines.iter().map(ToString::to_string).collect::<Vec<_>>();
                assert_eq!(
                    rows[1].split(" │ ").map(str::trim).collect::<Vec<_>>(),
                    ["Property", "Policy", "Description"]
                );
                let cells = rows[3..]
                    .iter()
                    .map(|row| row.split(" │ ").map(str::trim).collect::<Vec<_>>())
                    .collect::<Vec<_>>();
                assert!(cells.iter().all(|row| row.len() == 3));
                assert_eq!(
                    cells.iter().map(|row| row[0]).collect::<String>(),
                    property.name()
                );
                assert_eq!(
                    cells.iter().map(|row| row[1]).collect::<String>(),
                    property.policy().label()
                );
                assert_eq!(
                    cells
                        .iter()
                        .map(|row| row[2])
                        .collect::<Vec<_>>()
                        .join(" ")
                        .trim(),
                    property.description()
                );
                assert!(
                    !cells[0][2].is_empty(),
                    "description starts beside its property"
                );
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
