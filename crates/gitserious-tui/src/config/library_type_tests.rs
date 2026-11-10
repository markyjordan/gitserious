use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Cell;

use super::library_rows_tests::{Result, fixture, press, select};
use super::tests::Workspace;
use super::*;

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

fn every_row(state: &mut State, workspace: &Workspace, size: (u16, u16)) -> Result<Vec<Row>> {
    let Page {
        mut rows,
        mut hidden,
    } = inspector(state, size)?;
    while hidden > 0 {
        press(state, workspace, &[KeyCode::PageDown]);
        let next = inspector(state, size)?;
        assert!(next.hidden < hidden, "page down reveals more rows");
        let advanced = hidden - next.hidden;
        let overlap = next.rows.len().saturating_sub(advanced);
        for (row, again) in rows[rows.len() - overlap..].iter().zip(&next.rows) {
            assert_eq!(row.text, again.text, "the count of rows below is exact");
        }
        rows.extend_from_slice(&next.rows[overlap..]);
        hidden = next.hidden;
    }
    Ok(rows)
}

/// The rows of `rows` that continue the row at `at`: those that start with `indent`.
fn continued<'a>(rows: &'a [Row], at: usize, indent: &str) -> impl Iterator<Item = &'a Row> {
    rows[at + 1..]
        .iter()
        .take_while(move |row| row.text.starts_with(indent) && !row.text.trim().is_empty())
}

#[test]
fn a_built_in_type_lists_its_property_table_and_pages_to_the_fork_hint() -> Result {
    for size in SIZES {
        let (workspace, mut state) = fixture()?;
        select(&mut state, &["conventional", "fix"])?;
        press(&mut state, &workspace, &[KeyCode::Enter]);
        if size == (87, 40) {
            assert!(inspector(&mut state, size)?.hidden > 0, "fix overflows");
        }
        let rows = every_row(&mut state, &workspace, size)?;
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
        let header = rows
            .iter()
            .position(|row| row.text.starts_with("Property "))
            .ok_or("table header")?;
        assert_eq!(
            rows[header]
                .text
                .split('│')
                .map(str::trim)
                .collect::<Vec<_>>(),
            ["Property", "Policy", "Description"]
        );
        let mut at = header + 2;
        for (key, policy, guidance) in FIX {
            let start = at;
            assert!(
                !rows[start]
                    .text
                    .split('│')
                    .nth(2)
                    .ok_or("description cell")?
                    .trim()
                    .is_empty()
            );
            at += 1;
            while at < rows.len()
                && rows[at].text.contains('│')
                && rows[at]
                    .text
                    .split('│')
                    .nth(1)
                    .is_some_and(|cell| cell.trim().is_empty())
            {
                at += 1;
            }
            let cells = rows[start..at]
                .iter()
                .map(|row| row.text.split('│').map(str::trim).collect::<Vec<_>>())
                .collect::<Vec<_>>();
            assert_eq!(cells.iter().map(|row| row[0]).collect::<String>(), key);
            assert_eq!(cells[0][1], policy);
            assert_eq!(
                cells
                    .iter()
                    .map(|row| row[2])
                    .collect::<String>()
                    .split_whitespace()
                    .collect::<String>(),
                guidance.split_whitespace().collect::<String>()
            );
            for cell in rows[start].cells(policy)? {
                assert_eq!(
                    cell.fg,
                    if policy == "required" {
                        theme().blocks
                    } else {
                        theme().pending
                    }
                );
            }
        }
        assert_eq!(text(at), "");
        assert_eq!(text(at + 1), "f fork to customize");
        assert!(rows[at + 2..].iter().all(|row| row.text.trim().is_empty()));
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
        let properties = rows[first..]
            .iter()
            .filter(|row| {
                row.text
                    .split('│')
                    .nth(1)
                    .is_some_and(|cell| !cell.trim().is_empty())
            })
            .collect::<Vec<_>>();
        assert_eq!(properties.len(), 2);
        for (row, (name, policy, color)) in properties.iter().zip([
            ("intent", "required", theme().blocks),
            ("constraints", "conditional", theme().pending),
        ]) {
            let cells = row.text.split('│').map(str::trim).collect::<Vec<_>>();
            assert!(name.starts_with(cells[0]));
            assert_eq!(cells[1], policy);
            for cell in row.cells(policy)? {
                assert_eq!(cell.fg, color);
            }
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
