//! Paths as screens show them: whole, and wrapped after a separator rather than inside a name.

use unicode_width::UnicodeWidthStr;

/// `path` in lines at most `width` columns wide, each break after a separator. A name wider
/// than `width` is the only thing cut inside.
pub(crate) fn path_lines(path: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = Vec::new();
    let mut line = String::new();
    for part in path.split_inclusive(['/', '\\']) {
        if !line.is_empty() && line.width() + part.width() > width {
            lines.push(std::mem::take(&mut line));
        }
        for character in part.chars() {
            if line.width() + character.to_string().width() > width {
                lines.push(std::mem::take(&mut line));
            }
            line.push(character);
        }
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_breaks_after_separators_and_cuts_only_a_name_too_wide_to_fit() {
        assert_eq!(
            path_lines("/Users/marky/Documents/projects/gitserious", 20),
            ["/Users/marky/", "Documents/projects/", "gitserious"]
        );
        assert_eq!(
            path_lines(r"C:\Users\marky\gitserious", 12),
            [r"C:\Users\", r"marky\", "gitserious"]
        );
        assert_eq!(
            path_lines("/a-very-long-name", 8),
            ["/", "a-very-l", "ong-name"]
        );
        assert_eq!(path_lines("/short", 20), ["/short"]);
        assert_eq!(path_lines("", 20), [""]);
    }
}
