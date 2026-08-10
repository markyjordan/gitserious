use gitserious_core::{CommitTypeDefinition, PropertyMultiplicity, PropertyRequirement};
use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};

use super::state::{AuthoringSession, ConfirmationAction, FieldKind, Keymap, Stage, VimMode};

const MINIMUM_WIDTH: u16 = 60;
const MINIMUM_HEIGHT: u16 = 18;

pub(crate) fn render(frame: &mut Frame<'_>, session: &mut AuthoringSession<'_>) {
    let area = frame.area();
    session.too_small = area.width < MINIMUM_WIDTH || area.height < MINIMUM_HEIGHT;
    if session.too_small {
        let message = if session.stage == Stage::Confirm {
            "Discard this draft?\ny discard   Enter/n keep editing"
        } else {
            "Terminal too small\nResize or press Esc/q to cancel"
        };
        frame.render_widget(
            Paragraph::new(message)
                .block(Block::default().borders(Borders::ALL).title(" gitserious "))
                .wrap(Wrap { trim: true }),
            area,
        );
        return;
    }

    match session.stage {
        Stage::SelectType => render_picker(frame, area, session),
        Stage::Compose | Stage::Confirm => render_composer(frame, area, session),
        Stage::Review => render_review(frame, area, session),
    }
    if session.stage == Stage::Confirm {
        render_confirmation(frame, area, session);
    }
}

fn render_picker(frame: &mut Frame<'_>, area: Rect, session: &AuthoringSession<'_>) {
    let sections = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(4),
        Constraint::Length(5),
        Constraint::Length(1),
    ])
    .split(area);
    frame.render_widget(
        Paragraph::new("Choose the semantic contract for this commit.").block(
            Block::default()
                .borders(Borders::ALL)
                .title(" gitserious commit "),
        ),
        sections[0],
    );
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
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Commit types "),
        )
        .highlight_symbol("› ")
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    let mut state = ListState::default();
    state.select(Some(session.selected_type));
    frame.render_stateful_widget(list, sections[1], &mut state);
    frame.render_widget(
        Paragraph::new(session.definition().description())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" {} ", session.definition().id())),
            )
            .wrap(Wrap { trim: true }),
        sections[2],
    );
    frame.render_widget(
        Paragraph::new("↑/k ↓/j move  Home/End jump  Enter select  Esc/q cancel"),
        sections[3],
    );
}

fn render_composer(frame: &mut Frame<'_>, area: Rect, session: &mut AuthoringSession<'_>) {
    let sections = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(10),
        Constraint::Length(2),
    ])
    .split(area);
    let keymap = match (session.keymap, session.vim_mode) {
        (Keymap::Conventional, _) => "conventional",
        (Keymap::Vim, VimMode::Normal) => "vim normal",
        (Keymap::Vim, VimMode::Insert) => "vim insert",
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("Type: "),
            Span::styled(
                session.definition().id().as_str(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("   Keymap: "),
            Span::styled(keymap, Style::default().fg(Color::Yellow)),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Compose commit "),
        ),
        sections[0],
    );

    let body = Layout::horizontal([Constraint::Percentage(36), Constraint::Percentage(64)])
        .split(sections[1]);
    render_field_list(frame, body[0], session);
    render_field_editor(frame, body[1], session);

    let help = if session.keymap == Keymap::Vim {
        "Tab fields  F2 conventional  Ctrl+S review  Ctrl+N/D values  Esc normal  q back"
    } else {
        "Tab/Shift+Tab fields  F2 vim  Ctrl+S review  Ctrl+N/D values  Esc back"
    };
    let issue = session
        .composer
        .issues
        .first()
        .map_or("All required fields must be complete.", |issue| {
            issue.message.as_str()
        });
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled(
                issue,
                Style::default().fg(if session.composer.issues.is_empty() {
                    Color::DarkGray
                } else {
                    Color::Red
                }),
            ),
            Line::raw(help),
        ]),
        sections[2],
    );
}

fn render_field_list(frame: &mut Frame<'_>, area: Rect, session: &AuthoringSession<'_>) {
    let items = session
        .composer
        .fields
        .iter()
        .enumerate()
        .map(|(index, field)| {
            let populated = !field.text().trim().is_empty();
            let invalid = session
                .composer
                .issues
                .iter()
                .any(|issue| issue.field == index);
            let marker = if invalid {
                Span::styled("! ", Style::default().fg(Color::Red))
            } else if populated {
                Span::styled("✓ ", Style::default().fg(Color::Green))
            } else {
                Span::styled("○ ", Style::default().fg(Color::DarkGray))
            };
            ListItem::new(Line::from(vec![
                marker,
                Span::raw(field_label(field.kind, session.definition())),
            ]))
        })
        .collect::<Vec<_>>();
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Fields "))
        .highlight_symbol("› ")
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    let mut state = ListState::default();
    state.select(Some(session.composer.focused));
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_field_editor(frame: &mut Frame<'_>, area: Rect, session: &mut AuthoringSession<'_>) {
    let field_kind = session.composer.current().kind;
    let (title, description) = field_metadata(field_kind, session.definition());
    let sections = Layout::vertical([Constraint::Length(5), Constraint::Min(4)]).split(area);
    frame.render_widget(
        Paragraph::new(description)
            .block(Block::default().borders(Borders::ALL).title(title))
            .wrap(Wrap { trim: true }),
        sections[0],
    );
    let issues = session
        .composer
        .issues
        .iter()
        .filter(|issue| issue.field == session.composer.focused)
        .map(|issue| issue.message.as_str())
        .collect::<Vec<_>>()
        .join("; ");
    let editor_title = if issues.is_empty() {
        " Edit ".to_owned()
    } else {
        format!(" Error: {issues} ")
    };
    let border_style = if issues.is_empty() {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::Red)
    };
    session.composer.current_mut().editor.set_block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(editor_title),
    );
    frame.render_widget(&session.composer.current().editor, sections[1]);
}

fn render_review(frame: &mut Frame<'_>, area: Rect, session: &AuthoringSession<'_>) {
    let sections = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(8),
        Constraint::Length(1),
    ])
    .split(area);
    frame.render_widget(
        Paragraph::new("Review the exact canonical message before Git creates the commit.").block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Review commit "),
        ),
        sections[0],
    );
    if let Some(review) = &session.review {
        frame.render_widget(
            Paragraph::new(review.message.as_str())
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Commit message "),
                )
                .wrap(Wrap { trim: false })
                .scroll((review.scroll, 0)),
            sections[1],
        );
    }
    frame.render_widget(
        Paragraph::new("Enter commit  Esc edit  ↑/↓ scroll  q/Ctrl+C cancel"),
        sections[2],
    );
}

fn render_confirmation(frame: &mut Frame<'_>, area: Rect, session: &AuthoringSession<'_>) {
    let popup = centered_rect(54, 7, area);
    let message = match session.confirmation {
        ConfirmationAction::Cancel => "Discard this draft and cancel the commit?",
        ConfirmationAction::ChangeType => "Discard this draft and choose another type?",
    };
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(format!("{message}\n\ny discard   Enter/Esc/n keep editing"))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Red))
                    .title(" Confirm discard "),
            )
            .wrap(Wrap { trim: true }),
        popup,
    );
}

fn field_label(kind: FieldKind, definition: &CommitTypeDefinition) -> String {
    match kind {
        FieldKind::Scope => "scope (optional)".to_owned(),
        FieldKind::Subject => "subject (required)".to_owned(),
        FieldKind::Property {
            definition_index,
            value_index,
        } => {
            let property = &definition.properties()[definition_index];
            if property.multiplicity() == PropertyMultiplicity::Multiple {
                format!("{} [{}]", property.key(), value_index + 1)
            } else {
                property.key().to_string()
            }
        }
    }
}

fn field_metadata(kind: FieldKind, definition: &CommitTypeDefinition) -> (String, String) {
    match kind {
        FieldKind::Scope => (
            " Scope · optional ".to_owned(),
            "Semantic area affected by the commit. Leave empty when no scope applies.".to_owned(),
        ),
        FieldKind::Subject => (
            " Subject · required ".to_owned(),
            "Concise, single-line summary of the change.".to_owned(),
        ),
        FieldKind::Property {
            definition_index,
            value_index,
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
            let multiplicity = match property.multiplicity() {
                PropertyMultiplicity::Single => "single value".to_owned(),
                PropertyMultiplicity::Multiple => format!("value {} · repeatable", value_index + 1),
            };
            (
                format!(" {} · {requirement} · {multiplicity} ", property.key()),
                property.description().to_owned(),
            )
        }
    }
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .split(area);
    Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .split(vertical[0])[0]
}
