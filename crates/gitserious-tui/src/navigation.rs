use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::theme::navigation_key_style;

pub(crate) const COMMIT_STEPS: [&str; 3] = [
    "select taxonomy and type",
    "compose message",
    "review and submit",
];

/// Keep the clickable parent visible and horizontally reveal the active step on narrow terminals.
pub(crate) fn commit_breadcrumb(frame: &mut Frame<'_>, area: Rect, active: usize) -> Rect {
    let parent_width = u16::try_from(Line::from("Commit").width())
        .unwrap_or(0)
        .min(area.width);
    let parent = Rect::new(area.x, area.y, parent_width, 1);
    frame.render_widget(
        Paragraph::new("Commit").style(navigation_key_style()),
        parent,
    );
    let separator_width = 3.min(area.width.saturating_sub(parent_width));
    let separator = Rect::new(parent.right(), area.y, separator_width, 1);
    frame.render_widget(Paragraph::new(" / "), separator);
    let trail = Rect::new(
        separator.right(),
        area.y,
        area.width.saturating_sub(parent_width + separator_width),
        1,
    );
    let mut spans = Vec::new();
    let mut active_end = 0;
    let mut width = 0;
    for (index, label) in COMMIT_STEPS.into_iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw(" / "));
            width += 3;
        }
        width += label.len();
        spans.push(Span::styled(
            label,
            if index == active {
                navigation_key_style()
            } else {
                Style::default().fg(Color::White)
            },
        ));
        if index == active {
            active_end = width;
        }
    }
    let scroll = u16::try_from(active_end)
        .unwrap_or(u16::MAX)
        .saturating_sub(trail.width);
    frame.render_widget(Paragraph::new(Line::from(spans)).scroll((0, scroll)), trail);
    parent
}
