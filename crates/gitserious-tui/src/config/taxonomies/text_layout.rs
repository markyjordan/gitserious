use ratatui::text::Span;

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

pub(super) fn cursor_row(rows: &[(usize, usize)], cursor: usize) -> usize {
    rows.iter()
        .enumerate()
        .find(|(index, (start, end))| {
            (*start..*end).contains(&cursor)
                || (cursor == *end && rows.get(index + 1).is_none_or(|(next, _)| *next > cursor))
        })
        .map_or(rows.len() - 1, |(index, _)| index)
}
