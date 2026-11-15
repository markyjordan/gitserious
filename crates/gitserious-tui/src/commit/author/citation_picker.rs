//! The citation picker opened by `@` while the composer is on screen.

use gitserious_app::CitedCommit;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::theme::{popup_selected_style, theme};

/// Columns between the panel's edge and its text.
const PADDING: u16 = 2;
/// The panel's rows besides the list: padding above and below, the source row,
/// the query row, and a blank row on each side of the list.
const FIXED_ROWS: u16 = 6;
/// The most rows the list shows before it scrolls.
const MAX_ROWS: usize = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CitationSource {
    Files,
    Commits,
}

pub(crate) struct CitationPickerState {
    query: String,
    source: CitationSource,
    selected: usize,
    offset: usize,
}

struct CitationOption {
    /// A column the rows line up after, such as a commit's sha.
    lead: String,
    label: String,
    insert: String,
}

pub(crate) enum CitationPickerAction {
    Stay,
    Close,
    Insert(String),
}

impl CitationPickerState {
    pub(crate) fn new() -> Self {
        Self {
            query: String::new(),
            source: CitationSource::Files,
            selected: 0,
            offset: 0,
        }
    }

    /// Adds `text` to the filter. A paste with a line break is left unused.
    pub(crate) fn push_text(&mut self, text: &str) {
        if text.contains(['\n', '\r']) {
            return;
        }
        self.query.push_str(text);
        self.rewind();
    }

    pub(crate) fn key(
        &mut self,
        key: KeyEvent,
        paths: &[String],
        commits: &[CitedCommit],
    ) -> CitationPickerAction {
        let matches = matching(self, paths, commits);
        if !matches.is_empty() {
            self.selected %= matches.len();
        }
        match key.code {
            KeyCode::Left | KeyCode::Right if key.modifiers.is_empty() => {
                self.source = match self.source {
                    CitationSource::Files => CitationSource::Commits,
                    CitationSource::Commits => CitationSource::Files,
                };
                self.rewind();
                CitationPickerAction::Stay
            }
            KeyCode::Up | KeyCode::Down if !matches.is_empty() && key.modifiers.is_empty() => {
                let step = if key.code == KeyCode::Up {
                    matches.len() - 1
                } else {
                    1
                };
                self.selected = (self.selected + step) % matches.len();
                CitationPickerAction::Stay
            }
            KeyCode::Enter => matches
                .get(self.selected)
                .map_or(CitationPickerAction::Stay, |row| {
                    CitationPickerAction::Insert(row.insert.clone())
                }),
            KeyCode::Esc => CitationPickerAction::Close,
            KeyCode::Backspace => {
                self.query.pop();
                self.rewind();
                CitationPickerAction::Stay
            }
            KeyCode::Char(character)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.query.push(character);
                self.rewind();
                CitationPickerAction::Stay
            }
            _ => CitationPickerAction::Stay,
        }
    }

    fn rewind(&mut self) {
        self.selected = 0;
        self.offset = 0;
    }
}

/// Draws the list as a panel under the cursor's row, or above it when the
/// rows below are too few. The panel keeps its height while the query narrows
/// the list, so it does not jump as the author types.
pub(crate) fn render(
    frame: &mut Frame<'_>,
    area: Rect,
    cursor_row: Option<u16>,
    picker: &mut Option<CitationPickerState>,
    paths: &[String],
    commits: &[CitedCommit],
) {
    let Some(picker) = picker else {
        return;
    };
    let matches = matching(picker, paths, commits);
    if matches.is_empty() {
        picker.selected = 0;
        picker.offset = 0;
    } else if picker.selected >= matches.len() {
        picker.selected = 0;
    }
    let total = match picker.source {
        CitationSource::Files => paths.len(),
        CitationSource::Commits => commits.len(),
    };
    let wanted = u16::try_from(total.clamp(1, MAX_ROWS))
        .unwrap_or(1)
        .saturating_add(FIXED_ROWS);
    let panel = place(area, cursor_row, wanted);
    if panel.width <= PADDING * 2 || panel.height == 0 {
        return;
    }
    frame.render_widget(Clear, panel);
    frame.render_widget(
        Block::default().style(Style::new().fg(theme().text).bg(theme().band)),
        panel,
    );
    let [_, sources, _, list, _, query] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(Rect {
        height: panel.height.saturating_sub(1),
        ..inset(panel)
    });
    frame.render_widget(Paragraph::new(source_line(picker.source)), sources);
    frame.render_widget(Paragraph::new(query_line(picker, query.width)), query);
    if matches.is_empty() {
        frame.render_widget(
            Paragraph::new("no match").style(Style::new().fg(theme().dim)),
            list,
        );
        return;
    }
    let rows = usize::from(list.height);
    picker.offset = visible_offset(picker.selected, picker.offset, rows);
    let lead = matches
        .iter()
        .map(|row| row.lead.width())
        .max()
        .unwrap_or(0);
    for (y, (index, row)) in
        (list.y..list.bottom()).zip(matches.iter().enumerate().skip(picker.offset))
    {
        let style = if index == picker.selected {
            frame.render_widget(
                Block::default().style(popup_selected_style()),
                Rect::new(panel.x, y, panel.width, 1),
            );
            popup_selected_style()
        } else {
            Style::new().fg(theme().text)
        };
        let label = if lead == 0 {
            row.label.clone()
        } else {
            format!("{:<lead$}  {}", row.lead, row.label)
        };
        frame.render_widget(
            Paragraph::new(label).style(style),
            Rect::new(list.x, y, list.width, 1),
        );
    }
}

/// The panel's rows under `cursor_row` with a blank row between, else above
/// it, else whichever side has more room, shortened to fit.
fn place(area: Rect, cursor_row: Option<u16>, wanted: u16) -> Rect {
    let panel =
        |y: u16, height: u16| Rect::new(area.x + 1, y, area.width.saturating_sub(2), height);
    let Some(cursor) = cursor_row else {
        let height = wanted.min(area.height);
        return panel(area.bottom() - height, height);
    };
    let below = area.bottom().saturating_sub(cursor.saturating_add(2));
    let above = cursor.saturating_sub(area.y.saturating_add(1));
    if wanted <= below || below >= above {
        panel(
            cursor.saturating_add(2).min(area.bottom()),
            wanted.min(below),
        )
    } else {
        let height = wanted.min(above);
        panel(cursor - 1 - height, height)
    }
}

fn inset(panel: Rect) -> Rect {
    Rect::new(
        panel.x + PADDING,
        panel.y,
        panel.width.saturating_sub(PADDING * 2),
        panel.height,
    )
}

fn matching(
    picker: &CitationPickerState,
    paths: &[String],
    commits: &[CitedCommit],
) -> Vec<CitationOption> {
    let query = picker.query.as_str();
    match picker.source {
        CitationSource::Files => paths
            .iter()
            .filter(|path| path.contains(query))
            .map(|path| CitationOption {
                lead: String::new(),
                label: path.clone(),
                insert: format!("@{path}"),
            })
            .collect(),
        CitationSource::Commits => commits
            .iter()
            .filter(|commit| commit.sha.contains(query) || commit.subject.contains(query))
            .map(|commit| CitationOption {
                lead: commit.sha.clone(),
                label: commit.subject.clone(),
                insert: format!("@{}", commit.sha),
            })
            .collect(),
    }
}

fn source_line(source: CitationSource) -> Line<'static> {
    let paint = |name: &'static str, active: bool| {
        Span::styled(
            name,
            if active {
                Style::new()
                    .fg(theme().text)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else {
                Style::new().fg(theme().dim)
            },
        )
    };
    Line::from(vec![
        paint("Files", source == CitationSource::Files),
        Span::raw("  "),
        paint("Commits", source == CitationSource::Commits),
    ])
}

/// What was typed, cut from the left to fit, or a dim prompt until something is.
fn query_line(picker: &CitationPickerState, width: u16) -> Line<'static> {
    if picker.query.is_empty() {
        let source = match picker.source {
            CitationSource::Files => "files",
            CitationSource::Commits => "commits",
        };
        return Line::styled(
            format!("Type to filter {source}"),
            Style::new().fg(theme().dim),
        );
    }
    let mut hidden = picker.query.width().saturating_sub(usize::from(width));
    let shown = picker.query.trim_start_matches(|character: char| {
        let skip = hidden > 0;
        hidden = hidden.saturating_sub(character.width().unwrap_or(0));
        skip
    });
    Line::styled(
        shown.to_owned(),
        Style::new().fg(theme().text).add_modifier(Modifier::BOLD),
    )
}

fn visible_offset(selected: usize, offset: usize, rows: usize) -> usize {
    if rows == 0 {
        return 0;
    }
    if selected < offset {
        selected
    } else if selected >= offset.saturating_add(rows) {
        selected + 1 - rows
    } else {
        offset
    }
}
