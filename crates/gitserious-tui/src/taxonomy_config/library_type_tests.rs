use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Cell;

use super::library_rows_tests::{Result, fixture, press, select};
use super::tests::Workspace;
use super::*;

const DIM: Color = Color::Gray;
const SIZES: [(u16, u16); 3] = [(87, 40), (125, 51), (100, 30)];

/// The fix type's properties as conventional defines them: key, policy, and description.
const FIX: [(&str, &str, &str); 3] = [
    (
        "cause",
        "required",
        "The causal model found while debugging. The diff shows what changed, not why it was the cause.",
    ),
    (
        "decision",
        "required",
        "Why this layer was repaired rather than a superficially equivalent intervention.",
    ),
    (
        "constraints",
        "conditional",
        "Why the obvious fix was unacceptable, and what behavior the repair must preserve.",
    ),
];

/// One Inspector row as drawn: its text past the gutter and its cells.
#[derive(Clone)]
struct Row {
    text: String,
    cells: Vec<Cell>,
}

impl Row {
    /// The cells of `word` in this row.
    fn cells(&self, word: &str) -> Result<&[Cell]> {
        let start = self
            .text
            .find(word)
            .ok_or_else(|| format!("{word:?} not in {:?}", self.text))?;
        let start = self.text[..start].chars().count();
        Ok(&self.cells[start..start + word.chars().count()])
    }
}

/// The Inspector as drawn: the rows it shows and how many its last row counts below them.
struct Page {
    rows: Vec<Row>,
    hidden: usize,
}

fn inspector(state: &mut State, size: (u16, u16)) -> Result<Page> {
    let mut terminal = Terminal::new(TestBackend::new(size.0, size.1))?;
    terminal.draw(|frame| state.render(frame))?;
    let area = state.taxonomies.observe(state.library()).inspector_area;
    let buffer = terminal.backend().buffer();
    let row = |y: u16| {
        // Past the gutter, where EDIT marks the focused field.
        let cells = (area.x + 1..area.right())
            .map(|x| buffer[(x, y)].clone())
            .collect::<Vec<_>>();
        Row {
            text: cells.iter().map(Cell::symbol).collect(),
            cells,
        }
    };
    let below = row(area.bottom());
    let hidden = match below.text.trim().strip_prefix("↓ ") {
        Some(count) => count.trim_end_matches(" more").parse()?,
        None => 0,
    };
    Ok(Page {
        rows: (area.y..area.bottom()).map(row).collect(),
        hidden,
    })
}

/// Every row of the Inspector, from its first page and, when that page counts rows below, the
/// page pgdn shows. The count must match the rows pgdn reveals.
fn every_row(state: &mut State, workspace: &Workspace, size: (u16, u16)) -> Result<Vec<Row>> {
    let Page { mut rows, hidden } = inspector(state, size)?;
    if hidden == 0 {
        return Ok(rows);
    }
    press(state, workspace, &[KeyCode::PageDown]);
    let last = inspector(state, size)?;
    assert_eq!(last.hidden, 0, "one page down reaches the end");
    // The last page ends on the last row, so it starts `hidden` rows into the first page.
    let shown = rows.len();
    assert_eq!(last.rows.len(), shown);
    assert!(hidden <= shown);
    for (row, again) in rows[hidden..].iter().zip(&last.rows) {
        assert_eq!(row.text, again.text, "the count of rows below is exact");
    }
    rows.extend_from_slice(&last.rows[shown - hidden..]);
    Ok(rows)
}

/// The rows of `rows` that continue the row at `at`: those that start with `indent`.
fn continued<'a>(rows: &'a [Row], at: usize, indent: &str) -> impl Iterator<Item = &'a Row> {
    rows[at + 1..]
        .iter()
        .take_while(move |row| row.text.starts_with(indent) && !row.text.trim().is_empty())
}

#[test]
fn a_built_in_type_lists_its_properties_without_a_heading_and_pages_to_the_fork_hint() -> Result {
    for size in SIZES {
        let (workspace, mut state) = fixture()?;
        select(&mut state, &["conventional", "fix"])?;
        press(&mut state, &workspace, &[KeyCode::Enter]);
        if size == (87, 40) {
            assert!(inspector(&mut state, size)?.hidden > 0, "fix overflows");
        }
        let rows = every_row(&mut state, &workspace, size)?;
        let width = rows[0].text.chars().count();
        let text = |at: usize| rows[at].text.trim_end();

        assert!(text(0).starts_with("Type fix "), "{}", text(0));
        assert!(text(0).ends_with("◇ read-only"), "{}", text(0));
        assert_eq!(text(1), "in conventional");
        assert_eq!(text(2), "");
        assert!(text(3).starts_with("Description  A causal correction"));
        let description = std::iter::once(&rows[3].text[13..])
            .chain(continued(&rows, 3, &" ".repeat(13)).map(|row| row.text.as_str()))
            .map(str::trim)
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(
            description,
            "A causal correction of an observed failure or incorrect behavior."
        );
        let mut at = 3 + continued(&rows, 3, &" ".repeat(13)).count() + 1;
        assert_eq!(text(at), "");
        for (key, policy, guidance) in FIX {
            at += 1;
            let row = &rows[at];
            assert!(row.text.starts_with(&format!("{key} ")), "{}", row.text);
            // Flush right, one column short of the pane's edge like the heading's badge.
            assert_eq!(
                row.text.trim_end().chars().count(),
                width - 1,
                "{}",
                row.text
            );
            assert!(row.text.trim_end().ends_with(&format!(" {policy}")));
            for cell in row.cells(key)? {
                assert_ne!(cell.fg, DIM);
                assert!(!cell.modifier.contains(Modifier::BOLD));
            }
            let style = if policy == "required" {
                theme().text
            } else {
                DIM
            };
            for cell in row.cells(policy)? {
                assert_eq!(cell.fg, style);
            }
            let lines = continued(&rows, at, "  ").collect::<Vec<_>>();
            assert!(!lines.is_empty(), "{key} has its guidance beneath it");
            for line in &lines {
                assert!(!line.text.starts_with("   "), "indented 2: {:?}", line.text);
                for cell in line.cells(line.text.trim())? {
                    assert_eq!(cell.fg, DIM);
                }
            }
            assert_eq!(
                lines
                    .iter()
                    .map(|line| line.text.trim())
                    .collect::<Vec<_>>()
                    .join(" "),
                guidance
            );
            at += lines.len();
        }
        assert_eq!(text(at + 1), "");
        assert_eq!(text(at + 2), "f fork to customize");
        assert!(rows[at + 3..].iter().all(|row| row.text.trim().is_empty()));
        assert!(
            !rows
                .iter()
                .any(|row| row.text.contains("The composer asks"))
        );
        assert!(!rows.iter().any(|row| row.text.starts_with("Properties")));
        assert!(!rows.iter().any(|row| row.text.starts_with("Name ")));
    }
    Ok(())
}

#[test]
fn a_custom_type_shows_each_policy_in_the_outline_style() -> Result {
    for size in SIZES {
        let (workspace, mut state) = fixture()?;
        select(&mut state, &["team"])?;
        press(&mut state, &workspace, &[KeyCode::Enter]);
        select(&mut state, &["team", "feat"])?;
        let rows = every_row(&mut state, &workspace, size)?;
        assert!(rows[0].text.trim_end() == "Type feat", "{}", rows[0].text);
        assert_eq!(rows[1].text.trim_end(), "in team");
        assert!(
            !rows
                .iter()
                .any(|row| row.text.contains("The composer asks"))
        );
        let first = rows
            .iter()
            .position(|row| row.text.starts_with("intent "))
            .ok_or("intent guidance")?;
        let keys = rows[first..]
            .iter()
            .filter(|row| !row.text.starts_with(' '))
            .map(|row| row.text.split_whitespace().collect::<Vec<_>>())
            .collect::<Vec<_>>();
        assert_eq!(
            keys,
            [["intent", "required"], ["constraints", "conditional"]]
        );
        let row = |key: &str| {
            rows.iter()
                .find(|row| row.text.starts_with(&format!("{key} ")))
                .ok_or(format!("no {key} row"))
        };
        for cell in row("intent")?.cells("required")? {
            assert_eq!(cell.fg, theme().text);
        }
        for cell in row("constraints")?.cells("conditional")? {
            assert_eq!(cell.fg, DIM);
        }
        let text = rows.iter().map(|row| row.text.as_str()).collect::<String>();
        assert!(!text.contains("fork"), "{text}");
    }
    Ok(())
}

#[test]
fn edit_on_a_type_keeps_its_fields() -> Result {
    for size in SIZES {
        let (workspace, mut state) = fixture()?;
        select(&mut state, &["team"])?;
        press(&mut state, &workspace, &[KeyCode::Enter]);
        select(&mut state, &["team", "feat"])?;
        press(&mut state, &workspace, &[KeyCode::Char('e')]);
        assert!(state.taxonomies.editing());
        let rows = inspector(&mut state, size)?.rows;
        let labels = rows
            .iter()
            .skip(2)
            .filter_map(|row| row.text.split("  ").next())
            .filter(|label| !label.trim().is_empty())
            .collect::<Vec<_>>();
        assert_eq!(labels, ["Name", "Description", "Properties"]);
        assert!(!rows.iter().any(|row| row.text.contains("composer")));
    }
    Ok(())
}
