//! The cite list opened by `@` while the cards are on screen.

use gitserious_app::CitedCommit;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use crate::theme::{
    block_title, centered_rect, popup_block, popup_selected_style, section_heading_style, theme,
};

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

pub(crate) fn render(
    frame: &mut Frame<'_>,
    area: Rect,
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
    let width = area.width.saturating_sub(4).clamp(28, 68).min(area.width);
    let height = area.height.saturating_sub(2).clamp(8, 18).min(area.height);
    let popup = centered_rect(width, height, area);
    frame.render_widget(Clear, popup);
    let block = popup_block().title(block_title("Cite"));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    frame.render_widget(
        Paragraph::new(kind_line(cite.kind)),
        Rect { height: 1, ..inner },
    );
    frame.render_widget(
        Paragraph::new(cite.query.as_str()),
        Rect {
            y: inner.y.saturating_add(1),
            height: 1,
            ..inner
        },
    );
    let list = Rect {
        y: inner.y.saturating_add(2),
        height: inner.height.saturating_sub(2),
        ..inner
    };
    if matches.is_empty() {
        frame.render_widget(
            Paragraph::new("no match").style(Style::new().fg(theme().dim)),
            list,
        );
        return;
    }
    let rows = usize::from(list.height);
    cite.offset = visible_offset(cite.selected, cite.offset, rows);
    let lines = matches
        .iter()
        .enumerate()
        .skip(cite.offset)
        .take(rows)
        .map(|(index, row)| {
            let style = if index == cite.selected {
                popup_selected_style()
            } else {
                Style::new().fg(theme().text)
            };
            Line::styled(row.label.clone(), style)
        })
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines), list);
}

fn matching(cite: &Cite, paths: &[String], commits: &[CitedCommit]) -> Vec<CiteRow> {
    let query = cite.query.as_str();
    match cite.kind {
        CiteKind::Files => paths
            .iter()
            .filter(|path| path.contains(query))
            .map(|path| CiteRow {
                label: path.clone(),
                insert: format!("@{path}"),
            })
            .collect(),
        CiteKind::Commits => commits
            .iter()
            .filter(|commit| commit.sha.contains(query) || commit.subject.contains(query))
            .map(|commit| CiteRow {
                label: if commit.subject.is_empty() {
                    commit.sha.clone()
                } else {
                    format!("{}  {}", commit.sha, commit.subject)
                },
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
                section_heading_style()
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
