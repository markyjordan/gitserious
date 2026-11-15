use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};

use super::citation_picker;
use super::composer;
use super::state::{AuthoringSession, ConfirmationAction, Stage};
use super::type_picker;
use crate::confirmation::{Confirmation, DISCARD};
use crate::theme::{JET_BLACK, centered_rect, popup_selected_style, section_heading_style, theme};

const MINIMUM_WIDTH: u16 = 60;
const MINIMUM_HEIGHT: u16 = 16;

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
        Stage::SelectType => type_picker::render(frame, area, session),
        Stage::Compose => composer::render(frame, area, &mut session.composer),
        Stage::Review => render_review(frame, area, session, target),
        Stage::Confirm => unreachable!("confirmation resolves to its underlying stage"),
    }
    if session.stage == Stage::Compose {
        citation_picker::render(
            frame,
            area,
            session.composer.layout().cursor_row,
            &mut session.citation_picker,
            &session.paths,
            &session.commits,
        );
    }
    if session.stage == Stage::Confirm {
        render_confirmation(frame, session);
    }
}

/// The footer keys of the stage on screen. A conditional field adds the keys
/// that answer whether it applies.
pub(crate) fn hints(session: &AuthoringSession) -> Vec<(&'static str, &'static str)> {
    if session.citation_picker.is_some() && session.stage == Stage::Compose {
        return vec![
            ("enter", "cite"),
            ("↑/↓", "move"),
            ("←/→", "kind"),
            ("esc", "cancel"),
        ];
    }
    match session.visible_stage() {
        Stage::SelectType => vec![
            ("enter", "choose"),
            ("↑/↓", "move"),
            ("←/→", "taxonomy"),
            ("esc", "back"),
        ],
        Stage::Compose => {
            let mut hints = vec![("ctrl+s", "review"), ("ctrl+t", "type"), ("↑/↓", "move")];
            if session.composer.focused_conditional() {
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
        Stage::Confirm => unreachable!("confirmation shows over the composer"),
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
        .composer
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
    let [text, _, commit, _, feedback] = Layout::vertical([
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
    frame.render_widget(Paragraph::new(recommendations), feedback);
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

pub(super) const fn inset_horizontally(area: Rect, amount: u16) -> Rect {
    Rect::new(
        area.x.saturating_add(amount),
        area.y,
        area.width.saturating_sub(amount.saturating_mul(2)),
        area.height,
    )
}
