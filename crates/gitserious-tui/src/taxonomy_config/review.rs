//! Project review and apply. Library edits use direct persistence.

use gitserious_app::{ConfigurationDestination, ConfigurationSession, ConfigurationWorkspace};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Paragraph, Wrap};

use super::{Notice, Screen, State, hanging_wrap};
use crate::theme::{block_title, frame_style};

impl State {
    pub(super) fn open_review(&mut self) {
        let destination = self.active_scope;
        let should_review = self
            .session(destination)
            .is_some_and(ConfigurationSession::is_dirty);
        if should_review {
            if let Some(error) = self
                .session(destination)
                .and_then(|session| session.review().err())
            {
                self.status = Some(Notice::error(error));
            } else {
                self.scroll = 0;
                self.screen = Screen::Review(destination);
            }
        } else {
            self.status = Some(Notice::info("No changes to apply.".into()));
        }
    }

    pub(super) fn review_key(
        &mut self,
        key: KeyEvent,
        destination: ConfigurationDestination,
        workspace: &dyn ConfigurationWorkspace,
    ) -> bool {
        match key.code {
            KeyCode::Enter if self.refuse_apply(destination) => {}
            KeyCode::Enter => {
                let Some(session) = self.session(destination).cloned() else {
                    return false;
                };
                match workspace.save(&session) {
                    Ok(saved) => {
                        self.replace_session(destination, Ok(saved));
                        self.screen = Screen::Home;
                        self.status = Some(Notice::info("Configuration applied.".into()));
                    }
                    Err(error) => {
                        self.screen = Screen::Home;
                        self.status = Some(Notice::error(error));
                    }
                }
            }
            KeyCode::Esc => self.screen = Screen::Home,
            KeyCode::Up => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::Down => self.scroll = self.scroll.saturating_add(1),
            _ => {}
        }
        false
    }

    /// Says why `destination` cannot apply while its review lists problems. Returns whether it
    /// refused.
    pub(super) fn refuse_apply(&mut self, destination: ConfigurationDestination) -> bool {
        if let Some(error) = self
            .session(destination)
            .and_then(|session| session.review().err())
        {
            self.status = Some(Notice::error(error));
            true
        } else {
            false
        }
    }

    /// Draws the review and returns its footer hints, which leave out apply while it is
    /// disabled.
    pub(super) fn render_review(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        destination: ConfigurationDestination,
    ) -> Vec<(&'static str, &'static str)> {
        let width = usize::from(area.width.saturating_sub(2));
        let review = self
            .session(destination)
            .and_then(|session| session.review().ok());
        let blocked = review.is_none();
        let lines = review.map_or_else(
            || hanging_wrap("Configuration cannot be reviewed.", width),
            |review| hanging_wrap(&review.changes, width),
        );
        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
        let viewport_height = area.height.saturating_sub(2);
        let total_rows =
            u16::try_from(paragraph.line_count(area.width.saturating_sub(2))).unwrap_or(u16::MAX);
        self.scroll = self.scroll.min(total_rows.saturating_sub(viewport_height));
        let title = if blocked {
            "Review changes — apply disabled"
        } else {
            "Review changes — Enter to apply"
        };
        frame.render_widget(
            paragraph.scroll((self.scroll, 0)).block(
                Block::bordered()
                    .border_style(frame_style())
                    .title(block_title(title)),
            ),
            area,
        );
        let mut hints = vec![("esc", "back"), ("↑/↓", "scroll")];
        if !blocked {
            hints.insert(0, ("enter", "apply"));
        }
        hints
    }
}
