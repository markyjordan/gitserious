use super::*;
use gitserious_app::CitedCommit;

fn citing(paths: &[&str]) -> AuthoringSession {
    let mut session = AuthoringSession::new(built_in_commit_types(), Some(0));
    session.set_citation_paths(paths.iter().map(|path| (*path).to_owned()).collect());
    session
}

fn focused_text(session: &AuthoringSession) -> String {
    session
        .composer
        .text(session.composer.focus)
        .lines()
        .join("\n")
}

#[test]
fn at_opens_a_list_and_does_not_write_at() {
    let mut session = citing(&["src/net/retry.rs"]);
    paste(&mut session, "see ");
    press(&mut session, KeyCode::Char('@'));
    assert!(session.citation_picker.is_some());
    assert_eq!(focused_text(&session), "see ");
    assert_eq!(
        hints(&session),
        [
            ("enter", "cite"),
            ("↑/↓", "move"),
            ("←/→", "kind"),
            ("esc", "cancel"),
        ]
    );
}

#[test]
fn enter_inserts_the_path_and_esc_writes_nothing() {
    let mut session = citing(&["src/net/retry.rs", "README.md"]);
    press(&mut session, KeyCode::Char('@'));
    press(&mut session, KeyCode::Enter);
    assert!(session.citation_picker.is_none());
    assert_eq!(focused_text(&session), "@src/net/retry.rs");

    let mut session = citing(&["src/net/retry.rs"]);
    paste(&mut session, "see ");
    press(&mut session, KeyCode::Char('@'));
    typed(&mut session, "nope");
    press(&mut session, KeyCode::Esc);
    assert!(session.citation_picker.is_none());
    assert_eq!(focused_text(&session), "see ");
}

#[test]
fn typing_keeps_paths_that_contain_the_query() -> Result<(), Box<dyn Error>> {
    let mut session = citing(&["src/net/retry.rs", "README.md"]);
    press(&mut session, KeyCode::Char('@'));
    typed(&mut session, "retry");
    let screen = rendered(&mut session, 80, 24)?;
    assert!(screen.contains("src/net/retry.rs"), "{screen}");
    assert!(!screen.contains("README.md"), "{screen}");
    assert!(!screen.contains("Typing filters"), "{screen}");
    assert!(!screen.contains("tab kind"), "{screen}");
    press(&mut session, KeyCode::Enter);
    assert_eq!(focused_text(&session), "@src/net/retry.rs");
    Ok(())
}

#[test]
fn down_and_up_wrap_the_list() {
    let mut session = citing(&["README.md", "src/net/retry.rs"]);
    press(&mut session, KeyCode::Char('@'));
    press(&mut session, KeyCode::Down);
    press(&mut session, KeyCode::Enter);
    assert_eq!(focused_text(&session), "@src/net/retry.rs");

    let mut session = citing(&["README.md", "src/net/retry.rs"]);
    press(&mut session, KeyCode::Char('@'));
    press(&mut session, KeyCode::Up);
    press(&mut session, KeyCode::Enter);
    assert_eq!(focused_text(&session), "@src/net/retry.rs");
}

#[test]
fn a_directory_keeps_its_trailing_slash() {
    let mut session = citing(&["src/net/retry.rs", "src/net/"]);
    press(&mut session, KeyCode::Char('@'));
    press(&mut session, KeyCode::Down);
    press(&mut session, KeyCode::Enter);
    assert_eq!(focused_text(&session), "@src/net/");
}

#[test]
fn enter_on_no_match_leaves_the_list_open() -> Result<(), Box<dyn Error>> {
    let mut session = citing(&["src/net/retry.rs"]);
    press(&mut session, KeyCode::Char('@'));
    typed(&mut session, "zzz");
    let screen = rendered(&mut session, 80, 24)?;
    assert!(screen.contains("no match"), "{screen}");
    press(&mut session, KeyCode::Enter);
    assert!(session.citation_picker.is_some());
    assert_eq!(focused_text(&session), "");
    Ok(())
}

#[test]
fn cite_lands_at_the_cursor() {
    let mut session = citing(&["src/net/retry.rs"]);
    press(&mut session, KeyCode::Down);
    paste(&mut session, "abXcd");
    press(&mut session, KeyCode::Left);
    press(&mut session, KeyCode::Left);
    press(&mut session, KeyCode::Char('@'));
    press(&mut session, KeyCode::Enter);
    assert_eq!(focused_text(&session), "abX@src/net/retry.rscd");
}

#[test]
fn the_list_takes_the_keys_that_would_leave_the_composer() {
    let mut session = citing(&["src/a.rs"]);
    press(&mut session, KeyCode::Down);
    press(&mut session, KeyCode::Down);
    press(&mut session, KeyCode::Down);
    press(&mut session, KeyCode::Char('@'));
    control(&mut session, 's');
    control(&mut session, 't');
    alt(&mut session, 'n');
    assert_eq!(session.stage, Stage::Compose);
    assert!(session.citation_picker.is_some());
    assert!(session.review.is_none());
    assert_eq!(
        session
            .composer
            .field(session.composer.focus)
            .map(|field| field.requirement.word()),
        Some("conditional")
    );
}

#[test]
fn the_selected_path_uses_the_popup_row() -> Result<(), Box<dyn Error>> {
    let mut session = citing(&["src/net/retry.rs", "README.md"]);
    press(&mut session, KeyCode::Char('@'));
    let buffer = rendered_buffer(&mut session, 80, 24)?;
    let (column, row) = find_ascii(&buffer, 80, 24, "src/net/retry.rs").ok_or("missing path")?;
    assert_eq!(buffer[(column, row)].bg, theme().cursor);
    let (column, row) = find_ascii(&buffer, 80, 24, "README.md").ok_or("missing readme")?;
    assert_ne!(buffer[(column, row)].bg, theme().cursor);
    assert_eq!(buffer[(column - 1, row)].bg, theme().band);
    assert!(!rendered(&mut session, 80, 24)?.contains("Cite"));
    Ok(())
}

#[test]
fn the_query_row_prompts_until_something_is_typed() -> Result<(), Box<dyn Error>> {
    let mut session = with_commits(&["src/net/retry.rs"], &[("abc1234", "back off")]);
    press(&mut session, KeyCode::Char('@'));
    let text = rendered(&mut session, 80, 24)?;
    assert!(text.contains("Type to filter files"), "{text}");
    press(&mut session, KeyCode::Right);
    let text = rendered(&mut session, 80, 24)?;
    assert!(text.contains("Type to filter commits"), "{text}");
    typed(&mut session, "back");
    let text = rendered(&mut session, 80, 24)?;
    assert!(!text.contains("Type to filter"), "{text}");
    let rows = screen(&mut session, 80, 24)?;
    let sha = rows
        .iter()
        .position(|row| row.contains("abc1234"))
        .ok_or("missing commit")?;
    assert_eq!(rows[sha + 2].trim(), "back", "{rows:#?}");
    Ok(())
}

#[test]
fn the_list_opens_under_the_line_being_written() -> Result<(), Box<dyn Error>> {
    let mut session = citing(&["src/net/retry.rs"]);
    press(&mut session, KeyCode::Down);
    paste(&mut session, "see ");
    press(&mut session, KeyCode::Char('@'));
    let buffer = rendered_buffer(&mut session, 80, 24)?;
    let (_, line) = find_ascii(&buffer, 80, 24, "see ").ok_or("missing body")?;
    let (_, kinds) = find_ascii(&buffer, 80, 24, "Files").ok_or("missing kinds")?;
    let (_, path) = find_ascii(&buffer, 80, 24, "src/net/retry.rs").ok_or("missing path")?;
    assert_eq!(kinds, line + 3);
    assert_eq!(path, kinds + 2);
    Ok(())
}

#[test]
fn the_list_opens_above_a_line_near_the_bottom() -> Result<(), Box<dyn Error>> {
    let mut session = citing(&["src/net/retry.rs"]);
    press(&mut session, KeyCode::Down);
    paste(&mut session, &format!("{}last ", "line\n".repeat(11)));
    press(&mut session, KeyCode::Char('@'));
    let buffer = rendered_buffer(&mut session, 80, 24)?;
    let (_, line) = find_ascii(&buffer, 80, 24, "last ").ok_or("missing body")?;
    let (_, path) = find_ascii(&buffer, 80, 24, "src/net/retry.rs").ok_or("missing path")?;
    assert!(path < line, "{}", rendered(&mut session, 80, 24)?);
    Ok(())
}

#[test]
fn at_before_a_type_is_chosen_stays_in_the_picker() {
    let mut session = AuthoringSession::new(built_in_commit_types(), None);
    press(&mut session, KeyCode::Char('@'));
    assert_eq!(session.stage, Stage::SelectType);
    assert!(session.citation_picker.is_none());
    assert!(!session.composer.dirty());
}

#[test]
fn a_pasted_query_filters_the_list() {
    let mut session = citing(&["src/net/retry.rs", "README.md"]);
    press(&mut session, KeyCode::Char('@'));
    paste(&mut session, "README");
    press(&mut session, KeyCode::Enter);
    assert_eq!(focused_text(&session), "@README.md");
}

fn with_commits(paths: &[&str], commits: &[(&str, &str)]) -> AuthoringSession {
    let mut session = citing(paths);
    session.set_citation_commits(
        commits
            .iter()
            .map(|(sha, subject)| CitedCommit {
                sha: (*sha).to_owned(),
                subject: (*subject).to_owned(),
            })
            .collect(),
    );
    session
}

#[test]
fn right_keeps_the_query_and_inserts_the_short_sha() {
    let mut session = with_commits(
        &["src/net/retry.rs"],
        &[
            ("def5678", "rename the pool"),
            ("abc1234", "back off on 429"),
        ],
    );
    press(&mut session, KeyCode::Char('@'));
    typed(&mut session, "429");
    press(&mut session, KeyCode::Right);
    press(&mut session, KeyCode::Enter);
    assert!(session.citation_picker.is_none());
    assert_eq!(focused_text(&session), "@abc1234");
}

#[test]
fn left_returns_to_files_and_esc_on_a_commit_writes_nothing() {
    let mut session = with_commits(&["README.md"], &[("abc1234", "back off")]);
    press(&mut session, KeyCode::Char('@'));
    press(&mut session, KeyCode::Right);
    press(&mut session, KeyCode::Left);
    press(&mut session, KeyCode::Enter);
    assert_eq!(focused_text(&session), "@README.md");

    let mut session = with_commits(&["README.md"], &[("abc1234", "back off")]);
    paste(&mut session, "see ");
    press(&mut session, KeyCode::Char('@'));
    press(&mut session, KeyCode::Right);
    press(&mut session, KeyCode::Esc);
    assert!(session.citation_picker.is_none());
    assert_eq!(focused_text(&session), "see ");
}

#[test]
fn the_commit_list_shows_the_sha_and_subject() -> Result<(), Box<dyn Error>> {
    let mut session = with_commits(&["README.md"], &[("abc1234", "back off on 429")]);
    press(&mut session, KeyCode::Char('@'));
    press(&mut session, KeyCode::Right);
    let screen = rendered(&mut session, 80, 24)?;
    assert!(screen.contains("Commits"), "{screen}");
    assert!(screen.contains("abc1234"), "{screen}");
    assert!(screen.contains("back off on 429"), "{screen}");
    assert!(!screen.contains("Typing filters"), "{screen}");
    assert!(!screen.contains("@abc1234"), "{screen}");
    Ok(())
}

/// The columns of `row` drawn on a panel, banded or highlighted.
fn panel_columns(buffer: &Buffer, width: u16, row: u16) -> Option<(u16, u16)> {
    let on_panel = |column: &u16| {
        let bg = buffer[(*column, row)].bg;
        bg == theme().band || bg == theme().cursor
    };
    let first = (0..width).find(on_panel)?;
    let last = (0..width).rev().find(on_panel)?;
    Some((first, last))
}

/// The selected row of a picker panel.
#[derive(Debug, PartialEq)]
struct SelectedRow {
    /// The first and last column the panel covers.
    columns: (u16, u16),
    /// The column the selected text starts at.
    text: u16,
    fg: Color,
    bg: Color,
    bold: bool,
}

fn selected_row(
    buffer: &Buffer,
    width: u16,
    height: u16,
    text: &str,
) -> Result<SelectedRow, Box<dyn Error>> {
    let (column, row) = find_ascii(buffer, width, height, text).ok_or("missing selected text")?;
    let columns = panel_columns(buffer, width, row).ok_or("the selected row has no panel")?;
    for x in 0..width {
        let highlighted = buffer[(x, row)].bg == theme().cursor;
        assert_eq!(
            highlighted,
            (columns.0..=columns.1).contains(&x),
            "the highlight spans the panel at ({x}, {row})"
        );
    }
    let cell = &buffer[(column, row)];
    Ok(SelectedRow {
        columns,
        text: column,
        fg: cell.fg,
        bg: cell.bg,
        bold: cell.modifier.contains(Modifier::BOLD),
    })
}

#[test]
fn the_citation_panel_is_banded_inset_and_highlights_its_whole_row() -> Result<(), Box<dyn Error>> {
    for (width, height) in [(80, 24), (100, 30)] {
        let mut session = citing(&["src/net/retry.rs", "README.md"]);
        press(&mut session, KeyCode::Char('@'));
        let buffer = rendered_buffer(&mut session, width, height)?;
        let row = selected_row(&buffer, width, height, "src/net/retry.rs")?;
        let columns = row.columns;
        assert_eq!(columns, (1, width - 2), "at {width}x{height}");
        assert_eq!(
            row.text,
            columns.0 + 2,
            "two columns of inset at {width}x{height}"
        );
        assert_eq!(
            (row.fg, row.bg, row.bold),
            (theme().ground, theme().cursor, true)
        );

        let (_, readme) =
            find_ascii(&buffer, width, height, "README.md").ok_or("missing readme")?;
        assert_eq!(panel_columns(&buffer, width, readme), Some(columns));
        for x in columns.0..=columns.1 {
            assert_eq!(buffer[(x, readme)].bg, theme().band, "({x}, {readme})");
        }
    }
    Ok(())
}

#[test]
fn the_citation_sources_underline_the_active_one() -> Result<(), Box<dyn Error>> {
    let mut session = with_commits(&["README.md"], &[("abc1234", "back off")]);
    press(&mut session, KeyCode::Char('@'));
    let styles = |session: &mut AuthoringSession| -> Result<_, Box<dyn Error>> {
        let buffer = rendered_buffer(session, 80, 24)?;
        let files = find_ascii(&buffer, 80, 24, "Files").ok_or("missing files")?;
        let commits = find_ascii(&buffer, 80, 24, "Commits").ok_or("missing commits")?;
        assert_eq!(commits.0, files.0 + 7, "two columns between the sources");
        Ok([files, commits].map(|position| {
            let cell = &buffer[position];
            (cell.fg, cell.modifier)
        }))
    };
    let active = (theme().text, Modifier::BOLD | Modifier::UNDERLINED);
    let inactive = (theme().dim, Modifier::empty());
    assert_eq!(styles(&mut session)?, [active, inactive]);
    press(&mut session, KeyCode::Right);
    assert_eq!(styles(&mut session)?, [inactive, active]);
    Ok(())
}

#[test]
fn the_selected_path_stays_visible_when_the_list_scrolls() -> Result<(), Box<dyn Error>> {
    let paths: Vec<String> = (0..12)
        .map(|index| format!("src/file{index:02}.rs"))
        .collect();
    let mut session = citing(&paths.iter().map(String::as_str).collect::<Vec<_>>());
    press(&mut session, KeyCode::Char('@'));
    let shown = |session: &mut AuthoringSession| -> Result<Vec<String>, Box<dyn Error>> {
        let rows = screen(session, 80, 24)?;
        Ok(rows
            .iter()
            .filter_map(|row| row.split_whitespace().find(|word| word.starts_with("src/")))
            .map(str::to_owned)
            .collect())
    };
    assert_eq!(shown(&mut session)?, paths[..8]);
    press(&mut session, KeyCode::Up);
    assert_eq!(
        shown(&mut session)?,
        paths[4..],
        "up from the top wraps to the last"
    );
    let buffer = rendered_buffer(&mut session, 80, 24)?;
    let (column, row) = find_ascii(&buffer, 80, 24, "src/file11.rs").ok_or("missing last")?;
    assert_eq!(buffer[(column, row)].bg, theme().cursor);
    press(&mut session, KeyCode::Up);
    assert_eq!(
        shown(&mut session)?,
        paths[4..],
        "moving inside the view does not scroll"
    );
    press(&mut session, KeyCode::Down);
    press(&mut session, KeyCode::Down);
    assert_eq!(
        shown(&mut session)?,
        paths[..8],
        "down from the last wraps to the top"
    );
    Ok(())
}

#[test]
fn both_pickers_draw_the_same_band_inset_and_highlight() -> Result<(), Box<dyn Error>> {
    for (width, height) in [(80, 24), (100, 30)] {
        let mut types = AuthoringSession::new(built_in_commit_types(), None);
        let types_buffer = rendered_buffer(&mut types, width, height)?;
        let type_row = selected_row(&types_buffer, width, height, "feat")?;

        let mut citation = citing(&["src/net/retry.rs"]);
        press(&mut citation, KeyCode::Char('@'));
        let citation_buffer = rendered_buffer(&mut citation, width, height)?;
        let citation_row = selected_row(&citation_buffer, width, height, "src/net/retry.rs")?;

        assert_eq!(type_row, citation_row, "at {width}x{height}");
    }
    Ok(())
}

/// The rows the citation panel covers, read from its band at `column`.
fn panel_rows(buffer: &Buffer, column: u16, height: u16) -> Vec<u16> {
    (0..height)
        .filter(|row| {
            let bg = buffer[(column, *row)].bg;
            bg == theme().band || bg == theme().cursor
        })
        .collect()
}

#[test]
fn the_citation_panel_keeps_its_height_as_the_query_narrows() -> Result<(), Box<dyn Error>> {
    let paths = [
        "src/a.rs",
        "src/b.rs",
        "src/c.rs",
        "README.md",
        "Cargo.toml",
    ];
    let mut session = citing(&paths);
    press(&mut session, KeyCode::Char('@'));
    let rows = |session: &mut AuthoringSession| -> Result<Vec<u16>, Box<dyn Error>> {
        Ok(panel_rows(&rendered_buffer(session, 80, 24)?, 2, 24))
    };
    let full = rows(&mut session)?;
    assert_eq!(
        full.len(),
        11,
        "five rows, the source row, the query row, and padding"
    );
    typed(&mut session, "src");
    assert_eq!(rows(&mut session)?, full, "three matches");
    typed(&mut session, "/a");
    assert_eq!(rows(&mut session)?, full, "one match");
    typed(&mut session, "zz");
    assert_eq!(rows(&mut session)?, full, "no match");
    Ok(())
}

#[test]
fn no_composer_text_shows_through_the_citation_panel() -> Result<(), Box<dyn Error>> {
    let mut session = citing(&["src/a.rs", "src/b.rs", "src/c.rs"]);
    press(&mut session, KeyCode::Down);
    paste(
        &mut session,
        &format!(
            "{}\nsee ",
            "qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq\n".repeat(3)
        ),
    );
    for _ in 0..3 {
        press(&mut session, KeyCode::Up);
    }
    press(&mut session, KeyCode::Char('@'));
    let buffer = rendered_buffer(&mut session, 80, 24)?;
    let rows = panel_rows(&buffer, 2, 24);
    assert!(!rows.is_empty());
    let (first, last) = panel_columns(&buffer, 80, rows[0]).ok_or("missing panel")?;
    for row in &rows {
        let text: String = (first..=last).map(|x| buffer[(x, *row)].symbol()).collect();
        assert!(!text.contains('q'), "row {row}: {text}");
    }
    assert!(
        rendered(&mut session, 80, 24)?.contains("qqqq"),
        "the body still shows outside the panel"
    );
    Ok(())
}

#[test]
fn the_citation_panel_follows_the_caret_after_the_composer_scrolls() -> Result<(), Box<dyn Error>> {
    let mut session = citing(&["src/net/retry.rs"]);
    press(&mut session, KeyCode::Down);
    let lines: Vec<String> = (0..40).map(|index| format!("line {index:02}")).collect();
    paste(&mut session, &lines.join("\n"));
    let open_at = |session: &mut AuthoringSession,
                   ups: usize,
                   caret_text: &str|
     -> Result<(u16, Vec<u16>, u16), Box<dyn Error>> {
        for _ in 0..ups {
            press(session, KeyCode::Up);
        }
        rendered(session, 80, 24)?;
        press(session, KeyCode::Char('@'));
        let buffer = rendered_buffer(session, 80, 24)?;
        assert!(
            find_ascii(&buffer, 80, 24, "line 00").is_none(),
            "the composer has scrolled"
        );
        let (_, caret) = find_ascii(&buffer, 80, 24, caret_text).ok_or("missing caret line")?;
        let (_, sources) = find_ascii(&buffer, 80, 24, "Files").ok_or("missing panel")?;
        press(session, KeyCode::Esc);
        Ok((caret, panel_rows(&buffer, 2, 24), sources))
    };
    let (caret, panel, _) = open_at(&mut session, 12, "line 27")?;
    assert_eq!(
        panel.last(),
        Some(&(caret - 2)),
        "above, one blank row over the caret"
    );
    let (caret, _, sources) = open_at(&mut session, 8, "line 19")?;
    assert_eq!(sources, caret + 3, "under, one blank row below the caret");
    Ok(())
}
