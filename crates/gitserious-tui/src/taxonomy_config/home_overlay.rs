use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, BorderType, Clear, List, ListItem, ListState, Paragraph};

use super::Category;
use crate::theme::{
    JET_BLACK, block_title, centered_rect, frame_style, navigation_key_style, section_heading_style,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum HomeCommand {
    Configure,
    Initialize,
    ReviewProject,
    New,
    Edit,
    Fork,
    Delete,
    ReviewLibrary,
    SwitchTab,
    Quit,
}

impl HomeCommand {
    fn label(self, category: Category) -> &'static str {
        if category == Category::Taxonomies {
            return match self {
                Self::New => "create taxonomy",
                Self::Edit => "edit taxonomy",
                Self::Fork => "fork taxonomy",
                Self::Delete => "delete taxonomy",
                Self::ReviewLibrary => "review changes",
                Self::SwitchTab => "switch to project",
                Self::Quit => "quit",
                _ => unreachable!("Project commands are not shown in Taxonomies"),
            };
        }
        match self {
            Self::Configure => "Configure project",
            Self::Initialize => "Initialize project",
            Self::ReviewProject | Self::ReviewLibrary => "Review changes",
            Self::New => "New taxonomy",
            Self::Edit => "Edit taxonomy",
            Self::Fork => "Fork taxonomy",
            Self::Delete => "Delete taxonomy",
            Self::SwitchTab => match category {
                Category::Project => "Switch to Library",
                Category::Taxonomies => "Switch to Project",
            },
            Self::Quit => "Quit",
        }
    }

    fn shortcut(self) -> &'static str {
        match self {
            Self::Configure => "enter",
            Self::Initialize => "i",
            Self::ReviewProject | Self::ReviewLibrary => "ctrl+s",
            Self::New => "n",
            Self::Edit => "e",
            Self::Fork => "f",
            Self::Delete => "d",
            Self::SwitchTab => "",
            Self::Quit => "q",
        }
    }
}

const PROJECT_COMMANDS: &[HomeCommand] = &[
    HomeCommand::Configure,
    HomeCommand::Initialize,
    HomeCommand::ReviewProject,
    HomeCommand::SwitchTab,
    HomeCommand::Quit,
];
const LIBRARY_COMMANDS: &[HomeCommand] = &[
    HomeCommand::New,
    HomeCommand::Edit,
    HomeCommand::Fork,
    HomeCommand::Delete,
    HomeCommand::ReviewLibrary,
    HomeCommand::SwitchTab,
    HomeCommand::Quit,
];

fn commands(category: Category) -> &'static [HomeCommand] {
    match category {
        Category::Project => PROJECT_COMMANDS,
        Category::Taxonomies => LIBRARY_COMMANDS,
    }
}

#[derive(Debug, Default)]
pub(super) struct CommandPaletteState {
    pub(super) query: String,
    pub(super) selected: usize,
    pub(super) list_area: Rect,
    pub(super) list_offset: usize,
}

impl CommandPaletteState {
    pub(super) fn visible(&self, category: Category) -> Vec<HomeCommand> {
        let query = self.query.to_ascii_lowercase();
        commands(category)
            .iter()
            .copied()
            .filter(|command| {
                command
                    .label(category)
                    .to_ascii_lowercase()
                    .contains(&query)
                    || command.shortcut().contains(&query)
            })
            .collect()
    }

    pub(super) fn selected_command(&self, category: Category) -> Option<HomeCommand> {
        self.visible(category).get(self.selected).copied()
    }

    pub(super) fn move_selection(&mut self, category: Category, down: bool) {
        let last = self.visible(category).len().saturating_sub(1);
        self.selected = if down {
            self.selected.saturating_add(1).min(last)
        } else {
            self.selected.saturating_sub(1)
        };
    }

    pub(super) fn set_query(&mut self, query: String) {
        self.query = query;
        self.selected = 0;
        self.list_offset = 0;
    }
}

impl CommandPaletteState {
    pub(super) fn render(&mut self, frame: &mut Frame<'_>, area: Rect, category: Category) {
        let entries = self
            .visible(category)
            .iter()
            .map(|command| (command.label(category), command.shortcut()))
            .collect();
        render_palette(frame, area, self, entries);
    }
}

fn popup_block() -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Double)
        .border_style(frame_style())
        .style(Style::default().bg(JET_BLACK))
}

fn render_palette(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &mut CommandPaletteState,
    visible: Vec<(&'static str, &'static str)>,
) {
    let height = u16::try_from(visible.len().max(1))
        .unwrap_or(u16::MAX)
        .saturating_add(7)
        .min(area.height);
    let popup = centered_rect(58, height, area);
    frame.render_widget(Clear, popup);
    frame.render_widget(popup_block(), popup);
    let content_x = popup.x.saturating_add(2);
    let content_width = popup.width.saturating_sub(4);
    frame.render_widget(
        Paragraph::new("Commands")
            .style(section_heading_style())
            .centered(),
        Rect::new(content_x, popup.y.saturating_add(1), content_width, 1),
    );
    frame.render_widget(
        Paragraph::new(format!("/{}▌", state.query)).block(
            Block::bordered()
                .border_style(frame_style())
                .title(block_title("Search")),
        ),
        Rect::new(
            content_x.saturating_sub(1),
            popup.y.saturating_add(2),
            content_width.saturating_add(2),
            3,
        ),
    );
    let list_area = Rect::new(
        content_x,
        popup.y.saturating_add(5),
        content_width,
        popup.height.saturating_sub(7),
    );
    state.list_area = list_area;
    let rows = if visible.is_empty() {
        vec![ListItem::new("No matching commands")]
    } else {
        visible
            .iter()
            .map(|(label, shortcut)| {
                let gap = usize::from(list_area.width)
                    .saturating_sub(label.chars().count() + shortcut.len());
                ListItem::new(format!("{label}{}{shortcut}", " ".repeat(gap)))
            })
            .collect::<Vec<_>>()
    };
    let mut list_state = ListState::default();
    if !visible.is_empty() {
        state.selected = state.selected.min(visible.len() - 1);
        list_state.select(Some(state.selected));
    }
    frame.render_stateful_widget(
        List::new(rows).highlight_style(navigation_key_style()),
        list_area,
        &mut list_state,
    );
    state.list_offset = list_state.offset();
    frame.render_widget(
        Paragraph::new("[↑/↓] select  [enter] run  [esc] close").centered(),
        Rect::new(
            content_x,
            popup.bottom().saturating_sub(2),
            content_width,
            1,
        ),
    );
}
