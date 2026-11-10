use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::Paragraph;

use crate::theme::theme;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Section {
    Home,
    Commit,
    Config,
    Help,
}
impl Section {
    pub(super) const ALL: [Self; 4] = [Self::Home, Self::Commit, Self::Config, Self::Help];
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Home => "Home",
            Self::Config => "Config",
            Self::Commit => "Commit",
            Self::Help => "Help",
        }
    }
    fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|section| *section == self)
            .unwrap_or(0)
    }
    pub(super) fn next(self, reverse: bool) -> Self {
        let count = Self::ALL.len();
        Self::ALL[(self.index() + if reverse { count - 1 } else { 1 }) % count]
    }
}

/// The header's page tabs, and where each was last drawn.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct Tabs([Rect; Section::ALL.len()]);

impl Tabs {
    /// The tab drawn under `column` and `row`, if any.
    pub(super) fn at(self, column: u16, row: u16) -> Option<Section> {
        Section::ALL
            .into_iter()
            .find(|section| self.0[section.index()].contains((column, row).into()))
    }

    /// Draws a tab per page from the left of `area`, marking `active`.
    pub(super) fn render(&mut self, frame: &mut Frame<'_>, area: Rect, active: Section) {
        let mut x = area.x.saturating_add(1);
        for section in Section::ALL {
            let label = section.label();
            let width = u16::try_from(label.len()).unwrap_or(0);
            let rect = Rect::new(x, area.y, width.min(area.right().saturating_sub(x)), 1);
            let style = if section == active {
                Style::new()
                    .fg(theme().text)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else {
                Style::new().fg(theme().dim)
            };
            frame.render_widget(Paragraph::new(label).style(style), rect);
            self.0[section.index()] = rect;
            x = x.saturating_add(width + 2);
        }
    }

    #[cfg(test)]
    pub(super) fn area(self, section: Section) -> Rect {
        self.0[section.index()]
    }
}
