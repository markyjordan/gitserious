use ratatui::Frame;
use ratatui::crossterm::event::{Event, KeyCode, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph};

use super::{Action, Section, Shell};
use crate::home::HomeAction;
use crate::theme::{block_title, command_hints, navigation_key_style};

impl Shell {
    /// The palette's commands on the current page, in the order it lists them.
    pub(super) fn commands(&self) -> Vec<(String, Action)> {
        let mut commands: Vec<_> = Section::ALL
            .into_iter()
            .map(|section| (format!("Open {}", section.label()), Action::Select(section)))
            .collect();
        if self.section == Section::Home && self.home.action().is_some_and(HomeAction::initializes)
        {
            commands.push(("Initialize gitserious".into(), Action::Initialize));
        }
        if self.section == Section::Commit {
            commands.push(("Refresh repository".into(), Action::Refresh));
            if !self.commit.notice.is_empty() {
                commands.push(("View last Git result".into(), Action::Result));
            }
            if self.commit.blocker.is_none() {
                commands.push(("Start commit".into(), Action::Author));
            }
            if self.commit.has_pending_review() {
                commands.push(("Resume review".into(), Action::Resume));
            }
        }
        commands.push(("Quit".into(), Action::Quit));
        commands
    }
}

/// The root command palette's search and selection.
#[derive(Default)]
pub(super) struct Palette {
    query: String,
    selected: usize,
    list: ListState,
}

/// What the root does with the palette after an event.
pub(super) enum Step {
    Keep,
    Close,
    Run(Action),
}

impl Palette {
    /// The `commands` whose labels contain the query, in their order.
    fn matching(&self, commands: Vec<(String, Action)>) -> Vec<(String, Action)> {
        let query = self.query.to_lowercase();
        commands
            .into_iter()
            .filter(|(label, _)| label.to_lowercase().contains(&query))
            .collect()
    }

    pub(super) fn event(&mut self, event: Event, commands: Vec<(String, Action)>) -> Step {
        if let Event::Key(key) = event {
            let commands = self.matching(commands);
            match key.code {
                KeyCode::Esc => return Step::Close,
                KeyCode::Up => self.selected = self.selected.saturating_sub(1),
                KeyCode::Down => {
                    self.selected = (self.selected + 1).min(commands.len().saturating_sub(1))
                }
                KeyCode::Enter => {
                    if let Some((_, action)) = commands.get(self.selected) {
                        return Step::Run(*action);
                    }
                }
                KeyCode::Backspace => {
                    self.query.pop();
                    self.selected = 0;
                }
                KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.query.push(character);
                    self.selected = 0;
                }
                _ => {}
            }
        } else if let Event::Paste(text) = event {
            self.query.push_str(&text);
            self.selected = 0;
        }
        Step::Keep
    }

    /// Draws the search, the matching `commands`, and the palette's keys in `inner`.
    pub(super) fn render(
        &mut self,
        frame: &mut Frame<'_>,
        inner: Rect,
        commands: Vec<(String, Action)>,
    ) {
        let [search, list, footer] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .areas(inner);
        frame.render_widget(
            Paragraph::new(format!("/ {}", self.query))
                .block(Block::bordered().title(block_title("Search"))),
            search,
        );
        let rows: Vec<_> = self
            .matching(commands)
            .into_iter()
            .map(|(label, _)| ListItem::new(label))
            .collect();
        self.selected = self.selected.min(rows.len().saturating_sub(1));
        self.list
            .select((!rows.is_empty()).then_some(self.selected));
        frame.render_stateful_widget(
            List::new(rows).highlight_style(navigation_key_style()),
            list,
            &mut self.list,
        );
        frame.render_widget(
            Paragraph::new(command_hints(&[
                ("↑/↓", "select"),
                ("enter", "run"),
                ("esc", "close"),
            ])),
            footer,
        );
    }
}
