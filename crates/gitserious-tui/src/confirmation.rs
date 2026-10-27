use ratatui::Frame;
use ratatui::crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Clear, Paragraph, Wrap};

use crate::theme::{
    JET_BLACK, centered_rect, frame_style, navigation_style, section_heading_style,
};

/// What a confirmation's choice does.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Answer {
    /// The action the dialog asks about.
    Confirm,
    /// The second action a three-way dialog offers.
    Alternate,
    /// Back to where the user was, with nothing changed.
    Keep,
}

/// One answer, the keys that choose it, and the hint its button and the footer both show.
#[derive(Debug)]
pub(crate) struct Choice {
    keys: &'static [KeyCode],
    hint: &'static str,
    label: &'static str,
    answer: Answer,
}

const YES: &[KeyCode] = &[KeyCode::Char('y')];
const KEEP_KEYS: &[KeyCode] = &[KeyCode::Enter, KeyCode::Esc, KeyCode::Char('n')];

const fn confirm(label: &'static str) -> Choice {
    Choice {
        keys: YES,
        hint: "y",
        label,
        answer: Answer::Confirm,
    }
}

const fn keep(label: &'static str) -> Choice {
    Choice {
        keys: KEEP_KEYS,
        hint: "enter/esc/n",
        label,
        answer: Answer::Keep,
    }
}

pub(crate) const DISCARD: &[Choice] = &[confirm("discard"), keep("keep editing")];

/// A question asked before work is dropped, removed, or applied. The message names what it
/// affects.
pub(crate) struct Confirmation<'a> {
    pub(crate) title: &'a str,
    pub(crate) message: String,
    pub(crate) choices: &'static [Choice],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DialogButtons {
    pub(crate) confirm: Rect,
    pub(crate) keep: Rect,
    pub(crate) alternate: Option<Rect>,
}

#[derive(Default, Debug)]
pub(crate) struct ConfirmDialog {
    pub(crate) buttons: Option<DialogButtons>,
}

impl ConfirmDialog {
    pub(crate) fn invalidate(&mut self) {
        self.buttons = None;
    }

    pub(crate) fn handle_event(&mut self, choices: &[Choice], event: &Event) -> Option<Answer> {
        match event {
            Event::Resize(_, _) => {
                self.invalidate();
                None
            }
            Event::Key(key) if key.kind == KeyEventKind::Press => choices
                .iter()
                .find(|choice| choice.keys.contains(&key.code))
                .map(|choice| choice.answer),
            Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => {
                let buttons = self.buttons?;
                [
                    (Some(buttons.confirm), Answer::Confirm),
                    (Some(buttons.keep), Answer::Keep),
                    (buttons.alternate, Answer::Alternate),
                ]
                .into_iter()
                .find(|(area, _)| {
                    area.is_some_and(|area| area.contains((mouse.column, mouse.row).into()))
                })
                .map(|(_, answer)| answer)
            }
            _ => None,
        }
    }

    pub(crate) fn render(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        question: &Confirmation<'_>,
    ) {
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
        let labels = question
            .choices
            .iter()
            .map(|choice| format!(" {}: {} ", choice.hint, choice.label))
            .collect::<Vec<_>>();
        let widths = labels
            .iter()
            .map(|label| button_width(label))
            .collect::<Vec<_>>();
        let gaps = u16::try_from(labels.len().saturating_sub(1)).unwrap_or(u16::MAX);
        let needed = widths.iter().sum::<u16>().saturating_add(gaps);
        let content_width = popup.width.saturating_sub(4);
        if popup.height < 8 || content_width < needed {
            let keys = question
                .choices
                .iter()
                .map(|choice| choice.hint)
                .collect::<Vec<_>>()
                .join(", ");
            frame.render_widget(
                Paragraph::new(format!("Resize or press {keys}")).wrap(Wrap { trim: false }),
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
            Paragraph::new(question.title)
                .centered()
                .style(section_heading_style()),
            Rect::new(content_x, content_y, content_width, 1),
        );
        frame.render_widget(
            Paragraph::new(question.message.as_str())
                .centered()
                .wrap(Wrap { trim: false }),
            Rect::new(content_x, content_y + 2, content_width, 2),
        );
        let row = Rect::new(content_x, content_y + 4, content_width, 1);
        let constraints = widths
            .iter()
            .enumerate()
            .flat_map(|(index, width)| {
                let gap = (index > 0).then_some(Constraint::Length(1));
                gap.into_iter().chain([Constraint::Length(*width)])
            })
            .collect::<Vec<_>>();
        let areas = Layout::horizontal(constraints)
            .flex(Flex::Center)
            .split(row);
        let mut confirm = None;
        let mut keep = None;
        let mut alternate = None;
        for ((choice, label), area) in question
            .choices
            .iter()
            .zip(&labels)
            .zip(areas.iter().step_by(2))
        {
            frame.render_widget(
                Paragraph::new(label.as_str()).style(navigation_style()),
                *area,
            );
            match choice.answer {
                Answer::Confirm => confirm = Some(*area),
                Answer::Keep => keep = Some(*area),
                Answer::Alternate => alternate = Some(*area),
            }
        }
        if let (Some(confirm), Some(keep)) = (confirm, keep) {
            self.buttons = Some(DialogButtons {
                confirm,
                keep,
                alternate,
            });
        }
    }
}

fn button_width(label: &str) -> u16 {
    u16::try_from(Line::from(label).width()).unwrap_or(u16::MAX)
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

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn question<'a>(title: &'a str, message: &str, choices: &'static [Choice]) -> Confirmation<'a> {
        Confirmation {
            title,
            message: message.into(),
            choices,
        }
    }

    #[test]
    fn shared_dialog_renders_clickable_buttons_and_resolves_shortcuts()
    -> Result<(), Box<dyn std::error::Error>> {
        let discard = question("Discard Property", "Discard property changes?", DISCARD);
        for (width, height) in [(60, 18), (120, 30)] {
            let mut dialog = ConfirmDialog::default();
            let mut terminal = Terminal::new(TestBackend::new(width, height))?;
            terminal.draw(|frame| {
                let area = frame.area();
                dialog.render(frame, area, &discard);
            })?;
            let buttons = dialog.buttons.ok_or("missing buttons")?;
            assert_eq!(buttons.alternate, None);
            for area in [buttons.confirm, buttons.keep] {
                assert!(area.right() <= width && area.bottom() <= height);
                for x in area.x..area.right() {
                    assert_eq!(terminal.backend().buffer()[(x, area.y)].bg, Color::Yellow);
                    assert_eq!(terminal.backend().buffer()[(x, area.y)].fg, JET_BLACK);
                }
            }
            assert_eq!(
                dialog.handle_event(DISCARD, &click(buttons.confirm)),
                Some(Answer::Confirm)
            );
            assert_eq!(
                dialog.handle_event(DISCARD, &click(buttons.keep)),
                Some(Answer::Keep)
            );
            for code in [KeyCode::Enter, KeyCode::Esc, KeyCode::Char('n')] {
                assert_eq!(dialog.handle_event(DISCARD, &key(code)), Some(Answer::Keep));
            }
            assert_eq!(
                dialog.handle_event(DISCARD, &key(KeyCode::Char('y'))),
                Some(Answer::Confirm)
            );
            assert_eq!(
                dialog.handle_event(DISCARD, &click(Rect::new(0, 0, 1, 1))),
                None
            );
            assert_eq!(
                dialog.handle_event(DISCARD, &Event::Paste("ignored".into())),
                None
            );
            assert_eq!(dialog.handle_event(DISCARD, &key(KeyCode::Down)), None);
            let mut release = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE);
            release.kind = KeyEventKind::Release;
            assert_eq!(dialog.handle_event(DISCARD, &Event::Key(release)), None);
        }
        Ok(())
    }

    #[test]
    fn resizing_or_hiding_buttons_invalidates_mouse_targets()
    -> Result<(), Box<dyn std::error::Error>> {
        let discard = question("Discard Type", "Discard type changes?", DISCARD);
        let mut dialog = ConfirmDialog::default();
        let mut terminal = Terminal::new(TestBackend::new(60, 18))?;
        terminal.draw(|frame| {
            let area = frame.area();
            dialog.render(frame, area, &discard);
        })?;
        let old = dialog.buttons.ok_or("missing buttons")?;
        dialog.handle_event(DISCARD, &Event::Resize(30, 7));
        assert!(dialog.buttons.is_none());
        assert_eq!(dialog.handle_event(DISCARD, &click(old.confirm)), None);
        for (width, height) in [(30, 7), (1, 1)] {
            let mut tiny = Terminal::new(TestBackend::new(width, height))?;
            tiny.draw(|frame| {
                let area = frame.area();
                dialog.render(frame, area, &discard);
            })?;
            assert!(dialog.buttons.is_none());
            assert_eq!(dialog.handle_event(DISCARD, &click(old.keep)), None);
            assert_eq!(
                dialog.handle_event(DISCARD, &key(KeyCode::Esc)),
                Some(Answer::Keep)
            );
        }
        terminal.draw(|frame| {
            let area = frame.area();
            dialog.render(frame, area, &discard);
        })?;
        dialog.invalidate();
        assert_eq!(dialog.handle_event(DISCARD, &click(old.confirm)), None);
        Ok(())
    }
}
