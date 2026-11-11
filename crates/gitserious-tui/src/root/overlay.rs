use ratatui::Frame;
use ratatui::crossterm::event::Event;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Clear};

use super::commands::{Palette, Step};
use super::{Action, Capabilities, Root};
use crate::confirmation::{self, Answer, Confirmation, DISCARD_AND_QUIT};
use crate::theme::{
    FOOTER_HEIGHT, JET_BLACK, centered_rect, frame_style, render_footer, render_footer_block,
};

fn quit_question() -> Confirmation<'static> {
    Confirmation {
        title: "Quit",
        message: "Discard unapplied changes and quit?".into(),
        choices: DISCARD_AND_QUIT,
    }
}

/// The root's own overlays, which every page shares.
pub(super) enum Overlay {
    Palette(Palette),
    QuitConfirm,
}

impl Root {
    pub(super) fn open_palette(&mut self) {
        self.overlay = Some(Overlay::Palette(Palette::default()));
    }

    pub(super) fn handle_overlay(
        &mut self,
        event: Event,
        capabilities: Capabilities<'_>,
    ) -> Option<Action> {
        let overlay = self.overlay.take()?;
        match overlay {
            Overlay::QuitConfirm => match self.quit_dialog.handle_event(DISCARD_AND_QUIT, &event) {
                Some(Answer::Confirm) => return Some(Action::Quit),
                Some(Answer::Keep | Answer::Alternate) => self.quit_dialog.invalidate(),
                None => self.overlay = Some(Overlay::QuitConfirm),
            },
            Overlay::Palette(mut palette) => match palette.event(event, self.commands()) {
                Step::Keep => self.overlay = Some(Overlay::Palette(palette)),
                Step::Close => {}
                Step::Run(action) => return self.execute(action, capabilities),
            },
        }
        None
    }

    pub(super) fn render_overlay(&mut self, frame: &mut Frame<'_>, area: Rect) {
        self.quit_dialog.invalidate();
        if matches!(self.overlay, Some(Overlay::QuitConfirm)) {
            self.quit_dialog.render(frame, &quit_question());
            let footer = Rect::new(
                area.x,
                area.bottom().saturating_sub(FOOTER_HEIGHT),
                area.width,
                FOOTER_HEIGHT,
            );
            let footer = render_footer_block(frame, footer);
            render_footer(frame, footer, &confirmation::hints(DISCARD_AND_QUIT), &[]);
            return;
        }
        let commands = self.commands();
        if self.overlay.is_none() && !self.commit.modal() {
            return;
        }
        let popup = centered_rect(54, 13.min(area.height), area);
        frame.render_widget(Clear, popup);
        frame.render_widget(
            Block::bordered()
                .border_style(frame_style())
                .style(Style::default().fg(Color::White).bg(JET_BLACK)),
            popup,
        );
        let inner = Block::bordered().inner(popup);
        match &mut self.overlay {
            None => self.commit.render_modal(frame, inner),
            Some(Overlay::QuitConfirm) => {}
            Some(Overlay::Palette(palette)) => palette.render(frame, inner, commands),
        }
    }
}
