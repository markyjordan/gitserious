use ratatui::crossterm::event::{
    Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};

use super::navigation::Section;
use super::{Action, Capabilities, SessionRequest, Shell};
use crate::commit::CommitRequest;
use crate::config::{ConfigRequest, ConfigView, ShellKeys};
use crate::help::HelpPage;

impl Shell {
    /// Handles `event` and returns what only the session loop can do.
    pub(crate) fn session_event(
        &mut self,
        event: Event,
        capabilities: Capabilities<'_>,
        help: &[HelpPage],
    ) -> Option<SessionRequest> {
        match self.handle_event(event, capabilities, help)? {
            Action::Quit => Some(SessionRequest::Quit),
            Action::Commit(CommitRequest::Author) => Some(SessionRequest::Author { resume: false }),
            Action::Commit(CommitRequest::Resume) => Some(SessionRequest::Author { resume: true }),
            Action::Select(_) | Action::Home(_) | Action::Commit(_) | Action::Initialize => None,
        }
    }

    /// Routes `event` in order: key releases are dropped, then an open root overlay,
    /// an open page modal, a too-small terminal, tab clicks, the root's keys, and
    /// finally the page take it.
    pub(super) fn handle_event(
        &mut self,
        event: Event,
        capabilities: Capabilities<'_>,
        help: &[HelpPage],
    ) -> Option<Action> {
        if matches!(&event, Event::Key(key) if key.kind == KeyEventKind::Release) {
            return None;
        }
        if self.overlay.is_some() {
            return self.handle_overlay(event, capabilities);
        }
        if self.commit.modal() {
            self.commit.modal_event(&event);
            return None;
        }
        if self.too_small && self.section == Section::Config && self.config.modal() {
            return None;
        }
        if self.section == Section::Config && self.config.modal() {
            return self.config_event(event, capabilities);
        }
        if let Event::Mouse(mouse) = &event
            && mouse.kind == MouseEventKind::Down(MouseButton::Left)
            && !self.too_small
        {
            if let Some(section) = self.tabs.at(mouse.column, mouse.row) {
                return self.execute(Action::Select(section), capabilities);
            }
            match self.section {
                Section::Commit => self.commit.click(mouse.column, mouse.row),
                Section::Help => self.help.click(mouse.column, mouse.row),
                Section::Home | Section::Config => {}
            }
        }
        if let Event::Key(key) = &event {
            let control = key.modifiers.contains(KeyModifiers::CONTROL);
            if control && key.code == KeyCode::Char('c') {
                return self.execute(Action::Quit, capabilities);
            }
            if self.too_small {
                return if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                    self.execute(Action::Quit, capabilities)
                } else {
                    None
                };
            }
            if self.section == Section::Config {
                match (self.config.shell_keys(), key.code) {
                    (ShellKeys::Navigation, KeyCode::Tab | KeyCode::BackTab) => {
                        return self.execute(
                            Action::Select(self.section.next(key.code == KeyCode::BackTab)),
                            capabilities,
                        );
                    }
                    (ShellKeys::Navigation, KeyCode::Esc) => {
                        return self.execute(Action::Select(Section::Home), capabilities);
                    }
                    (ShellKeys::Navigation, KeyCode::Char('q')) => {
                        return self.execute(Action::Quit, capabilities);
                    }
                    _ => {}
                }
            } else {
                if self.section == Section::Home && self.home.captures(key) {
                    let request = self.home.key(*key, capabilities.project)?;
                    return self.execute(Action::Home(request), capabilities);
                }
                match key.code {
                    KeyCode::Tab | KeyCode::BackTab => {
                        return self.execute(
                            Action::Select(self.section.next(key.code == KeyCode::BackTab)),
                            capabilities,
                        );
                    }
                    KeyCode::Esc if self.section != Section::Home => {
                        return self.execute(Action::Select(Section::Home), capabilities);
                    }
                    KeyCode::Char('q') | KeyCode::Esc => {
                        return self.execute(Action::Quit, capabilities);
                    }
                    KeyCode::Char('/') => {
                        self.open_palette();
                        return None;
                    }
                    _ => {}
                }
                match self.section {
                    Section::Home => {
                        let request = self.home.key(*key, capabilities.project)?;
                        return self.execute(Action::Home(request), capabilities);
                    }
                    Section::Commit => {
                        let request = self.commit.key(key.code)?;
                        return self.execute(Action::Commit(request), capabilities);
                    }
                    Section::Help => self.help.key(key.code, help.len()),
                    Section::Config => {}
                }
            }
        }
        if self.section == Section::Config {
            return self.config_event(event, capabilities);
        }
        None
    }

    fn config_event(&mut self, event: Event, capabilities: Capabilities<'_>) -> Option<Action> {
        match self.config.handle_event(event, capabilities.config) {
            Some(ConfigRequest::Quit) => return self.execute(Action::Quit, capabilities),
            Some(ConfigRequest::Initialize) => {
                self.home.refresh(capabilities.project);
                if self.home.can_initialize() {
                    return self.execute(Action::Initialize, capabilities);
                }
                self.config = ConfigView::new(capabilities.config);
            }
            None => {}
        }
        None
    }
}
