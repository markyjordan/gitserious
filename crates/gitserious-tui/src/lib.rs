//! Ratatui-backed terminal interaction adapters for gitserious.

use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::io::{self, IsTerminal};

use gitserious_app::{CommitTypeSelection, CommitTypeSelector};
use gitserious_core::CommitTypeDefinition;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};

/// Ratatui implementation of interactive commit-type selection.
#[derive(Clone, Copy, Debug, Default)]
pub struct RatatuiCommitTypeSelector;

impl CommitTypeSelector for RatatuiCommitTypeSelector {
    type Error = CommitTypeSelectorError;

    fn select(
        &self,
        definitions: &[CommitTypeDefinition],
    ) -> Result<CommitTypeSelection, Self::Error> {
        if definitions.is_empty() {
            return Err(CommitTypeSelectorError::EmptyCatalog);
        }
        if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
            return Err(CommitTypeSelectorError::NotTerminal);
        }

        ratatui::run(|terminal| run_selector(terminal, definitions))
            .map_err(CommitTypeSelectorError::Terminal)
    }
}

/// Failure to present or operate the commit-type picker.
#[derive(Debug)]
pub enum CommitTypeSelectorError {
    /// Effective project policy contains no commit types.
    EmptyCatalog,
    /// Selection was requested without interactive terminal streams.
    NotTerminal,
    /// Terminal initialization, rendering, input, or restoration failed.
    Terminal(io::Error),
}

impl Display for CommitTypeSelectorError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyCatalog => {
                formatter.write_str("cannot select a commit type from an empty catalog")
            }
            Self::NotTerminal => formatter.write_str(
                "commit type selection requires a terminal; use `gitserious commit --type <COMMIT TYPE>`",
            ),
            Self::Terminal(_) => formatter.write_str("commit type selection failed"),
        }
    }
}

impl Error for CommitTypeSelectorError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Terminal(error) => Some(error),
            Self::EmptyCatalog | Self::NotTerminal => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct PickerState {
    selected: usize,
}

impl PickerState {
    fn previous(&mut self, length: usize) {
        self.selected = if self.selected == 0 {
            length.saturating_sub(1)
        } else {
            self.selected - 1
        };
    }

    fn next(&mut self, length: usize) {
        self.selected = if self.selected + 1 >= length {
            0
        } else {
            self.selected + 1
        };
    }

    const fn first(&mut self) {
        self.selected = 0;
    }

    fn last(&mut self, length: usize) {
        self.selected = length.saturating_sub(1);
    }
}

fn run_selector(
    terminal: &mut DefaultTerminal,
    definitions: &[CommitTypeDefinition],
) -> io::Result<CommitTypeSelection> {
    let mut state = PickerState::default();
    loop {
        terminal.draw(|frame| render(frame, definitions, state))?;
        if let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
            && let Some(selection) = handle_key(key, definitions, &mut state)
        {
            return Ok(selection);
        }
    }
}

fn handle_key(
    key: KeyEvent,
    definitions: &[CommitTypeDefinition],
    state: &mut PickerState,
) -> Option<CommitTypeSelection> {
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => state.previous(definitions.len()),
        KeyCode::Down | KeyCode::Char('j') => state.next(definitions.len()),
        KeyCode::Home => state.first(),
        KeyCode::End => state.last(definitions.len()),
        KeyCode::Enter => {
            return definitions
                .get(state.selected)
                .map(|definition| CommitTypeSelection::Selected(definition.id().clone()));
        }
        KeyCode::Esc | KeyCode::Char('q') => return Some(CommitTypeSelection::Cancelled),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            return Some(CommitTypeSelection::Cancelled);
        }
        _ => {}
    }
    None
}

fn render(frame: &mut Frame<'_>, definitions: &[CommitTypeDefinition], state: PickerState) {
    let area = frame.area();
    if area.width < 24 || area.height < 8 {
        frame.render_widget(
            Paragraph::new("Terminal too small\nResize or press Esc to cancel")
                .block(Block::default().borders(Borders::ALL).title(" gitserious "))
                .wrap(Wrap { trim: true }),
            area,
        );
        return;
    }

    let sections = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(4),
        Constraint::Length(5),
        Constraint::Length(1),
    ])
    .split(area);

    frame.render_widget(
        Paragraph::new("Choose the semantic contract for this commit.").block(
            Block::default()
                .borders(Borders::ALL)
                .title(" gitserious commit "),
        ),
        sections[0],
    );
    render_type_list(frame, sections[1], definitions, state);

    if let Some(selected) = definitions.get(state.selected) {
        frame.render_widget(
            Paragraph::new(selected.description())
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(format!(" {} ", selected.id())),
                )
                .wrap(Wrap { trim: true }),
            sections[2],
        );
    }

    frame.render_widget(
        Paragraph::new("↑/k ↓/j move  Home/End jump  Enter select  Esc/q cancel"),
        sections[3],
    );
}

fn render_type_list(
    frame: &mut Frame<'_>,
    area: Rect,
    definitions: &[CommitTypeDefinition],
    state: PickerState,
) {
    let items = definitions
        .iter()
        .map(|definition| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    definition.id().as_str(),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(" — "),
                Span::raw(definition.description()),
            ]))
        })
        .collect::<Vec<_>>();
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Commit types "),
        )
        .highlight_symbol("› ")
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    let mut list_state = ListState::default();
    list_state.select(Some(state.selected));
    frame.render_stateful_widget(list, area, &mut list_state);
}
