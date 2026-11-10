use ratatui::Frame;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Padding, Paragraph};

use crate::theme::theme;

mod page;

pub use page::HelpPage;
#[cfg(test)]
pub(crate) use page::{HelpLine, HelpSpan};

/// The labels of the digit keys that jump to a page, by page count.
const JUMP: [&str; 9] = ["1", "1-2", "1-3", "1-4", "1-5", "1-6", "1-7", "1-8", "1-9"];

/// Where Help's keys go.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum HelpFocus {
    /// The sidebar: arrows and digits choose the page.
    #[default]
    Pages,
    /// The page: arrows and paging scroll it.
    Reading,
}

/// The Help section's chosen page, reading position, and focus.
#[derive(Debug, Default)]
pub(crate) struct HelpView {
    page: usize,
    scroll: u16,
    focus: HelpFocus,
    /// Sidebar rows from the last render, for mouse hits.
    rows: Vec<Rect>,
    /// Content rows visible in the last render, which one page key moves.
    height: u16,
}

impl HelpView {
    /// Help on `page`, in the sidebar.
    pub(crate) fn at(page: usize) -> Self {
        Self {
            page,
            ..Self::default()
        }
    }

    /// Arriving at Help always starts in the sidebar, on the page left last.
    pub(crate) fn enter(&mut self) {
        self.focus = HelpFocus::Pages;
    }

    fn select(&mut self, page: usize) {
        if page != self.page {
            self.page = page;
            self.scroll = 0;
        }
    }

    pub(crate) fn key(&mut self, code: KeyCode, count: usize) {
        let Some(last) = count.checked_sub(1) else {
            return;
        };
        let screen = self.height.max(1);
        match (self.focus, code) {
            (HelpFocus::Pages, KeyCode::Up) => self.select(self.page.saturating_sub(1)),
            (HelpFocus::Pages, KeyCode::Down) => self.select((self.page + 1).min(last)),
            (HelpFocus::Pages, KeyCode::Char(digit)) => {
                if let Some(page) = digit
                    .to_digit(10)
                    .and_then(|digit| usize::try_from(digit).ok())
                    .filter(|page| (1..=count).contains(page))
                {
                    self.select(page - 1);
                }
            }
            (HelpFocus::Pages, KeyCode::Right | KeyCode::Enter) => {
                self.focus = HelpFocus::Reading;
            }
            (HelpFocus::Reading, KeyCode::Left) => self.focus = HelpFocus::Pages,
            (HelpFocus::Reading, KeyCode::Up) => self.scroll = self.scroll.saturating_sub(1),
            (HelpFocus::Reading, KeyCode::Down) => self.scroll = self.scroll.saturating_add(1),
            (HelpFocus::Reading, KeyCode::PageUp) => {
                self.scroll = self.scroll.saturating_sub(screen);
            }
            (HelpFocus::Reading, KeyCode::PageDown) => {
                self.scroll = self.scroll.saturating_add(screen);
            }
            _ => {}
        }
    }

    pub(crate) fn click(&mut self, column: u16, row: u16) {
        if let Some(page) = self
            .rows
            .iter()
            .position(|rect| rect.contains((column, row).into()))
        {
            self.select(page);
            self.focus = HelpFocus::Pages;
        }
    }

    pub(crate) fn hints(&self, count: usize) -> Vec<(&'static str, &'static str)> {
        match self.focus {
            HelpFocus::Pages => [("→", "read"), ("↑↓", "pages")]
                .into_iter()
                .chain(
                    count
                        .min(JUMP.len())
                        .checked_sub(1)
                        .map(|last| (JUMP[last], "jump")),
                )
                .collect(),
            HelpFocus::Reading => vec![("↑↓", "scroll"), ("pgup/pgdn", "screen"), ("←", "pages")],
        }
    }

    pub(crate) fn render(&mut self, frame: &mut Frame<'_>, area: Rect, pages: &[HelpPage]) {
        let text = Style::new().fg(theme().text);
        let dim = Style::new().fg(theme().dim);
        let digits = pages.len().to_string().len();
        let labels: Vec<String> = pages
            .iter()
            .enumerate()
            .map(|(index, page)| format!("{:>digits$}  {}", index + 1, page.title))
            .collect();
        let widest = labels.iter().map(|label| Span::raw(label).width()).max();
        let sidebar_width = u16::try_from(widest.unwrap_or(0) + 2).unwrap_or(u16::MAX);
        let [sidebar, rule, content] = Layout::horizontal([
            Constraint::Length(sidebar_width),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .areas(area);

        self.rows.clear();
        for (index, (label, y)) in labels
            .into_iter()
            .zip(sidebar.y + 1..sidebar.bottom())
            .enumerate()
        {
            let row = Rect::new(sidebar.x, y, sidebar.width, 1);
            let selected = index == self.page;
            let line = match (selected, self.focus) {
                (true, HelpFocus::Pages) => Paragraph::new(format!(" {label}")).style(
                    Style::new()
                        .fg(theme().ground)
                        .bg(theme().cursor)
                        .add_modifier(Modifier::BOLD),
                ),
                (true, HelpFocus::Reading) => Paragraph::new(Line::from(vec![
                    Span::raw(" "),
                    Span::styled(
                        label,
                        text.add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                    ),
                ])),
                (false, _) => Paragraph::new(format!(" {label}")).style(dim),
            };
            frame.render_widget(line, row);
            self.rows.push(row);
        }
        frame.render_widget(Block::new().borders(Borders::LEFT).border_style(dim), rule);

        let area = Block::new()
            .padding(Padding::new(1, 1, 1, 0))
            .inner(content);
        self.height = area.height;
        let Some(page) = pages.get(self.page) else {
            return;
        };
        let strong = text.add_modifier(Modifier::BOLD);
        let mut lines: Vec<Line<'_>> = page
            .command
            .iter()
            .flat_map(|command| {
                [
                    Line::from(vec![
                        Span::styled("gitserious help: ", dim),
                        Span::styled(command.as_str(), text),
                    ]),
                    Line::default(),
                ]
            })
            .collect();
        lines.extend(page.lines(area.width).into_iter().map(|line| {
            // Prose at column 0 reads in the text color; indented descriptions recede.
            let plain = if line.first().is_some_and(|span| span.text.starts_with(' ')) {
                dim
            } else {
                text
            };
            Line::from(
                line.into_iter()
                    .map(|span| Span::styled(span.text, if span.strong { strong } else { plain }))
                    .collect::<Vec<_>>(),
            )
        }));
        let maximum = lines.len().saturating_sub(usize::from(area.height));
        self.scroll = self.scroll.min(u16::try_from(maximum).unwrap_or(u16::MAX));
        frame.render_widget(Paragraph::new(lines).scroll((self.scroll, 0)), area);
    }
}
