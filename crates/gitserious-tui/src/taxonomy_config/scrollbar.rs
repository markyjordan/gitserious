use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

/// Draws the commit editor's track and thumb on a framed pane only on overflow.
pub(super) fn render_overflow_scrollbar(
    frame: &mut Frame<'_>,
    pane: Rect,
    content_rows: u16,
    viewport_rows: u16,
    viewport_top: u16,
) {
    if content_rows <= viewport_rows || pane.width == 0 || pane.height <= 2 {
        return;
    }
    let x = pane.right() - 1;
    let track_top = pane.y + 1;
    let track_length = pane.height - 2;
    for y in track_top..track_top + track_length {
        frame.buffer_mut()[(x, y)]
            .set_symbol("│")
            .set_style(Style::default().fg(Color::DarkGray));
    }
    let thumb_length = u16::try_from(
        u32::from(viewport_rows)
            .saturating_mul(u32::from(track_length))
            .div_ceil(u32::from(content_rows)),
    )
    .unwrap_or(track_length)
    .clamp(1, track_length);
    let maximum_position = content_rows.saturating_sub(viewport_rows);
    let maximum_thumb_offset = track_length.saturating_sub(thumb_length);
    let position = viewport_top.min(maximum_position);
    let thumb_offset = if maximum_position == 0 {
        0
    } else {
        u16::try_from(
            (u32::from(position)
                .saturating_mul(u32::from(maximum_thumb_offset))
                .saturating_add(u32::from(maximum_position) / 2))
                / u32::from(maximum_position),
        )
        .unwrap_or(maximum_thumb_offset)
        .min(maximum_thumb_offset)
    };
    for row in thumb_offset..thumb_offset + thumb_length {
        frame.buffer_mut()[(x, track_top + row)]
            .set_symbol("█")
            .set_style(Style::default().fg(Color::Yellow));
    }
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::widgets::Block;

    use super::*;

    #[test]
    fn thumb_tracks_top_middle_and_bottom_only_when_content_overflows()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut terminal = Terminal::new(TestBackend::new(12, 12))?;
        let pane = Rect::new(0, 0, 10, 10);
        let thumb_rows = |terminal: &Terminal<TestBackend>| {
            (1..9)
                .filter(|&y| terminal.backend().buffer()[(9, y)].symbol() == "█")
                .collect::<Vec<_>>()
        };
        terminal.draw(|frame| {
            frame.render_widget(Block::bordered(), pane);
            render_overflow_scrollbar(frame, pane, 8, 8, 0);
        })?;
        assert!(thumb_rows(&terminal).is_empty());
        terminal.draw(|frame| {
            frame.render_widget(Block::bordered(), pane);
            render_overflow_scrollbar(frame, pane, 24, 8, 0);
        })?;
        let top = thumb_rows(&terminal);
        assert_eq!(top.first().copied(), Some(1));
        terminal.draw(|frame| {
            frame.render_widget(Block::bordered(), pane);
            render_overflow_scrollbar(frame, pane, 24, 8, 8);
        })?;
        let middle = thumb_rows(&terminal);
        terminal.draw(|frame| {
            frame.render_widget(Block::bordered(), pane);
            render_overflow_scrollbar(frame, pane, 24, 8, 16);
        })?;
        let bottom = thumb_rows(&terminal);
        assert!(top.first() < middle.first());
        assert!(middle.first() < bottom.first());
        assert_eq!(bottom.last().copied(), Some(8));
        assert!(
            (1..9).all(|y| terminal.backend().buffer()[(9, y)].fg == Color::Yellow
                || terminal.backend().buffer()[(9, y)].fg == Color::DarkGray)
        );
        Ok(())
    }
}
