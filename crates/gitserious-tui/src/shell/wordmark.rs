use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use crate::theme::theme;

const WORDMARK: [&str; 5] = [
    "▄████ ███ █████ ▄███ ████ ███▄ ███ ▄██▄ █  █ ▄███",
    "█      █    █   █    █    █  █  █  █  █ █  █ █   ",
    "█  ██  █    █   ████ ███  ███   █  █  █ █  █ ████",
    "█   █  █    █      █ █    █  █  █  █  █ █  █    █",
    "▀███▀ ███   █   ███▀ ████ █  █ ███ ▀██▀ ▀██▀ ███▀",
];

pub(super) const TAGLINE: &str = "Why did this commit message explain nothing?";

/// Draws the static wordmark and tagline centered in `area`.
pub(super) fn render_home(frame: &mut Frame<'_>, area: Rect) {
    let mut lines: Vec<Line<'static>> = WORDMARK
        .iter()
        .map(|row| Line::styled(*row, Style::new().fg(theme().text)))
        .collect();
    lines.push(Line::default());
    lines.push(Line::styled(TAGLINE, Style::new().fg(theme().dim)));
    let height = u16::try_from(lines.len()).unwrap_or(u16::MAX);
    let [block] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(area);
    frame.render_widget(Paragraph::new(lines).centered(), block);
}
