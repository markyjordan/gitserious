use gitserious_core::{COMMIT_MESSAGE_WIDTH, ValidationSeverity};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use tui_textarea::TextArea;
use unicode_width::UnicodeWidthStr;

use super::layout::{self, char_width};
use super::{CardId, Cards, Mark};
use crate::theme::theme;

/// Columns left of a card's text: the focus bar, the mark, and a space.
const GUTTER: u16 = 3;
const MARGIN: u16 = 1;
/// The subject length `git log --oneline` and most forges show uncut.
const SUBJECT_LIMIT: usize = 72;

/// One row of the card list.
#[derive(Default)]
struct Row {
    mark: Option<Span<'static>>,
    text: Line<'static>,
    right: Option<Span<'static>>,
    focused: bool,
}

/// Every card's rows, with the focused card's first row and the cursor's row.
#[derive(Default)]
struct Sheet {
    rows: Vec<Row>,
    focus_top: usize,
    cursor: usize,
}

/// Draws the cards in `area`, scrolled so the cursor shows, with the status
/// line under them.
pub(crate) fn render(frame: &mut Frame<'_>, area: Rect, cards: &mut Cards) {
    let [list, _, status] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area);
    let width = usize::from(list.width.saturating_sub(GUTTER + MARGIN));
    // One column stays free past the wrap, so a cursor after a full row shows.
    cards.wrap = width
        .saturating_sub(1)
        .clamp(1, usize::from(COMMIT_MESSAGE_WIDTH));
    let sheet = sheet(cards, width);
    cards.scroll = scroll(cards.scroll, &sheet, usize::from(list.height));
    for (y, row) in (list.y..list.bottom()).zip(sheet.rows.iter().skip(cards.scroll)) {
        draw_row(frame, Rect::new(list.x, y, list.width, 1), row);
    }
    render_status(frame, status, cards);
}

fn sheet(cards: &Cards, width: usize) -> Sheet {
    let mut sheet = Sheet::default();
    for id in cards.ids() {
        if !sheet.rows.is_empty() {
            sheet.rows.push(Row::default());
        }
        let focused = id == cards.focus;
        if focused {
            sheet.focus_top = sheet.rows.len();
            sheet.cursor = sheet.rows.len();
        }
        let failed = cards
            .notes
            .iter()
            .any(|note| note.card == id && note.severity == ValidationSeverity::Error);
        let mark = Some(mark_span(cards.mark(id), failed));
        let rows = match cards.card(id) {
            None => vec![Row {
                mark,
                focused,
                ..subject_row(cards, width)
            }],
            Some(card) => {
                let header = Row {
                    mark,
                    text: Line::styled(format!("{}:", cards.name(id)), bold()),
                    right: Some(Span::styled(card.policy.word(), dim())),
                    focused,
                };
                let body = if !focused {
                    text_lines(&card.body, cards.wrap, None)
                } else if card.body.is_empty() {
                    sheet.cursor += 1;
                    guidance_lines(cards.guidance(id), cards.wrap)
                } else {
                    let (lines, cursor) = cursor_lines(&card.body, cards.wrap);
                    sheet.cursor += 1 + cursor;
                    lines
                };
                std::iter::once(header)
                    .chain(body.into_iter().map(|text| Row {
                        text,
                        focused,
                        ..Row::default()
                    }))
                    .collect()
            }
        };
        sheet.rows.extend(rows);
        for note in cards.notes.iter().filter(|note| note.card == id) {
            let style = match note.severity {
                ValidationSeverity::Error => Style::new().fg(theme().blocks),
                ValidationSeverity::Warning => dim(),
            };
            let lines = vec![note.message.clone()];
            sheet
                .rows
                .extend(layout::rows(&lines, cards.wrap).into_iter().map(|row| Row {
                    text: Line::styled(slice(&lines[0], row.start, row.end), style),
                    focused,
                    ..Row::default()
                }));
        }
    }
    sheet
}

/// The type key and what follows it, scrolled sideways so the cursor shows,
/// with the subject's length on the right.
fn subject_row(cards: &Cards, width: usize) -> Row {
    let key = cards.definition().id().as_str();
    let tail = &cards.subject.lines()[0];
    let length = UnicodeWidthStr::width(key) + UnicodeWidthStr::width(tail.as_str());
    let counter = format!("{length}/{SUBJECT_LIMIT}");
    let room = width.saturating_sub(counter.width() + 1);
    let mut cells: Vec<_> = key
        .chars()
        .chain(tail.chars())
        .map(|c| (c, bold()))
        .collect();
    if cards.focus == CardId::Subject {
        let cursor = key.chars().count() + cards.subject.cursor().1;
        if tail.is_empty() {
            cells.extend(cards.guidance(CardId::Subject).chars().map(|c| (c, dim())));
        }
        place_cursor(&mut cells, cursor);
        let before: usize = cells[..cursor].iter().map(|(c, _)| char_width(*c)).sum();
        let mut hidden = (before + 1).saturating_sub(room);
        while hidden > 0 && !cells.is_empty() {
            hidden = hidden.saturating_sub(char_width(cells.remove(0).0));
        }
    } else if tail.is_empty() {
        cells.extend(cards.guidance(CardId::Subject).chars().map(|c| (c, dim())));
    }
    let over = if length > SUBJECT_LIMIT {
        Style::new().fg(theme().pending)
    } else {
        dim()
    };
    Row {
        text: line(cells),
        right: Some(Span::styled(counter, over)),
        ..Row::default()
    }
}

/// The text wrapped at `wrap`, with `cursor` drawn where it sits.
fn text_lines(
    text: &TextArea<'_>,
    wrap: usize,
    cursor: Option<(usize, usize)>,
) -> Vec<Line<'static>> {
    if text.is_empty() && cursor.is_none() {
        return Vec::new();
    }
    let lines = text.lines();
    let selection = text.selection_range();
    let rows = layout::rows(lines, wrap);
    let at = cursor.map(|cursor| layout::locate(lines, &rows, cursor).0);
    rows.iter()
        .enumerate()
        .map(|(index, row)| {
            let mut cells: Vec<_> = lines[row.line]
                .chars()
                .enumerate()
                .skip(row.start)
                .take(row.end - row.start)
                .map(|(column, c)| {
                    let selected = selection.is_some_and(|(from, to)| {
                        from <= (row.line, column) && (row.line, column) < to
                    });
                    let style = Style::new().fg(theme().text);
                    (
                        c,
                        if selected {
                            style.add_modifier(Modifier::REVERSED)
                        } else {
                            style
                        },
                    )
                })
                .collect();
            if let (Some(cursor), Some(at)) = (cursor, at)
                && at == index
            {
                place_cursor(&mut cells, cursor.1 - row.start);
            }
            line(cells)
        })
        .collect()
}

/// The focused card's text, and the row of its cursor.
fn cursor_lines(text: &TextArea<'_>, wrap: usize) -> (Vec<Line<'static>>, usize) {
    let rows = layout::rows(text.lines(), wrap);
    let row = layout::locate(text.lines(), &rows, text.cursor()).0;
    (text_lines(text, wrap, Some(text.cursor())), row)
}

/// What an empty focused card is for, dimmed, under the cursor.
fn guidance_lines(guidance: &str, wrap: usize) -> Vec<Line<'static>> {
    let lines = vec![guidance.to_owned()];
    let mut rows: Vec<Vec<_>> = layout::rows(&lines, wrap)
        .into_iter()
        .map(|row| {
            slice(guidance, row.start, row.end)
                .chars()
                .map(|c| (c, dim()))
                .collect()
        })
        .collect();
    place_cursor(&mut rows[0], 0);
    rows.into_iter().map(line).collect()
}

/// Reverses the cell at `index`, or adds a reversed space past the last cell.
fn place_cursor(cells: &mut Vec<(char, Style)>, index: usize) {
    match cells.get_mut(index) {
        Some((_, style)) => *style = style.add_modifier(Modifier::REVERSED),
        None => cells.push((
            ' ',
            Style::new()
                .fg(theme().text)
                .add_modifier(Modifier::REVERSED),
        )),
    }
}

/// Joins runs of equally styled chars into spans.
fn line(cells: Vec<(char, Style)>) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    for (character, style) in cells {
        match spans.last_mut() {
            Some(span) if span.style == style => span.content.to_mut().push(character),
            _ => spans.push(Span::styled(character.to_string(), style)),
        }
    }
    Line::from(spans)
}

fn slice(text: &str, start: usize, end: usize) -> String {
    text.chars().skip(start).take(end - start).collect()
}

fn mark_span(mark: Mark, failed: bool) -> Span<'static> {
    let (symbol, color) = match mark {
        Mark::Done => ("✓", theme().done),
        Mark::Focused => ("●", theme().pending),
        Mark::Undecided => ("◐", theme().text),
        Mark::Open => ("○", theme().dim),
    };
    Span::styled(
        symbol,
        Style::new().fg(if failed { theme().blocks } else { color }),
    )
}

/// Keeps the top row unless the cursor would leave the list, then shows the
/// focused card from its first row when it fits.
fn scroll(previous: usize, sheet: &Sheet, height: usize) -> usize {
    let height = height.max(1);
    let top = previous.min(sheet.rows.len().saturating_sub(height));
    if sheet.cursor < top {
        if sheet.cursor - sheet.focus_top < height {
            sheet.focus_top
        } else {
            sheet.cursor
        }
    } else if sheet.cursor >= top + height {
        sheet.cursor + 1 - height
    } else {
        top
    }
}

fn draw_row(frame: &mut Frame<'_>, area: Rect, row: &Row) {
    if row.focused {
        frame.render_widget(
            Paragraph::new("▎").style(Style::new().fg(theme().pending)),
            area,
        );
    }
    if let Some(mark) = &row.mark {
        frame.render_widget(
            Paragraph::new(mark.clone()),
            Rect {
                x: area.x + 1,
                width: area.width.saturating_sub(1),
                ..area
            },
        );
    }
    let text = Rect {
        x: area.x + GUTTER,
        width: area.width.saturating_sub(GUTTER + MARGIN),
        ..area
    };
    let left = if let Some(right) = &row.right {
        let reserved = u16::try_from(right.width()).unwrap_or(u16::MAX);
        let [left, _, right_area] = Layout::horizontal([
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(reserved),
        ])
        .areas(text);
        frame.render_widget(Paragraph::new(right.clone()).right_aligned(), right_area);
        left
    } else {
        text
    };
    frame.render_widget(Paragraph::new(row.text.clone()), left);
}

/// What review waits on, or that it waits on nothing, and the cursor's column
/// in the message.
fn render_status(frame: &mut Frame<'_>, area: Rect, cards: &Cards) {
    let area = Rect {
        x: area.x + 1,
        width: area.width.saturating_sub(2),
        ..area
    };
    let column = format!("col {}/{COMMIT_MESSAGE_WIDTH}", cursor_column(cards));
    let [left, right] = Layout::horizontal([
        Constraint::Min(0),
        Constraint::Length(u16::try_from(column.width()).unwrap_or(u16::MAX)),
    ])
    .areas(area);
    let to_go = cards.to_go();
    let summary = if to_go.is_empty() {
        Span::styled("ready for review", Style::new().fg(theme().done))
    } else {
        let names: Vec<_> = to_go.iter().map(|id| cards.name(*id)).collect();
        Span::styled(
            format!("{} to go: {}", to_go.len(), names.join(", ")),
            dim(),
        )
    };
    frame.render_widget(Paragraph::new(summary), left);
    frame.render_widget(Paragraph::new(column).style(dim()).right_aligned(), right);
}

/// Where the cursor falls in the message's 80-column lines, counting from 1.
fn cursor_column(cards: &Cards) -> usize {
    let text = cards.text(cards.focus);
    match cards.focus {
        CardId::Subject => {
            let key = cards.definition().id().as_str();
            let before: usize = text.lines()[0]
                .chars()
                .take(text.cursor().1)
                .map(char_width)
                .sum();
            UnicodeWidthStr::width(key) + before + 1
        }
        CardId::Property(_) | CardId::BreakingChange => layout::column(
            text.lines(),
            text.cursor(),
            usize::from(COMMIT_MESSAGE_WIDTH),
        ),
    }
}

fn bold() -> Style {
    Style::new().fg(theme().text).add_modifier(Modifier::BOLD)
}

fn dim() -> Style {
    Style::new().fg(theme().dim)
}
