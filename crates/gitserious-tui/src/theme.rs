use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

pub(crate) const MINIMUM_WIDTH: u16 = 66;
pub(crate) const MINIMUM_HEIGHT: u16 = 18;
pub(crate) const JET_BLACK: Color = Color::Rgb(0, 0, 0);
pub(crate) const ZEBRA_BACKGROUND: Color = Color::Rgb(16, 16, 16);
/// Unapplied changes and EDIT.
pub(crate) const AMBER: Color = Color::Rgb(255, 176, 0);

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

pub(crate) fn row_style(index: usize) -> Style {
    Style::default().bg(if index.is_multiple_of(2) {
        JET_BLACK
    } else {
        ZEBRA_BACKGROUND
    })
}

pub(crate) fn selected_style(active: bool) -> Style {
    if active {
        navigation_key_style()
    } else {
        Style::default()
            .fg(Color::White)
            .bg(Color::Rgb(40, 40, 40))
            .add_modifier(Modifier::BOLD)
    }
}

pub(crate) fn focus_frame_style(active: bool) -> Style {
    if active {
        Style::default().fg(Color::Yellow)
    } else {
        frame_style()
    }
}

/// Shared hint text for popup controls, without imposing colors.
pub(crate) fn command_hints(hints: &[(&str, &str)]) -> Line<'static> {
    let regular = Style::default().remove_modifier(Modifier::BOLD);
    let mut spans = Vec::with_capacity(hints.len().saturating_mul(3));
    for (index, (key, action)) in hints.iter().copied().enumerate() {
        if index > 0 {
            spans.push(Span::styled("  ", regular));
        }
        spans.push(Span::styled(
            format!("[{key}]"),
            Style::default().add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(format!(" {action}"), regular));
    }
    Line::from(spans)
}

pub(crate) fn footer_hints(hints: &[(&str, &str)]) -> Line<'static> {
    let mut spans = Vec::with_capacity(hints.len().saturating_mul(3));
    for (index, (key, action)) in hints.iter().copied().enumerate() {
        if index > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(key.to_owned(), navigation_key_style()));
        spans.push(Span::raw(": "));
        spans.push(Span::raw(action.to_owned()));
    }
    Line::from(spans)
}

pub(crate) fn render_navigation_row(frame: &mut Frame<'_>, area: Rect, hints: &[(&str, &str)]) {
    let mut visible = hints.to_vec();
    // Keep the switch and command hints readable on compact terminals. Arrow controls
    // are also documented in contextual Help and can be omitted from a crowded footer.
    for key in ["↑/↓", "←/→"] {
        if footer_hints(&visible).width() <= usize::from(area.width) {
            break;
        }
        visible.retain(|(hint, _)| *hint != key);
    }
    frame.render_widget(
        Paragraph::new(footer_hints(&visible)).style(navigation_style()),
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

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn shared_navigation_row_separates_authoring_hints_without_pipes()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut terminal = Terminal::new(TestBackend::new(100, 1))?;
        terminal.draw(|frame| {
            let area = frame.area();
            render_navigation_row(
                frame,
                area,
                &[("tab", "switch"), ("↑/↓", "move"), ("enter", "select")],
            );
        })?;
        let row = (0..100)
            .map(|x| terminal.backend().buffer()[(x, 0)].symbol())
            .collect::<String>();
        assert!(row.starts_with("tab: switch  ↑/↓: move  enter: select"));
        assert!(!row.contains('|'));
        Ok(())
    }

    #[test]
    fn crowded_footer_drops_arrow_hints_before_switch_and_commands()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut terminal = Terminal::new(TestBackend::new(40, 1))?;
        terminal.draw(|frame| {
            render_navigation_row(
                frame,
                frame.area(),
                &[
                    ("tab/shift+tab", "switch"),
                    ("↑/↓", "move"),
                    ("←/→", "tree"),
                    ("/", "commands"),
                ],
            )
        })?;
        let row = (0..40)
            .map(|x| terminal.backend().buffer()[(x, 0)].symbol())
            .collect::<String>();
        assert!(row.starts_with("tab/shift+tab: switch  /: commands"));
        Ok(())
    }

    #[test]
    fn popup_hint_actions_override_inherited_bold_without_changing_colors()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut terminal = Terminal::new(TestBackend::new(20, 1))?;
        terminal.draw(|frame| {
            frame.render_widget(
                Paragraph::new(command_hints(&[("esc", "close")])).style(
                    Style::default()
                        .fg(Color::White)
                        .bg(JET_BLACK)
                        .add_modifier(Modifier::BOLD),
                ),
                frame.area(),
            )
        })?;
        let buffer = terminal.backend().buffer();
        assert!(buffer[(0, 0)].modifier.contains(Modifier::BOLD));
        assert!(!buffer[(6, 0)].modifier.contains(Modifier::BOLD));
        assert_eq!(buffer[(6, 0)].fg, Color::White);
        assert_eq!(buffer[(6, 0)].bg, JET_BLACK);
        Ok(())
    }
}
