use std::path::Path;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::Paragraph;
use unicode_segmentation::UnicodeSegmentation;

use crate::theme::theme;

/// One row level with the rail's top padding, the title, the file, and a blank row.
pub(super) const HEIGHT: u16 = 4;

/// Names the page and the file the Library saves to.
pub(super) fn render(frame: &mut Frame<'_>, area: Rect, path: &Path, home: Option<&Path>) {
    let [_, title, file] =
        Layout::vertical([Constraint::Length(1); 3]).areas(area.inner(Margin::new(1, 0)));
    let dim = Style::new().fg(theme().dim);
    frame.render_widget(
        Paragraph::new(Span::styled(
            "Library",
            Style::new().fg(theme().text).add_modifier(Modifier::BOLD),
        )),
        title,
    );
    frame.render_widget(
        Paragraph::new(Span::styled("saves as you go", dim)).right_aligned(),
        title,
    );
    let shown = keep_end(&home_relative(path, home), usize::from(file.width));
    frame.render_widget(Paragraph::new(Span::styled(shown, dim)), file);
}

/// `path` with a leading `home` written as `~`.
fn home_relative(path: &Path, home: Option<&Path>) -> String {
    // A HOME of `/` would turn every absolute path into `~/...`.
    match home
        .filter(|home| home.parent().is_some())
        .and_then(|home| path.strip_prefix(home).ok())
    {
        Some(rest) => Path::new("~").join(rest).display().to_string(),
        None => path.display().to_string(),
    }
}

/// The end of `text` that fits in `width` columns, led by `…` when the start is cut.
fn keep_end(text: &str, width: usize) -> String {
    if Span::raw(text).width() <= width {
        return text.to_owned();
    }
    if width == 0 {
        return String::new();
    }
    let mut kept = Vec::new();
    let mut used = 1;
    for grapheme in text.graphemes(true).rev() {
        used += Span::raw(grapheme).width();
        if used > width {
            break;
        }
        kept.push(grapheme);
    }
    std::iter::once("…").chain(kept.into_iter().rev()).collect()
}

#[cfg(test)]
mod tests {
    use std::path::MAIN_SEPARATOR;

    use super::*;

    #[test]
    fn a_file_under_home_reads_from_tilde() {
        let path = Path::new("/home/tester/.config/gitserious/config.toml");
        assert_eq!(
            home_relative(path, Some(Path::new("/home/tester"))),
            format!("~{MAIN_SEPARATOR}.config/gitserious/config.toml")
        );
        assert_eq!(
            home_relative(path, Some(Path::new("/home/test"))),
            path.display().to_string()
        );
        assert_eq!(home_relative(path, None), path.display().to_string());
        assert_eq!(
            home_relative(path, Some(Path::new("/"))),
            path.display().to_string()
        );
    }

    #[test]
    fn a_long_path_keeps_its_end() {
        assert_eq!(keep_end("~/config.toml", 13), "~/config.toml");
        assert_eq!(keep_end("~/config.toml", 12), "…config.toml");
        assert_eq!(keep_end("~/config.toml", 1), "…");
        assert_eq!(keep_end("~/config.toml", 0), "");
        assert_eq!(keep_end("/日本/a", 4), "…/a");
    }
}
