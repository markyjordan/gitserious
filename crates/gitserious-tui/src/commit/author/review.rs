use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Wrap};

use super::render::inset_horizontally;
use super::state::AuthoringSession;
use crate::theme::{popup_selected_style, theme};

/// The message without its provenance trailers, then the highlighted `target` row
/// that enter commits, then any recommendations the message leaves out, dimmed.
pub(super) fn render(
    frame: &mut Frame<'_>,
    area: Rect,
    session: &mut AuthoringSession,
    target: &str,
) {
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
