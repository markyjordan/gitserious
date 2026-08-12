use gitserious_core::{CommitTypeDefinition, PropertyRequirement};
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Cell, Clear, List, ListItem, ListState, Paragraph, Row, Table, Widget, Wrap,
};

use super::state::{AuthoringSession, ConfirmationAction, FieldId, FieldKind, FieldStatus, Stage};

const MINIMUM_WIDTH: u16 = 60;
const MINIMUM_HEIGHT: u16 = 18;
const MAX_EDITOR_INNER_WIDTH: u16 = 80;
const FIELD_COLUMN_SPACING: u16 = 2;
const FIELD_MARKER_WIDTH: u16 = 1;
const FIELD_REQUIREMENT_WIDTH: u16 = 11;
const MINIMUM_EDITOR_HEIGHT: u16 = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CursorStatus {
    column: u16,
    wrap_width: u16,
}

pub(crate) fn render(frame: &mut Frame<'_>, session: &mut AuthoringSession<'_>) {
    let area = frame.area();
    session.too_small = area.width < MINIMUM_WIDTH || area.height < MINIMUM_HEIGHT;
    if session.too_small {
        if session.stage == Stage::Confirm {
            render_centered_notice(
                frame,
                area,
                "Confirm discard",
                "Discard this draft?",
                Some("y: discard · Enter/n: keep editing"),
            );
        } else {
            render_centered_notice(
                frame,
                area,
                "Terminal too small",
                "Resize or press Esc/q to cancel",
                None,
            );
        }
        return;
    }

    match session.visible_stage() {
        Stage::SelectType => render_picker(frame, area, session),
        Stage::Compose => render_composer(frame, area, session),
        Stage::Review => render_review(frame, area, session),
        Stage::Confirm => unreachable!("confirmation resolves to its underlying stage"),
    }
    if session.stage == Stage::Confirm {
        render_confirmation(frame, area, session);
    }
}

fn render_picker(frame: &mut Frame<'_>, area: Rect, session: &AuthoringSession<'_>) {
    let sections = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(area);
    render_stage_header(frame, sections[0], "Select commit type", 1);
    let items = session
        .definitions
        .iter()
        .map(|definition| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    definition.id().as_str(),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(" — "),
                Span::raw(definition.description()),
            ]))
        })
        .collect::<Vec<_>>();
    let list = List::new(items)
        .highlight_symbol("› ")
        .highlight_style(navigation_key_style());
    let mut state = ListState::default();
    state.select(Some(session.selected_type));
    frame.render_stateful_widget(list, sections[2], &mut state);
    render_navigation_row(
        frame,
        sections[3],
        &[("↑/↓", "move"), ("Enter", "select"), ("Esc/q", "cancel")],
        None,
    );
}

fn render_composer(frame: &mut Frame<'_>, area: Rect, session: &mut AuthoringSession<'_>) {
    let desired_context_height = context_height(area, session);
    let maximum_context_height = area
        .height
        .saturating_sub(2 + 1 + 1 + 1 + MINIMUM_EDITOR_HEIGHT + 1 + 1);
    let context_height = desired_context_height.min(maximum_context_height).max(1);
    let sections = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Length(context_height),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(MINIMUM_EDITOR_HEIGHT),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);
    let header =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(sections[0]);
    render_stage_header(frame, header[0], "Compose commit message", 2);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("Type: "),
            Span::styled(
                session.definition().id().as_str(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ])),
        header[1],
    );

    render_field_context(frame, sections[2], session);
    render_section_heading(frame, sections[4], "Message form");
    let cursor_status = render_document_editor(frame, sections[5], session);

    let help: &[_] = &[("↑/↓", "move"), ("esc", "back"), ("ctrl+s", "review")];
    if let Some(issue) = session.composer.issues.first() {
        frame.render_widget(
            Paragraph::new(issue.message.as_str()).style(Style::default().fg(Color::Red)),
            sections[6],
        );
    }
    render_navigation_row(
        frame,
        sections[7],
        help,
        Some(&format!(
            "col {}/{}",
            cursor_status.column, cursor_status.wrap_width
        )),
    );
}

fn render_field_context(frame: &mut Frame<'_>, area: Rect, session: &AuthoringSession<'_>) {
    let sections =
        Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)]).split(area);
    render_field_hud(frame, sections[0], session);
    render_field_description(frame, sections[1], session);
}

fn render_field_hud(frame: &mut Frame<'_>, area: Rect, session: &AuthoringSession<'_>) {
    let definition = session.definition();
    let current = session
        .composer
        .current_field(definition)
        .map(FieldKind::id);
    let fields = session.composer.hud_fields(definition);
    let fixed_width =
        FIELD_MARKER_WIDTH + FIELD_REQUIREMENT_WIDTH + FIELD_COLUMN_SPACING.saturating_mul(2);
    let name_width = fields
        .iter()
        .map(|field| field_columns(field.id, definition).0.chars().count())
        .max()
        .and_then(|width| u16::try_from(width).ok())
        .unwrap_or(1)
        .min(area.width.saturating_sub(fixed_width).max(1));
    let rows = fields
        .into_iter()
        .map(|field| {
            let marker = match field.status {
                FieldStatus::Invalid => Cell::from("!").style(Style::default().fg(Color::Red)),
                FieldStatus::Complete => Cell::from("✓").style(Style::default().fg(Color::Green)),
                FieldStatus::Incomplete => {
                    Cell::from("○").style(Style::default().fg(Color::DarkGray))
                }
            };
            let style = if current == Some(field.id) {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let (name, requirement) = field_columns(field.id, definition);
            Row::new(vec![
                marker,
                Cell::from(name).style(style),
                Cell::from(requirement).style(style),
            ])
        })
        .collect::<Vec<_>>();
    let sections = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(area);
    render_section_heading(frame, sections[0], "Fields");
    frame.render_widget(
        Table::new(
            rows,
            [
                Constraint::Length(FIELD_MARKER_WIDTH),
                Constraint::Length(name_width),
                Constraint::Length(FIELD_REQUIREMENT_WIDTH),
            ],
        )
        .column_spacing(FIELD_COLUMN_SPACING),
        sections[1],
    );
}

fn render_field_description(frame: &mut Frame<'_>, area: Rect, session: &AuthoringSession<'_>) {
    let definition = session.definition().clone();
    let current = session.composer.current_field(&definition);
    let (title, description) = current.map_or_else(
        || {
            (
                " Commit form ".to_owned(),
                "Edit values beneath the schema-generated field headers.".to_owned(),
            )
        },
        |kind| field_metadata(kind, &definition),
    );
    let sections = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(area);
    render_section_heading(frame, sections[0], "Field guidance");
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled(
                title.trim().to_owned(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Line::from(description),
        ])
        .wrap(Wrap { trim: true }),
        sections[1],
    );
}

fn render_document_editor(
    frame: &mut Frame<'_>,
    area: Rect,
    session: &mut AuthoringSession<'_>,
) -> CursorStatus {
    session.composer.editor.remove_block();

    // Render against a stable 80-column editing surface, then copy its
    // cursor-following portion directly into the unpadded visible area.
    let virtual_area = Rect::new(0, 0, MAX_EDITOR_INNER_WIDTH, area.height);
    let mut virtual_buffer = Buffer::empty(virtual_area);
    Widget::render(&session.composer.editor, virtual_area, &mut virtual_buffer);

    let column = session
        .composer
        .editor
        .rendered_cursor_position()
        .map_or(1, |position| position.x + 1)
        .clamp(1, MAX_EDITOR_INNER_WIDTH);
    let viewport_width = area.width.clamp(1, MAX_EDITOR_INNER_WIDTH);
    let horizontal_offset = column.saturating_sub(viewport_width);
    for row in 0..area.height.min(virtual_area.height) {
        for viewport_column in 0..viewport_width {
            let source = (horizontal_offset + viewport_column, row);
            let destination = (area.x + viewport_column, area.y + row);
            frame.buffer_mut()[destination] = virtual_buffer[source].clone();
        }
    }

    CursorStatus {
        column,
        wrap_width: MAX_EDITOR_INNER_WIDTH,
    }
}

fn render_review(frame: &mut Frame<'_>, area: Rect, session: &AuthoringSession<'_>) {
    let sections = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(area);
    render_stage_header(frame, sections[0], "Review and commit", 3);
    render_section_heading(frame, sections[2], "Commit message");
    if let Some(review) = &session.review {
        frame.render_widget(
            Paragraph::new(review.message.as_str())
                .wrap(Wrap { trim: false })
                .scroll((review.scroll, 0)),
            sections[3],
        );
    }
    render_navigation_row(
        frame,
        sections[4],
        &[
            ("Enter", "commit"),
            ("Esc", "edit"),
            ("↑/↓", "scroll"),
            ("q/ctrl+c", "cancel"),
        ],
        None,
    );
}

fn render_confirmation(frame: &mut Frame<'_>, area: Rect, session: &AuthoringSession<'_>) {
    let message = match session.confirmation {
        ConfirmationAction::Cancel => "Discard this draft and cancel the commit?",
        ConfirmationAction::ChangeType => "Discard this draft and choose another type?",
    };
    render_centered_notice(
        frame,
        area,
        "Confirm discard",
        message,
        Some("y: discard · Enter/Esc/n: keep editing"),
    );
}

fn field_columns(id: FieldId, definition: &CommitTypeDefinition) -> (String, &'static str) {
    match id {
        FieldId::Scope => ("scope".to_owned(), "optional"),
        FieldId::Description => ("description".to_owned(), "required"),
        FieldId::Property(index) => {
            let property = &definition.properties()[index];
            let requirement = match property.requirement() {
                PropertyRequirement::Required => "required",
                PropertyRequirement::Recommended => "recommended",
                PropertyRequirement::Optional => "optional",
                PropertyRequirement::Conditional(_) => "conditional",
            };
            (property.key().to_string(), requirement)
        }
    }
}

fn field_metadata(kind: FieldKind, definition: &CommitTypeDefinition) -> (String, String) {
    match kind {
        FieldKind::Scope => (
            " Scope · optional ".to_owned(),
            "Optional affected area in a Conventional Commit: type(scope): description. Leave blank for type: description."
                .to_owned(),
        ),
        FieldKind::Description => (
            " Description · required ".to_owned(),
            "Required concise description in a Conventional Commit: type(scope): description, or type: description without a scope."
                .to_owned(),
        ),
        FieldKind::Property {
            definition_index,
            value_index: _,
        } => {
            let property = &definition.properties()[definition_index];
            let requirement = match property.requirement() {
                PropertyRequirement::Required => "required".to_owned(),
                PropertyRequirement::Recommended => "recommended".to_owned(),
                PropertyRequirement::Optional => "optional".to_owned(),
                PropertyRequirement::Conditional(condition) => {
                    format!("conditional: {}", condition.rationale())
                }
            };
            (
                format!(" {} · {requirement} ", property.key()),
                property.description().to_owned(),
            )
        }
    }
}

fn context_height(area: Rect, session: &AuthoringSession<'_>) -> u16 {
    let columns = Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(Rect::new(0, 0, area.width, 1));
    let hud_height = u16::try_from(session.composer.hud_fields(session.definition()).len())
        .unwrap_or(u16::MAX)
        .saturating_add(1);
    let definition = session.definition().clone();
    let (title, description) = session.composer.current_field(&definition).map_or_else(
        || {
            (
                " Commit form ".to_owned(),
                "Edit values beneath the schema-generated field headers.".to_owned(),
            )
        },
        |kind| field_metadata(kind, &definition),
    );
    let guidance_height = wrapped_line_count(title.trim(), columns[1].width)
        .saturating_add(wrapped_line_count(&description, columns[1].width))
        .saturating_add(1);
    hud_height.max(guidance_height)
}

fn wrapped_line_count(text: &str, width: u16) -> u16 {
    let width = usize::from(width.max(1));
    let mut rows = 1_u16;
    let mut used = 0_usize;
    for word in text.split_whitespace() {
        let word_width = Line::from(word).width();
        if used > 0 && used.saturating_add(1).saturating_add(word_width) <= width {
            used += 1 + word_width;
            continue;
        }
        if used > 0 {
            rows = rows.saturating_add(1);
        }
        let word_rows = word_width.max(1).div_ceil(width);
        rows = rows.saturating_add(u16::try_from(word_rows.saturating_sub(1)).unwrap_or(u16::MAX));
        used = word_width % width;
        if used == 0 && word_width > 0 {
            used = width;
        }
    }
    rows
}

fn section_heading_style() -> Style {
    Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD)
}

fn render_section_heading(frame: &mut Frame<'_>, area: Rect, title: &'static str) {
    frame.render_widget(Paragraph::new(title).style(section_heading_style()), area);
}

fn render_centered_notice(
    frame: &mut Frame<'_>,
    area: Rect,
    heading: &'static str,
    message: &'static str,
    controls: Option<&'static str>,
) {
    let lines = if let Some(controls) = controls {
        vec![
            Line::styled(heading, section_heading_style()),
            Line::default(),
            Line::from(message),
            Line::default(),
            Line::from(controls),
        ]
    } else {
        vec![
            Line::styled(heading, section_heading_style()),
            Line::default(),
            Line::from(message),
        ]
    };
    let height = u16::try_from(lines.len()).unwrap_or(u16::MAX);
    let popup = centered_rect(54, height, area);
    frame.render_widget(Clear, popup);
    frame.render_widget(Paragraph::new(lines).centered(), popup);
}

fn render_stage_header(frame: &mut Frame<'_>, area: Rect, title: &'static str, step: u8) {
    let columns = Layout::horizontal([Constraint::Min(0), Constraint::Length(8)]).split(area);
    frame.render_widget(
        Paragraph::new(title).style(Style::default().add_modifier(Modifier::BOLD)),
        columns[0],
    );
    frame.render_widget(
        Paragraph::new(format!("Step {step}/3"))
            .right_aligned()
            .style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        columns[1],
    );
}

fn navigation_style() -> Style {
    Style::default().fg(Color::Black).bg(Color::Yellow)
}

fn navigation_key_style() -> Style {
    navigation_style().add_modifier(Modifier::BOLD)
}

fn navigation_line<'a>(hints: &'a [(&'a str, &'a str)]) -> Line<'a> {
    let mut spans = Vec::with_capacity(hints.len().saturating_mul(3));
    for (index, (key, action)) in hints.iter().copied().enumerate() {
        if index > 0 {
            spans.push(Span::raw(" · "));
        }
        spans.push(Span::styled(key, navigation_key_style()));
        spans.push(Span::raw(": "));
        spans.push(Span::raw(action));
    }
    Line::from(spans)
}

fn render_navigation_row(
    frame: &mut Frame<'_>,
    area: Rect,
    hints: &[(&str, &str)],
    status: Option<&str>,
) {
    let line = navigation_line(hints);
    let Some(status) = status else {
        frame.render_widget(Paragraph::new(line).style(navigation_style()), area);
        return;
    };
    let status = format!("▌ {status} ");
    let status_width = u16::try_from(Line::from(status.as_str()).width()).unwrap_or(u16::MAX);
    let sections =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(status_width)]).split(area);
    frame.render_widget(Paragraph::new(line).style(navigation_style()), sections[0]);
    frame.render_widget(
        Paragraph::new(status).style(navigation_key_style()),
        sections[1],
    );
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .split(area);
    Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .split(vertical[0])[0]
}
