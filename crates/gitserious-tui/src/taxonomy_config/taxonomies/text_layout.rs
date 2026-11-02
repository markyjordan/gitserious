use ratatui::text::Span;
use unicode_segmentation::UnicodeSegmentation;

pub(super) fn symbols(text: &str) -> Vec<&str> {
    text.graphemes(true)
        .map(|symbol| {
            if symbol.contains(['\n', '\r']) {
                "\n"
            } else if symbol == "\t" {
                "    "
            } else if symbol.contains(char::is_control) {
                "�"
            } else {
                symbol
            }
        })
        .collect()
}

pub(super) fn wrap(graphemes: &[&str], width: usize) -> Vec<(usize, usize)> {
    let width = width.max(1);
    let mut rows = Vec::new();
    let (mut start, mut used, mut index) = (0, 0, 0);
    while index < graphemes.len() {
        if graphemes[index] == "\n" {
            rows.push((start, index));
            index += 1;
            start = index;
            used = 0;
            continue;
        }
        let size = Span::raw(graphemes[index]).width();
        if used + size > width && index > start {
            let space = if graphemes[index] == " " {
                Some(index)
            } else {
                (start + 1..index).rev().find(|&at| graphemes[at] == " ")
            };
            let (end, next) = space.map_or((index, index), |at| (at, at + 1));
            rows.push((start, end));
            start = next;
            index = index.max(next);
            used = graphemes[start..index]
                .iter()
                .map(|symbol| Span::raw(*symbol).width())
                .sum();
            continue;
        }
        used += size;
        index += 1;
    }
    rows.push((start, graphemes.len()));
    rows
}

pub(super) struct TextLayout<'a> {
    symbols: Vec<&'a str>,
    rows: Vec<(usize, usize)>,
}

impl<'a> TextLayout<'a> {
    pub(super) fn new(text: &'a str, width: usize) -> Self {
        let symbols = symbols(text);
        let rows = wrap(&symbols, width);
        Self { symbols, rows }
    }

    pub(super) fn row(&self, cursor: usize, trailing: bool) -> usize {
        cursor_row(&self.rows, cursor, trailing)
    }

    pub(super) fn edge(&self, cursor: usize, end: bool, trailing: bool) -> (usize, bool) {
        let row = self.row(cursor, trailing);
        let (start, finish) = self.rows[row];
        let at = if end { finish } else { start };
        (
            at,
            end && self.rows.get(row + 1).is_some_and(|(next, _)| *next == at),
        )
    }

    pub(super) fn vertical(
        &self,
        cursor: usize,
        down: bool,
        preferred: &mut Option<usize>,
        trailing: &mut bool,
    ) -> usize {
        let row = self.row(cursor, *trailing);
        let (start, end) = self.rows[row];
        let column = self.symbols[start..cursor.min(end)]
            .iter()
            .map(|symbol| Span::raw(*symbol).width())
            .sum();
        let wanted = *preferred.get_or_insert(column);
        let next = if down {
            (row + 1).min(self.rows.len() - 1)
        } else {
            row.saturating_sub(1)
        };
        let (start, end) = self.rows[next];
        let mut used = 0;
        for (at, symbol) in self.symbols.iter().enumerate().take(end).skip(start) {
            used += Span::raw(*symbol).width();
            if used > wanted {
                *trailing = false;
                return at;
            }
        }
        *trailing = self
            .rows
            .get(next + 1)
            .is_some_and(|(start, _)| *start == end);
        end
    }
}

pub(super) fn cursor_row(rows: &[(usize, usize)], cursor: usize, trailing: bool) -> usize {
    rows.iter()
        .enumerate()
        .find(|(index, (start, end))| {
            (trailing && cursor == *end)
                || (*start..*end).contains(&cursor)
                || (cursor == *end && rows.get(index + 1).is_none_or(|(next, _)| *next > cursor))
        })
        .map_or(rows.len() - 1, |(index, _)| index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_caret_at_a_soft_wrap_edge_moves_to_adjacent_visual_rows() {
        let layout = TextLayout::new("abcde\nabcdefghij", 5);
        let mut preferred = None;
        let mut trailing = false;
        let middle = layout.vertical(5, true, &mut preferred, &mut trailing);
        assert_eq!(layout.row(middle, trailing), 1);
        let first = layout.vertical(middle, false, &mut preferred, &mut trailing);
        assert_eq!(first, 5);
        assert_eq!(layout.row(first, trailing), 0);
        let (end, trailing) = layout.edge(8, true, false);
        assert_eq!(layout.row(end, trailing), 1);
    }

    #[test]
    fn vertical_motion_preserves_display_column_across_short_lines_and_joined_graphemes() {
        let text = "ab界\nx\nab👩‍💻c";
        let layout = TextLayout::new(text, 80);
        let mut preferred = None;
        let mut trailing = false;
        let short = layout.vertical(3, true, &mut preferred, &mut trailing);
        assert_eq!(short, 5);
        let next = layout.vertical(short, true, &mut preferred, &mut trailing);
        assert_eq!(
            text.graphemes(true).take(next).collect::<String>(),
            "ab界\nx\nab👩‍💻"
        );
        let back = layout.vertical(next, false, &mut preferred, &mut trailing);
        assert_eq!(
            layout.vertical(back, false, &mut preferred, &mut trailing),
            3
        );
    }

    #[test]
    fn hard_breaks_trailing_empty_rows_and_soft_wraps_have_reachable_caret_positions() {
        let text = "ab cd efghij\n";
        let layout = TextLayout::new(text, 5);
        assert_eq!(layout.row(text.graphemes(true).count(), false), 3);
        assert_eq!(layout.edge(8, false, false), (6, false));
        assert_eq!(layout.edge(8, true, false), (11, true));
        assert_eq!(wrap(&symbols("\n\n"), 5), [(0, 0), (1, 1), (2, 2)]);
        assert_eq!(symbols("\u{1b}\t👩‍💻\r\n"), ["�", "    ", "👩‍💻", "\n"]);
    }
}
