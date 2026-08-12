use gitserious_core::{CommitTypeDefinition, PropertyRequirement};
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Cell, Clear, List, ListItem, ListState, Padding, Paragraph, Row, Table, Widget,
    Wrap,
};

use super::state::{AuthoringSession, ConfirmationAction, FieldId, FieldKind, FieldStatus, Stage};

const MINIMUM_WIDTH: u16 = 60;
const MINIMUM_HEIGHT: u16 = 18;
const MAX_EDITOR_INNER_WIDTH: u16 = 80;
const PANE_PADDING: u16 = 1;
const MINIMUM_EDITOR_HEIGHT: u16 = 7;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CursorStatus {
    column: u16,
    wrap_width: u16,
}

pub(crate) fn render(frame: &mut Frame<'_>, session: &mut AuthoringSession<'_>) {
    let area = frame.area();
    session.too_small = area.width < MINIMUM_WIDTH || area.height < MINIMUM_HEIGHT;
    if session.too_small {
        let message = if session.stage == Stage::Confirm {
            "Discard this draft?\ny: discard · Enter/n: keep editing"
        } else {
            "Terminal too small\nResize or press Esc/q to cancel"
        };
        frame.render_widget(
            Paragraph::new(message)
                .block(pane_block(" gitserious "))
                .wrap(Wrap { trim: true }),
            area,
        );
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
        Constraint::Min(6),
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
        .block(pane_block(" Commit types "))
        .highlight_symbol("› ")
        .highlight_style(navigation_key_style());
    let mut state = ListState::default();
    state.select(Some(session.selected_type));
    frame.render_stateful_widget(list, sections[1], &mut state);
    render_navigation_row(
        frame,
        sections[2],
        &[("↑/↓", "move"), ("Enter", "select"), ("Esc/q", "cancel")],
        None,
    );
}

fn render_composer(frame: &mut Frame<'_>, area: Rect, session: &mut AuthoringSession<'_>) {
    let sections = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(12),
        Constraint::Length(2),
    ])
    .split(area);
    render_stage_header(frame, sections[0], "Compose commit message", 2);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("Type: "),
            Span::styled(
                session.definition().id().as_str(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ])),
        sections[1],
    );

    let desired_context_height =
        u16::try_from(session.definition().properties().len() + 6).unwrap_or(u16::MAX);
    let context_height = desired_context_height
        .min(sections[2].height.saturating_sub(MINIMUM_EDITOR_HEIGHT))
        .max(3);
    let body = Layout::vertical([
        Constraint::Length(context_height),
        Constraint::Min(MINIMUM_EDITOR_HEIGHT),
    ])
    .split(sections[2]);
    render_field_context(frame, body[0], session);
    let cursor_status = render_document_editor(frame, body[1], session);

    let help: &[_] = &[("↑/↓", "move"), ("esc", "back"), ("ctrl+s", "review")];
    let footer =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(sections[3]);
    if let Some(issue) = session.composer.issues.first() {
        frame.render_widget(
            Paragraph::new(issue.message.as_str()).style(Style::default().fg(Color::Red)),
            footer[0],
        );
    }
    render_navigation_row(
        frame,
        footer[1],
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
    let name_width = fields
        .iter()
        .map(|field| field_columns(field.id, definition).0.chars().count())
        .max()
        .and_then(|width| u16::try_from(width).ok())
        .unwrap_or(1)
        .min(area.width.saturating_sub(18).max(1));
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
    frame.render_widget(
        Table::new(
            rows,
            [
                Constraint::Length(1),
                Constraint::Length(name_width),
                Constraint::Length(11),
            ],
        )
        .column_spacing(1)
        .block(pane_block(" Fields ")),
        area,
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
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled(
                title.trim().to_owned(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Line::from(description),
        ])
        .block(pane_block(" Field guidance "))
        .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_document_editor(
    frame: &mut Frame<'_>,
    area: Rect,
    session: &mut AuthoringSession<'_>,
) -> CursorStatus {
    let definition = session.definition().clone();
    let current_id = session
        .composer
        .current_field(&definition)
        .map(FieldKind::id);
    let issues = session
        .composer
        .issues
        .iter()
        .filter(|issue| issue.field.is_none() || issue.field == current_id)
        .map(|issue| issue.message.as_str())
        .collect::<Vec<_>>()
        .join("; ");
    let editor_title = if issues.is_empty() {
        " Message form ".to_owned()
    } else {
        format!(" Error: {issues} ")
    };
    let border_style = if issues.is_empty() {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::Red)
    };
    let editor_block = pane_block(editor_title).border_style(border_style);
    session.composer.editor.set_block(editor_block.clone());

    // Render against a stable 80-column editing surface, then copy the
    // cursor-following portion into the real pane without overwriting padding.
    let virtual_width = MAX_EDITOR_INNER_WIDTH + 2 + PANE_PADDING * 2;
    let virtual_area = Rect::new(0, 0, virtual_width, area.height);
    let virtual_content = editor_block.inner(virtual_area);
    let visible_content = editor_block.inner(area);
    let mut virtual_buffer = Buffer::empty(virtual_area);
    Widget::render(&session.composer.editor, virtual_area, &mut virtual_buffer);
    frame.render_widget(editor_block, area);

    let column = session
        .composer
        .editor
        .rendered_cursor_position()
        .map_or(1, |position| {
            position.x.saturating_sub(virtual_content.x) + 1
        })
        .clamp(1, MAX_EDITOR_INNER_WIDTH);
    let viewport_width = visible_content.width.clamp(1, MAX_EDITOR_INNER_WIDTH);
    let horizontal_offset = column.saturating_sub(viewport_width);
    for row in 0..visible_content.height.min(virtual_content.height) {
        for viewport_column in 0..viewport_width {
            let source = (
                virtual_content.x + horizontal_offset + viewport_column,
                virtual_content.y + row,
            );
            let destination = (visible_content.x + viewport_column, visible_content.y + row);
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
        Constraint::Min(8),
        Constraint::Length(1),
    ])
    .split(area);
    render_stage_header(frame, sections[0], "Review and commit", 3);
    if let Some(review) = &session.review {
        frame.render_widget(
            Paragraph::new(review.message.as_str())
                .block(pane_block(" Commit message "))
                .wrap(Wrap { trim: false })
                .scroll((review.scroll, 0)),
            sections[1],
        );
    }
    render_navigation_row(
        frame,
        sections[2],
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
    let popup = centered_rect(54, 7, area);
    let message = match session.confirmation {
        ConfirmationAction::Cancel => "Discard this draft and cancel the commit?",
        ConfirmationAction::ChangeType => "Discard this draft and choose another type?",
    };
    frame.render_widget(Clear, popup);
    let block = pane_block(" Confirm discard ").border_style(Style::default().fg(Color::Red));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let content = Layout::vertical([Constraint::Length(3)])
        .flex(Flex::Center)
        .split(inner)[0];
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(message),
            Line::default(),
            Line::from("y: discard · Enter/Esc/n: keep editing"),
        ])
        .centered(),
        content,
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

fn pane_block<'a>(title: impl Into<Line<'a>>) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .padding(Padding::uniform(PANE_PADDING))
        .title(title)
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
