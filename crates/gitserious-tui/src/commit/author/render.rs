use gitserious_core::CommitTypeDefinition;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};

use super::cards;
use super::state::{AuthoringSession, CatalogTab, ConfirmationAction, Stage};
use crate::confirmation::{Confirmation, DISCARD};
use crate::theme::{JET_BLACK, centered_rect, popup_selected_style, section_heading_style, theme};

const MINIMUM_WIDTH: u16 = 60;
const MINIMUM_HEIGHT: u16 = 16;
/// Columns between the picker panel's edge and its text.
const PICKER_PADDING: u16 = 2;
/// The picker panel's rows besides the type list and the description: padding
/// above and below, the taxonomy row, and a blank row on each side of the list.
const PICKER_FIXED_ROWS: u16 = 5;
const PICKER_KEY_GAP: u16 = 4;
const TAB_GAP: u16 = 2;

/// Draws the editor in `area`, which the Commit page leaves it under the staged header.
/// `target` names where Git puts the commit, as the review's commit row reads.
pub(crate) fn render(
    frame: &mut Frame<'_>,
    area: Rect,
    session: &mut AuthoringSession,
    target: &str,
) {
    session.discard_dialog.invalidate();
    session.catalog_tabs.clear();
    session.type_rows.clear();
    session.too_small = area.width < MINIMUM_WIDTH || area.height < MINIMUM_HEIGHT;
    if session.too_small {
        if session.stage == Stage::Confirm {
            session.discard_dialog.render(
                frame,
                &Confirmation {
                    title: "Discard Message",
                    message: "Discard this draft?".into(),
                    choices: DISCARD,
                },
            );
        } else {
            render_centered_notice(
                frame,
                area,
                "Terminal too small",
                "Resize or press esc/q to cancel",
                None,
            );
        }
        return;
    }

    match session.visible_stage() {
        Stage::SelectType => render_picker(frame, area, session),
        Stage::Compose => cards::render(frame, area, &mut session.cards),
        Stage::Review => render_review(frame, area, session, target),
        Stage::Confirm => unreachable!("confirmation resolves to its underlying stage"),
    }
    if session.stage == Stage::Confirm {
        render_confirmation(frame, session);
    }
}

/// The footer keys of the stage on screen. A conditional card adds the keys
/// that answer whether it applies.
pub(crate) fn hints(session: &AuthoringSession) -> Vec<(&'static str, &'static str)> {
    match session.visible_stage() {
        Stage::SelectType => vec![
            ("enter", "choose"),
            ("↑/↓", "move"),
            ("←/→", "taxonomy"),
            ("esc", "back"),
        ],
        Stage::Compose => {
            let mut hints = vec![("ctrl+s", "review"), ("ctrl+t", "type"), ("↑/↓", "move")];
            if session.cards.focused_conditional() {
                hints.extend([("alt+a", "applies"), ("alt+n", "doesn't apply")]);
            }
            hints.push(("esc", "back"));
            hints
        }
        Stage::Review => vec![
            ("enter", "commit"),
            ("esc", "keep editing"),
            ("↑/↓", "scroll"),
        ],
        Stage::Confirm => unreachable!("confirmation shows over the cards"),
    }
}

/// The subject line, then a panel listing the types whose key starts with what
/// was typed on it, with the highlighted type's description underneath.
fn render_picker(frame: &mut Frame<'_>, area: Rect, session: &mut AuthoringSession) {
    let [subject, _, below] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .areas(area);
    render_subject_filter(frame, subject, &session.filter);
    let panel = inset_horizontally(below, 1);
    let content_width = panel.width.saturating_sub(PICKER_PADDING * 2);
    let key_width = picker_key_width(&session.definitions, content_width);
    let detail_width = content_width.saturating_sub(key_width);
    // The tallest description sizes the row, so moving the selection never moves the list.
    let detail_height = session
        .definitions
        .iter()
        .map(|definition| wrapped_line_count(definition.description(), detail_width))
        .max()
        .unwrap_or(1);
    let matches = session.matching_types();
    let room = panel
        .height
        .saturating_sub(PICKER_FIXED_ROWS.saturating_add(detail_height));
    let list_height = u16::try_from(matches.len())
        .unwrap_or(u16::MAX)
        .min(room)
        .max(1);
    let panel = Rect {
        height: PICKER_FIXED_ROWS
            .saturating_add(detail_height)
            .saturating_add(list_height)
            .min(panel.height),
        ..panel
    };
    frame.render_widget(
        Block::default().style(Style::new().fg(theme().text).bg(theme().band)),
        panel,
    );
    let [_, taxonomies, _, list, _, detail] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(list_height),
        Constraint::Length(1),
        Constraint::Length(detail_height),
    ])
    .areas(inset_horizontally(panel, PICKER_PADDING));
    render_catalog_tabs(frame, taxonomies, session);
    if matches.is_empty() {
        frame.render_widget(
            Paragraph::new(format!("No type starts with \"{}\"", session.filter))
                .style(Style::new().fg(theme().dim)),
            list,
        );
        return;
    }
    let position = matches
        .iter()
        .position(|index| *index == session.selected_type)
        .unwrap_or(0);
    let rows = usize::from(list.height);
    let mut offset = session.type_offset.min(matches.len().saturating_sub(rows));
    if position < offset {
        offset = position;
    } else if position >= offset + rows {
        offset = position + 1 - rows;
    }
    session.type_offset = offset;
    for (y, index) in (list.y..list.bottom()).zip(matches.into_iter().skip(offset)) {
        let definition = &session.definitions[index];
        let row = Rect::new(panel.x, y, panel.width, 1);
        let (key_style, property_style) = if index == session.selected_type {
            frame.render_widget(Block::default().style(popup_selected_style()), row);
            (popup_selected_style(), popup_selected_style())
        } else {
            (Style::new().fg(theme().text), Style::new().fg(theme().dim))
        };
        let properties = definition
            .properties()
            .iter()
            .map(|property| property.key().to_string())
            .collect::<Vec<_>>()
            .join(" · ");
        let [key, rest] = Layout::horizontal([Constraint::Length(key_width), Constraint::Min(0)])
            .areas(Rect::new(list.x, y, list.width, 1));
        frame.render_widget(
            Paragraph::new(definition.id().as_str()).style(key_style),
            key,
        );
        frame.render_widget(Paragraph::new(properties).style(property_style), rest);
        session.type_rows.push((index, row));
    }
    let definition = session.definition();
    let [key, description] =
        Layout::horizontal([Constraint::Length(key_width), Constraint::Min(0)]).areas(detail);
    frame.render_widget(
        Paragraph::new(definition.id().as_str())
            .style(Style::new().fg(theme().text).add_modifier(Modifier::BOLD)),
        key,
    );
    frame.render_widget(
        Paragraph::new(definition.description()).wrap(Wrap { trim: true }),
        description,
    );
}

/// An accent bar, the typed filter, a cursor, and the subject's shape until something is typed.
fn render_subject_filter(frame: &mut Frame<'_>, area: Rect, filter: &str) {
    frame.render_widget(
        Paragraph::new("▎").style(Style::new().fg(theme().pending)),
        area,
    );
    let text = Rect {
        x: area.x.saturating_add(PICKER_PADDING + 1),
        width: area.width.saturating_sub(PICKER_PADDING + 1),
        ..area
    };
    let room = usize::from(text.width.saturating_sub(1));
    let typed: String = filter
        .chars()
        .skip(filter.chars().count().saturating_sub(room))
        .collect();
    let mut spans = vec![
        Span::styled(
            typed,
            Style::new().fg(theme().text).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            " ",
            Style::new()
                .fg(theme().text)
                .add_modifier(Modifier::REVERSED),
        ),
    ];
    if filter.is_empty() {
        spans.push(Span::styled(
            "type(scope): description",
            Style::new().fg(theme().dim),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), text);
}

/// Taxonomy ids on the left, the active one underlined, and its versioned id on the right.
fn render_catalog_tabs(frame: &mut Frame<'_>, area: Rect, session: &mut AuthoringSession) {
    let mut x = area.x;
    let mut catalogs = session.available_type_catalogs();
    let total_width: u16 = catalogs
        .iter()
        .map(|kind| label_width(&session.catalog_label(*kind)).saturating_add(TAB_GAP))
        .fold(0, u16::saturating_add);
    if total_width > area.width {
        let selected = catalogs
            .iter()
            .position(|kind| *kind == session.type_catalog)
            .unwrap_or(0);
        catalogs.rotate_left(selected);
    }
    for kind in catalogs {
        if x >= area.right() {
            break;
        }
        let label = session.catalog_label(kind);
        let width = label_width(&label).min(area.right().saturating_sub(x));
        let tab = Rect::new(x, area.y, width, 1);
        let style = if session.type_catalog == kind {
            Style::new()
                .fg(theme().text)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else {
            Style::new().fg(theme().dim)
        };
        frame.render_widget(Paragraph::new(label).style(style), tab);
        session.catalog_tabs.push(CatalogTab { kind, area: tab });
        x = x.saturating_add(width).saturating_add(TAB_GAP);
    }
    let reference = session
        .taxonomy()
        .map(|taxonomy| taxonomy.reference().to_string())
        .unwrap_or_default();
    let width = label_width(&reference);
    if x.saturating_add(width) <= area.right() {
        frame.render_widget(
            Paragraph::new(reference).style(Style::new().fg(theme().dim)),
            Rect::new(area.right() - width, area.y, width, 1),
        );
    }
}

/// The message without its provenance trailers, then the highlighted `target` row
/// that enter commits, then any recommendations the message leaves out, dimmed.
fn render_review(frame: &mut Frame<'_>, area: Rect, session: &mut AuthoringSession, target: &str) {
    let Some(review) = &mut session.review else {
        return;
    };
    let area = inset_horizontally(area, 1);
    let recommendations: Vec<_> = session
        .cards
        .recommendations()
        .map(|message| Line::styled(message.to_owned(), Style::new().fg(theme().dim)))
        .collect();
    let recommendation_rows = u16::try_from(recommendations.len())
        .unwrap_or(u16::MAX)
        .saturating_add(u16::from(!recommendations.is_empty()));
    let message =
        Paragraph::new(review.message.content().trim_end().to_owned()).wrap(Wrap { trim: false });
    let rows = u16::try_from(message.line_count(area.width)).unwrap_or(u16::MAX);
    let room = area.height.saturating_sub(2 + recommendation_rows);
    review.scrollable = rows > room;
    review.scroll = review.scroll.min(rows.saturating_sub(room));
    let [text, _, commit, _, notes] = Layout::vertical([
        Constraint::Length(rows.min(room)),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(u16::from(!recommendations.is_empty())),
        Constraint::Min(0),
    ])
    .areas(area);
    frame.render_widget(message.scroll((review.scroll, 0)), text);
    let key = "enter ";
    let label = crate::commit::truncate(
        &format!(" {target}"),
        usize::from(commit.width).saturating_sub(key.len() + 1),
    );
    let gap =
        usize::from(commit.width).saturating_sub(Line::from(label.as_str()).width() + key.len());
    frame.render_widget(
        Paragraph::new(format!("{label}{}{key}", " ".repeat(gap))).style(popup_selected_style()),
        commit,
    );
    frame.render_widget(Paragraph::new(recommendations), notes);
}

fn render_confirmation(frame: &mut Frame<'_>, session: &mut AuthoringSession) {
    let message = match session.confirmation {
        ConfirmationAction::Cancel => "Discard this draft and cancel the commit?",
        ConfirmationAction::ChangeType => "Discard this draft and choose another type?",
    };
    session.discard_dialog.render(
        frame,
        &Confirmation {
            title: "Discard Message",
            message: message.into(),
            choices: DISCARD,
        },
    );
}

/// The widest type key plus a gap, so the property column lines up for every type.
fn picker_key_width(definitions: &[CommitTypeDefinition], content_width: u16) -> u16 {
    definitions
        .iter()
        .map(|definition| label_width(definition.id().as_str()))
        .max()
        .unwrap_or(1)
        .saturating_add(PICKER_KEY_GAP)
        .min(content_width / 2)
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
    frame.render_widget(
        Block::default().style(Style::default().bg(JET_BLACK)),
        popup,
    );
    frame.render_widget(Paragraph::new(lines).centered(), popup);
}

fn label_width(label: &str) -> u16 {
    u16::try_from(Line::from(label).width()).unwrap_or(u16::MAX)
}

const fn inset_horizontally(area: Rect, amount: u16) -> Rect {
    Rect::new(
        area.x.saturating_add(amount),
        area.y,
        area.width.saturating_sub(amount.saturating_mul(2)),
        area.height,
    )
}
