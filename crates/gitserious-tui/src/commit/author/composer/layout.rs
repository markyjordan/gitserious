//! Where a card's text breaks into rows, which drawing, ↑ and ↓, and the
//! column count all read.

use unicode_width::UnicodeWidthChar;

/// One row of a wrapped line: its chars from `start` up to `end`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Row {
    pub(super) line: usize,
    pub(super) start: usize,
    pub(super) end: usize,
}

/// Breaks every line into rows at most `width` columns wide. A row ends after
/// its last space, or inside a word too long for one row.
pub(super) fn rows(lines: &[String], width: usize) -> Vec<Row> {
    lines
        .iter()
        .enumerate()
        .flat_map(|(line, text)| {
            wrap(text, width)
                .into_iter()
                .map(move |(start, end)| Row { line, start, end })
        })
        .collect()
}

fn wrap(text: &str, width: usize) -> Vec<(usize, usize)> {
    let width = width.max(1);
    let widths: Vec<usize> = text.chars().map(char_width).collect();
    let mut rows = Vec::new();
    let mut start = 0;
    let mut used = 0;
    let mut after_space = None;
    for (index, character) in text.chars().enumerate() {
        while used + widths[index] > width && index > start {
            let end = after_space.filter(|end| *end > start).unwrap_or(index);
            rows.push((start, end));
            used = widths[end..index].iter().sum();
            start = end;
            after_space = None;
        }
        used += widths[index];
        if character.is_whitespace() {
            after_space = Some(index + 1);
        }
    }
    rows.push((start, widths.len()));
    rows
}

/// The row the cursor at char `column` of `line` sits on, and its column in that row.
pub(super) fn locate(
    lines: &[String],
    rows: &[Row],
    (line, column): (usize, usize),
) -> (usize, usize) {
    let index = rows
        .iter()
        .rposition(|row| row.line == line && row.start <= column)
        .unwrap_or(0);
    let x = lines[line]
        .chars()
        .skip(rows[index].start)
        .take(column.saturating_sub(rows[index].start))
        .map(char_width)
        .sum();
    (index, x)
}

/// Where ↑ or ↓ puts a cursor at `cursor`, keeping its column, or `None` from
/// the first or last row.
pub(super) fn vertical(
    lines: &[String],
    cursor: (usize, usize),
    width: usize,
    down: bool,
) -> Option<(usize, usize)> {
    let rows = rows(lines, width);
    let (index, x) = locate(lines, &rows, cursor);
    let target = if down {
        index + 1
    } else {
        index.checked_sub(1)?
    };
    let row = *rows.get(target)?;
    let last_in_line = rows
        .get(target + 1)
        .is_none_or(|next| next.line != row.line);
    // A wrapped row's end is the next row's start, so the cursor stops short of it.
    let limit = if last_in_line {
        row.end
    } else {
        row.end.saturating_sub(1).max(row.start)
    };
    let mut column = row.start;
    let mut used = 0;
    for character in lines[row.line]
        .chars()
        .skip(row.start)
        .take(limit - row.start)
    {
        used += char_width(character);
        if used > x {
            break;
        }
        column += 1;
    }
    Some((row.line, column))
}

/// The 1-based column of the cursor in its row when lines wrap at `width`.
pub(super) fn column(lines: &[String], cursor: (usize, usize), width: usize) -> usize {
    let rows = rows(&lines[cursor.0..=cursor.0], width);
    locate(&lines[cursor.0..=cursor.0], &rows, (0, cursor.1)).1 + 1
}

pub(super) fn char_width(character: char) -> usize {
    character.width().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(line: &str, width: usize) -> Vec<String> {
        wrap(line, width)
            .into_iter()
            .map(|(start, end)| line.chars().skip(start).take(end - start).collect())
            .collect()
    }

    #[test]
    fn rows_break_after_the_last_space_that_fits_or_inside_a_long_word() {
        assert_eq!(texts("a bcdefgh", 5), ["a ", "bcdef", "gh"]);
        assert_eq!(texts("one two three", 8), ["one two ", "three"]);
        assert_eq!(texts("", 8), [""]);
        assert_eq!(texts("🦀🦀🦀", 4), ["🦀🦀", "🦀"], "wide chars count twice");
    }

    #[test]
    fn up_and_down_keep_the_column_and_stop_at_the_edges() {
        let lines = vec!["one two three".to_owned(), "four".to_owned()];
        assert_eq!(vertical(&lines, (0, 2), 8, true), Some((0, 10)));
        assert_eq!(vertical(&lines, (0, 10), 8, true), Some((1, 2)));
        assert_eq!(vertical(&lines, (1, 4), 8, true), None);
        assert_eq!(vertical(&lines, (0, 10), 8, false), Some((0, 2)));
        assert_eq!(vertical(&lines, (0, 7), 8, false), None);
        assert_eq!(
            vertical(&lines, (1, 4), 8, false),
            Some((0, 12)),
            "the column clamps to a shorter row"
        );
        assert_eq!(
            vertical(&["ab cdefghij".to_owned()], (0, 7), 5, false),
            Some((0, 2)),
            "a wrapped row stops before the next row starts"
        );
    }

    #[test]
    fn the_column_counts_from_the_start_of_the_wrapped_row() {
        let lines = vec!["one two three".to_owned()];
        assert_eq!(column(&lines, (0, 0), 8), 1);
        assert_eq!(column(&lines, (0, 8), 8), 1);
        assert_eq!(column(&lines, (0, 13), 8), 6);
        assert_eq!(column(&lines, (0, 13), 80), 14);
    }
}
