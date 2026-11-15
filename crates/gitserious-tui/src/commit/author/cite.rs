//! The cite list opened by `@` while the cards are on screen.

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
/// The panel's rows besides the list: padding above and below, the kind row,
/// the query row, and a blank row on each side of the list.
const FIXED_ROWS: u16 = 6;
/// The most rows the list shows before it scrolls.
const MAX_ROWS: usize = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CiteKind {
    Files,
    Commits,
}

pub(crate) struct Cite {
    query: String,
    kind: CiteKind,
    selected: usize,
    offset: usize,
}

struct CiteRow {
    /// A column the rows line up after, such as a commit's sha.
    lead: String,
    label: String,
    insert: String,
}

pub(crate) enum CiteAction {
    Stay,
    Close,
    Insert(String),
}

impl Cite {
    pub(crate) fn new() -> Self {
        Self {
            query: String::new(),
            kind: CiteKind::Files,
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
    ) -> CiteAction {
        let matches = matching(self, paths, commits);
        if !matches.is_empty() {
            self.selected %= matches.len();
        }
        match key.code {
            KeyCode::Left | KeyCode::Right if key.modifiers.is_empty() => {
                self.kind = match self.kind {
                    CiteKind::Files => CiteKind::Commits,
                    CiteKind::Commits => CiteKind::Files,
                };
                self.rewind();
                CiteAction::Stay
            }
            KeyCode::Up | KeyCode::Down if !matches.is_empty() && key.modifiers.is_empty() => {
                let step = if key.code == KeyCode::Up {
                    matches.len() - 1
                } else {
                    1
                };
                self.selected = (self.selected + step) % matches.len();
                CiteAction::Stay
            }
            KeyCode::Enter => matches.get(self.selected).map_or(CiteAction::Stay, |row| {
                CiteAction::Insert(row.insert.clone())
            }),
            KeyCode::Esc => CiteAction::Close,
            KeyCode::Backspace => {
                self.query.pop();
                self.rewind();
                CiteAction::Stay
            }
            KeyCode::Char(character)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.query.push(character);
                self.rewind();
                CiteAction::Stay
            }
            _ => CiteAction::Stay,
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
    cite: &mut Option<Cite>,
    paths: &[String],
    commits: &[CitedCommit],
) {
    let Some(cite) = cite else {
        return;
    };
    let matches = matching(cite, paths, commits);
    if matches.is_empty() {
        cite.selected = 0;
        cite.offset = 0;
    } else if cite.selected >= matches.len() {
        cite.selected = 0;
    }
    let total = match cite.kind {
        CiteKind::Files => paths.len(),
        CiteKind::Commits => commits.len(),
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
    let [_, kinds, _, list, _, query] = Layout::vertical([
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
    frame.render_widget(Paragraph::new(kind_line(cite.kind)), kinds);
    frame.render_widget(Paragraph::new(query_line(cite, query.width)), query);
    if matches.is_empty() {
        frame.render_widget(
            Paragraph::new("no match").style(Style::new().fg(theme().dim)),
            list,
        );
        return;
    }
    let rows = usize::from(list.height);
    cite.offset = visible_offset(cite.selected, cite.offset, rows);
    let lead = matches
        .iter()
        .map(|row| row.lead.width())
        .max()
        .unwrap_or(0);
    for (y, (index, row)) in
        (list.y..list.bottom()).zip(matches.iter().enumerate().skip(cite.offset))
    {
        let style = if index == cite.selected {
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

fn matching(cite: &Cite, paths: &[String], commits: &[CitedCommit]) -> Vec<CiteRow> {
    let query = cite.query.as_str();
    match cite.kind {
        CiteKind::Files => paths
            .iter()
            .filter(|path| path.contains(query))
            .map(|path| CiteRow {
                lead: String::new(),
                label: path.clone(),
                insert: format!("@{path}"),
            })
            .collect(),
        CiteKind::Commits => commits
            .iter()
            .filter(|commit| commit.sha.contains(query) || commit.subject.contains(query))
            .map(|commit| CiteRow {
                lead: commit.sha.clone(),
                label: commit.subject.clone(),
                insert: format!("@{}", commit.sha),
            })
            .collect(),
    }
}

fn kind_line(kind: CiteKind) -> Line<'static> {
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
        paint("Files", kind == CiteKind::Files),
        Span::raw("  "),
        paint("Commits", kind == CiteKind::Commits),
    ])
}

/// What was typed, cut from the left to fit, or a dim prompt until something is.
fn query_line(cite: &Cite, width: u16) -> Line<'static> {
    if cite.query.is_empty() {
        let kind = match cite.kind {
            CiteKind::Files => "files",
            CiteKind::Commits => "commits",
        };
        return Line::styled(
            format!("Type to filter {kind}"),
            Style::new().fg(theme().dim),
        );
    }
    let mut hidden = cite.query.width().saturating_sub(usize::from(width));
    let shown = cite.query.trim_start_matches(|character: char| {
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
