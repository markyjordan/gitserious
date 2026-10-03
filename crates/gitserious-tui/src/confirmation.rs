use ratatui::Frame;
use ratatui::crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Clear, Paragraph, Wrap};

use crate::theme::{
    JET_BLACK, centered_rect, frame_style, navigation_style, section_heading_style,
};

const DISCARD: &str = "y: discard";
const KEEP_EDITING: &str = "enter/esc/n: keep editing";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DiscardChoice {
    Discard,
    KeepEditing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DiscardButtons {
    pub(crate) discard: Rect,
    pub(crate) keep_editing: Rect,
}

#[derive(Default, Debug)]
pub(crate) struct DiscardDialog {
    pub(crate) buttons: Option<DiscardButtons>,
}

impl DiscardDialog {
    pub(crate) fn invalidate(&mut self) {
        self.buttons = None;
    }

    pub(crate) fn handle_event(&mut self, event: &Event) -> Option<DiscardChoice> {
        match event {
            Event::Resize(_, _) => {
                self.invalidate();
                None
            }
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Char('y') => Some(DiscardChoice::Discard),
                KeyCode::Enter | KeyCode::Esc | KeyCode::Char('n') => {
                    Some(DiscardChoice::KeepEditing)
                }
                _ => None,
            },
            Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => {
                let buttons = self.buttons?;
                if buttons.discard.contains((mouse.column, mouse.row).into()) {
                    Some(DiscardChoice::Discard)
                } else if buttons
                    .keep_editing
                    .contains((mouse.column, mouse.row).into())
                {
                    Some(DiscardChoice::KeepEditing)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    pub(crate) fn render(&mut self, frame: &mut Frame<'_>, area: Rect, title: &str, message: &str) {
        self.invalidate();
        let popup = centered_rect(58, 9.min(area.height), area);
        frame.render_widget(Clear, popup);
        frame.render_widget(
            Block::bordered()
                .border_type(BorderType::Double)
                .border_style(frame_style())
                .style(Style::default().fg(Color::White).bg(JET_BLACK)),
            popup,
        );
        let discard_width = button_width(DISCARD);
        let keep_width = button_width(KEEP_EDITING);
        let content_width = popup.width.saturating_sub(4);
        if popup.height < 8 || content_width < discard_width + keep_width + 1 {
            frame.render_widget(
                Paragraph::new("Resize or use y / enter / esc / n").wrap(Wrap { trim: false }),
                Rect::new(
                    popup.x + 1,
                    popup.y + 1,
                    popup.width.saturating_sub(2),
                    popup.height.saturating_sub(2),
                ),
            );
            return;
        }
        let content_x = popup.x + 2;
        let content_y = popup.y + 2;
        frame.render_widget(
            Paragraph::new(title)
                .centered()
                .style(section_heading_style()),
            Rect::new(content_x, content_y, content_width, 1),
        );
        frame.render_widget(
            Paragraph::new(message)
                .centered()
                .wrap(Wrap { trim: false }),
            Rect::new(content_x, content_y + 2, content_width, 2),
        );
        let row = Rect::new(content_x, content_y + 4, content_width, 1);
        let [discard, _, keep_editing] = Layout::horizontal([
            Constraint::Length(discard_width),
            Constraint::Length(1),
            Constraint::Length(keep_width),
        ])
        .flex(Flex::Center)
        .areas(row);
        frame.render_widget(
            Paragraph::new(format!(" {DISCARD} ")).style(navigation_style()),
            discard,
        );
        frame.render_widget(
            Paragraph::new(format!(" {KEEP_EDITING} ")).style(navigation_style()),
            keep_editing,
        );
        self.buttons = Some(DiscardButtons {
            discard,
            keep_editing,
        });
    }
}

fn button_width(label: &str) -> u16 {
    u16::try_from(Line::from(label).width())
        .unwrap_or(u16::MAX)
        .saturating_add(2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{KeyEvent, KeyModifiers, MouseEvent};

    fn click(area: Rect) -> Event {
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: area.x,
            row: area.y,
            modifiers: KeyModifiers::NONE,
        })
    }

    #[test]
    fn shared_dialog_renders_clickable_buttons_and_resolves_shortcuts()
    -> Result<(), Box<dyn std::error::Error>> {
        for (width, height) in [(60, 18), (120, 30)] {
            let mut dialog = DiscardDialog::default();
            let mut terminal = Terminal::new(TestBackend::new(width, height))?;
            terminal.draw(|frame| {
                let area = frame.area();
                dialog.render(frame, area, "Discard Property", "Discard property changes?");
            })?;
            let buttons = dialog.buttons.ok_or("missing buttons")?;
            for area in [buttons.discard, buttons.keep_editing] {
                assert!(area.right() <= width && area.bottom() <= height);
                for x in area.x..area.right() {
                    assert_eq!(terminal.backend().buffer()[(x, area.y)].bg, Color::Yellow);
                    assert_eq!(terminal.backend().buffer()[(x, area.y)].fg, JET_BLACK);
                }
            }
            assert_eq!(
                dialog.handle_event(&click(buttons.discard)),
                Some(DiscardChoice::Discard)
            );
            assert_eq!(
                dialog.handle_event(&click(buttons.keep_editing)),
                Some(DiscardChoice::KeepEditing)
            );
            for code in [KeyCode::Enter, KeyCode::Esc, KeyCode::Char('n')] {
                assert_eq!(
                    dialog.handle_event(&Event::Key(KeyEvent::new(code, KeyModifiers::NONE))),
                    Some(DiscardChoice::KeepEditing)
                );
            }
            assert_eq!(
                dialog.handle_event(&Event::Key(KeyEvent::new(
                    KeyCode::Char('y'),
                    KeyModifiers::NONE
                ))),
                Some(DiscardChoice::Discard)
            );
            assert_eq!(dialog.handle_event(&click(Rect::new(0, 0, 1, 1))), None);
            assert_eq!(dialog.handle_event(&Event::Paste("ignored".into())), None);
            assert_eq!(
                dialog.handle_event(&Event::Key(KeyEvent::new(
                    KeyCode::Down,
                    KeyModifiers::NONE
                ))),
                None
            );
            let mut release = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE);
            release.kind = KeyEventKind::Release;
            assert_eq!(dialog.handle_event(&Event::Key(release)), None);
        }
        Ok(())
    }

    #[test]
    fn resizing_or_hiding_buttons_invalidates_mouse_targets()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut dialog = DiscardDialog::default();
        let mut terminal = Terminal::new(TestBackend::new(60, 18))?;
        terminal.draw(|frame| {
            let area = frame.area();
            dialog.render(frame, area, "Discard Type", "Discard type changes?");
        })?;
        let old = dialog.buttons.ok_or("missing buttons")?;
        dialog.handle_event(&Event::Resize(30, 7));
        assert!(dialog.buttons.is_none());
        assert_eq!(dialog.handle_event(&click(old.discard)), None);
        for (width, height) in [(30, 7), (1, 1)] {
            let mut tiny = Terminal::new(TestBackend::new(width, height))?;
            tiny.draw(|frame| {
                let area = frame.area();
                dialog.render(frame, area, "Discard Type", "Discard type changes?");
            })?;
            assert!(dialog.buttons.is_none());
            assert_eq!(dialog.handle_event(&click(old.keep_editing)), None);
            assert_eq!(
                dialog.handle_event(&Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))),
                Some(DiscardChoice::KeepEditing)
            );
        }
        terminal.draw(|frame| {
            let area = frame.area();
            dialog.render(frame, area, "Discard Type", "Discard type changes?");
        })?;
        dialog.invalidate();
        assert_eq!(dialog.handle_event(&click(old.discard)), None);
        Ok(())
    }
}
