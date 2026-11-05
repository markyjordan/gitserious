use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Padding, Paragraph};

pub(crate) const MINIMUM_WIDTH: u16 = 66;
pub(crate) const MINIMUM_HEIGHT: u16 = 18;
pub(crate) const JET_BLACK: Color = Color::Rgb(0, 0, 0);
pub(crate) const ZEBRA_BACKGROUND: Color = Color::Rgb(16, 16, 16);
pub(crate) const ACCENT: Color = Color::Rgb(80, 160, 255);

/// One appearance: the semantic colors every redesigned screen draws with.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Theme {
    /// Content background.
    pub ground: Color,
    /// The navigation bar and footer background.
    pub band: Color,
    pub text: Color,
    pub dim: Color,
    /// The selected row's background.
    pub cursor: Color,
    /// Something waiting on the user, such as an uninitialized repository.
    pub pending: Color,
    pub done: Color,
    /// A problem that blocks the next step.
    pub blocks: Color,
}

pub(crate) const BONE: Theme = Theme {
    ground: Color::Rgb(0x11, 0x10, 0x0E),
    band: Color::Rgb(0x1F, 0x1D, 0x1A),
    text: Color::Rgb(0xED, 0xEB, 0xDD),
    dim: Color::Rgb(0xA3, 0x9F, 0x91),
    cursor: Color::Rgb(0xED, 0xEB, 0xDD),
    pending: Color::Rgb(0xE8, 0xA3, 0x3D),
    done: Color::Rgb(0xA9, 0xC9, 0xA0),
    blocks: Color::Rgb(0xEA, 0x8F, 0x7F),
};

/// The active appearance.
pub(crate) const fn theme() -> &'static Theme {
    &BONE
}

pub(crate) fn block_title(title: &str) -> String {
    format!(" {} ", title.trim())
}

pub(crate) fn frame_style() -> Style {
    Style::default().fg(Color::DarkGray)
}
pub(crate) fn section_heading_style() -> Style {
    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
}
pub(crate) fn navigation_style() -> Style {
    Style::default().fg(JET_BLACK).bg(ACCENT)
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
        Style::default().fg(ACCENT)
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

pub(crate) fn footer_key_style() -> Style {
    Style::new()
        .fg(theme().text)
        .bg(theme().band)
        .add_modifier(Modifier::BOLD)
}

pub(crate) fn footer_label_style() -> Style {
    Style::new().fg(theme().dim).bg(theme().band)
}

pub(crate) fn footer_hints(hints: &[(&str, &str)]) -> Line<'static> {
    let mut spans = Vec::with_capacity(hints.len().saturating_mul(3));
    for (index, (key, action)) in hints.iter().copied().enumerate() {
        if index > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(key.to_owned(), footer_key_style()));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(action.to_owned(), footer_label_style()));
    }
    Line::from(spans)
}

pub(crate) const FOOTER_HEIGHT: u16 = 1;

/// The shell keys every root footer ends with.
pub(crate) const SHELL_HINTS: [(&str, &str); 3] =
    [("tab", "nav bar"), ("/", "commands"), ("q", "quit")];

/// Render the footer background and return its content area.
pub(crate) fn render_footer_block(frame: &mut Frame<'_>, area: Rect) -> Rect {
    let block = Block::default()
        .padding(Padding::horizontal(1))
        .style(Style::new().fg(theme().text).bg(theme().band));
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);
    inner
}

pub(crate) fn render_navigation_row(frame: &mut Frame<'_>, area: Rect, hints: &[(&str, &str)]) {
    render_footer(frame, area, hints, &[]);
}

/// Contextual hints on the left and shell hints on the right.
pub(crate) fn render_footer(
    frame: &mut Frame<'_>,
    area: Rect,
    left: &[(&str, &str)],
    right: &[(&str, &str)],
) {
    let right = footer_hints(right);
    let right_width = u16::try_from(right.width()).unwrap_or(u16::MAX);
    let left_width = area.width.saturating_sub(right_width.saturating_add(2));
    let mut visible = left.to_vec();
    // Keep the action hints readable on compact terminals. Paging and arrow
    // controls are also documented in contextual Help and can be omitted from a
    // crowded footer, paging first because the arrows reach the same lines.
    for key in ["pgup/pgdn", "↑/↓", "←/→"] {
        if footer_hints(&visible).width() <= usize::from(left_width) {
            break;
        }
        visible.retain(|(hint, _)| *hint != key);
    }
    while footer_hints(&visible).width() > usize::from(left_width) {
        visible.pop();
    }
    frame.render_widget(
        Paragraph::new(footer_hints(&visible)).style(footer_label_style()),
        area,
    );
    frame.render_widget(
        Paragraph::new(right)
            .style(footer_label_style())
            .right_aligned(),
        Rect::new(
            area.right().saturating_sub(right_width),
            area.y,
            right_width.min(area.width),
            area.height,
        ),
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

/// Mutes navigation while a field or dialog holds focus.
pub(crate) fn dim_region(frame: &mut Frame<'_>, area: Rect) {
    frame
        .buffer_mut()
        .set_style(area, Style::new().add_modifier(Modifier::DIM));
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
        assert!(row.starts_with("tab switch  ↑/↓ move  enter select"));
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
        assert!(row.starts_with("tab/shift+tab switch  / commands"));
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
