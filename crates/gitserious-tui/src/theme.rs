use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

pub(crate) const MINIMUM_WIDTH: u16 = 60;
pub(crate) const MINIMUM_HEIGHT: u16 = 18;
pub(crate) const JET_BLACK: Color = Color::Rgb(0, 0, 0);
pub(crate) const ZEBRA_BACKGROUND: Color = Color::Rgb(16, 16, 16);

pub(crate) fn frame_style() -> Style {
    Style::default().fg(Color::DarkGray)
}
pub(crate) fn section_heading_style() -> Style {
    Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD)
}
pub(crate) fn navigation_style() -> Style {
    Style::default().fg(JET_BLACK).bg(Color::Yellow)
}
pub(crate) fn navigation_key_style() -> Style {
    navigation_style().add_modifier(Modifier::BOLD)
}

pub(crate) fn render_navigation_row(frame: &mut Frame<'_>, area: Rect, hints: &[(&str, &str)]) {
    let mut spans = Vec::with_capacity(hints.len().saturating_mul(3));
    for (index, (key, action)) in hints.iter().copied().enumerate() {
        if index > 0 {
            spans.push(Span::raw(" | "));
        }
        spans.push(Span::styled(key, navigation_key_style()));
        spans.push(Span::raw(": "));
        spans.push(Span::raw(action));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(navigation_style()),
        area,
    );
}

pub(crate) fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .split(area);
    Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .split(vertical[0])[0]
}

pub(crate) fn normalize_background(frame: &mut Frame<'_>, area: Rect) {
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            let cell = &mut frame.buffer_mut()[(x, y)];
            if matches!(cell.bg, Color::Reset | Color::Black) {
                cell.set_bg(JET_BLACK);
            }
        }
    }
}
