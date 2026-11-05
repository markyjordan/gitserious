use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::{Buffer, Cell};

use super::library_rows_tests::{Result, fixture, press, select};
use super::*;

const DIM: Color = Color::Gray;
const SIZES: [(u16, u16); 2] = [(87, 40), (100, 30)];
const POLICIES: [&str; 4] = ["required", "recommended", "conditional", "optional"];

/// One row of the screen: where it is and its text.
struct Row<'a> {
    y: u16,
    x: u16,
    text: String,
    buffer: &'a Buffer,
}

impl Row<'_> {
    /// The cells of `word` in this row.
    fn cells(&self, word: &str) -> Result<Vec<&Cell>> {
        let start = self
            .text
            .find(word)
            .ok_or_else(|| format!("{word:?} not in {:?}", self.text))?;
        let x = self.x + u16::try_from(self.text[..start].chars().count())?;
        Ok((x..x + u16::try_from(word.chars().count())?)
            .map(|x| &self.buffer[(x, self.y)])
            .collect())
    }
}

fn draw(state: &mut State, (width, height): (u16, u16)) -> Result<Terminal<TestBackend>> {
    let mut terminal = Terminal::new(TestBackend::new(width, height))?;
    terminal.draw(|frame| state.render(frame))?;
    Ok(terminal)
}

/// The rows of `area`, each as drawn.
fn rows(buffer: &Buffer, area: Rect) -> Vec<Row<'_>> {
    (area.y..area.bottom())
        .map(|y| Row {
            y,
            x: area.x,
            text: (area.x..area.right())
                .map(|x| buffer[(x, y)].symbol())
                .collect(),
            buffer,
        })
        .collect()
}

fn outline_row<'a>(state: &State, buffer: &'a Buffer, marker: &str) -> Result<Row<'a>> {
    let area = state.taxonomies.observe(state.library()).outline_area;
    rows(buffer, area)
        .into_iter()
        .find(|row| row.text.contains(marker))
        .ok_or_else(|| format!("no outline row holds {marker:?}").into())
}

fn inspector<'a>(state: &State, buffer: &'a Buffer) -> Vec<Row<'a>> {
    rows(
        buffer,
        state.taxonomies.observe(state.library()).inspector_area,
    )
}

/// The Inspector's field labels, top to bottom.
fn labels(rows: &[Row<'_>]) -> Vec<String> {
    rows.iter()
        .skip(2)
        .filter_map(|row| {
            // Past the gutter, where EDIT marks the focused field.
            let label = row.text.chars().skip(1).take(13).collect::<String>();
            let label = label.trim();
            (!label.is_empty()).then(|| label.to_owned())
        })
        .collect()
}

/// The Library with team's feat open and `intent`, a required property, under the cursor.
fn on_intent() -> Result<(super::tests::Workspace, State)> {
    let (workspace, mut state) = fixture()?;
    select(&mut state, &["team"])?;
    press(
        &mut state,
        &workspace,
        &[KeyCode::Enter, KeyCode::Down, KeyCode::Enter, KeyCode::Down],
    );
    assert_eq!(
        state.taxonomies.observe(state.library()).cursor_path,
        ["team", "feat", "intent"]
    );
    Ok((workspace, state))
}

fn is_red(color: Color) -> bool {
    matches!(color, Color::Red | Color::LightRed) || color == theme().blocks
}

#[test]
fn policy_words_are_plain_text_and_a_selected_required_row_reads() -> Result {
    for size in SIZES {
        let (workspace, mut state) = on_intent()?;
        let terminal = draw(&mut state, size)?;
        let buffer = terminal.backend().buffer();
        // The focused cursor bar draws every word, the policy too, in its ground colour.
        let bar = outline_row(&state, buffer, "intent ")?;
        for cell in bar.cells("required")? {
            assert_eq!((cell.fg, cell.bg), (theme().ground, theme().cursor));
            assert_ne!(cell.fg, cell.bg);
        }
        let conditional = outline_row(&state, buffer, "constraints ")?;
        for cell in conditional.cells("conditional")? {
            assert_eq!((cell.fg, cell.bg), (DIM, JET_BLACK));
        }

        // Off the cursor, required is bright at normal weight and the rest dim.
        press(&mut state, &workspace, &[KeyCode::Down]);
        let terminal = draw(&mut state, size)?;
        let buffer = terminal.backend().buffer();
        for cell in outline_row(&state, buffer, "intent ")?.cells("required")? {
            assert_eq!((cell.fg, cell.bg), (theme().text, JET_BLACK));
            assert!(!cell.modifier.contains(Modifier::BOLD));
        }

        // The raised row of an outline without focus keeps each word's colour.
        press(&mut state, &workspace, &[KeyCode::Up, KeyCode::Char('e')]);
        assert!(state.taxonomies.editing());
        let terminal = draw(&mut state, size)?;
        let buffer = terminal.backend().buffer();
        for cell in outline_row(&state, buffer, "intent ")?.cells("required")? {
            assert_eq!((cell.fg, cell.bg), (theme().text, theme().band));
        }
        for row in rows(buffer, buffer.area) {
            for policy in POLICIES {
                for (start, _) in row.text.match_indices(policy) {
                    let x = u16::try_from(row.text[..start].chars().count())?;
                    assert!(!is_red(buffer[(x, row.y)].fg), "{policy} in {:?}", row.text);
                    assert!(!is_red(buffer[(x, row.y)].bg), "{policy} in {:?}", row.text);
                }
            }
        }
    }
    Ok(())
}

#[test]
fn browsing_a_property_leads_with_its_policy_and_names_it_only_in_the_heading() -> Result {
    for size in SIZES {
        let (workspace, mut state) = on_intent()?;
        press(&mut state, &workspace, &[KeyCode::Down]);
        let terminal = draw(&mut state, size)?;
        let buffer = terminal.backend().buffer();
        let rows = inspector(&state, buffer);
        assert!(
            rows[0].text.starts_with(" Property constraints "),
            "{}",
            rows[0].text
        );
        assert_eq!(rows[1].text.trim(), "in team › feat");
        assert_eq!(
            labels(&rows),
            ["Policy", "Condition", "Description", "Rationale"]
        );
        let policy = rows
            .iter()
            .find(|row| row.text.starts_with(" Policy "))
            .ok_or("Policy")?;
        assert_eq!(policy.text.trim_end(), " Policy       conditional");
        for cell in policy.cells("conditional")? {
            assert_eq!(cell.fg, Color::White);
        }
        assert!(
            rows.iter()
                .any(|row| row.text.contains("Condition    has-limits"))
        );
        assert!(
            rows.iter()
                .any(|row| row.text.contains("Rationale    Record limits when"))
        );
        for row in &rows {
            for cell in row.cells(&row.text)? {
                assert_eq!(cell.bg, JET_BLACK, "a chip in {:?}", row.text);
            }
        }

        // A required property's policy reads as bright as any other value.
        press(&mut state, &workspace, &[KeyCode::Up]);
        let terminal = draw(&mut state, size)?;
        let buffer = terminal.backend().buffer();
        let rows = inspector(&state, buffer);
        assert_eq!(labels(&rows), ["Policy", "Description"]);
        let policy = rows
            .iter()
            .find(|row| row.text.starts_with(" Policy "))
            .ok_or("Policy")?;
        for cell in policy.cells("required")? {
            assert_eq!((cell.fg, cell.bg), (Color::White, JET_BLACK));
        }
    }
    Ok(())
}

#[test]
fn edit_lists_every_policy_with_the_current_one_bold_and_underlined() -> Result {
    for size in SIZES {
        let (workspace, mut state) = on_intent()?;
        press(&mut state, &workspace, &[KeyCode::Down, KeyCode::Char('e')]);
        assert!(state.taxonomies.editing());
        let terminal = draw(&mut state, size)?;
        let buffer = terminal.backend().buffer();
        let rows = inspector(&state, buffer);
        assert_eq!(
            labels(&rows),
            ["Name", "Description", "Policy", "Condition", "Rationale"]
        );
        // The options wrap within the value column of a narrow Inspector.
        let at = rows
            .iter()
            .position(|row| row.text.starts_with(" Policy "))
            .ok_or("Policy")?;
        let policy = rows[at..]
            .iter()
            .take_while(|row| row.text.starts_with(" Policy ") || row.text.starts_with("      "))
            .collect::<Vec<_>>();
        let options = policy
            .iter()
            .map(|row| {
                row.text
                    .chars()
                    .skip(14)
                    .collect::<String>()
                    .trim()
                    .to_owned()
            })
            .collect::<Vec<_>>()
            .join("  ");
        assert_eq!(options, POLICIES.join("  "));
        if size == (87, 40) {
            assert_eq!(policy.len(), 1);
        }
        for word in POLICIES {
            let row = policy
                .iter()
                .find(|row| row.text.contains(word))
                .ok_or(word)?;
            for cell in row.cells(word)? {
                assert_eq!(cell.bg, JET_BLACK);
                let current = word == "conditional";
                assert_eq!(cell.fg, if current { theme().text } else { DIM });
                assert_eq!(
                    cell.modifier
                        .contains(Modifier::BOLD | Modifier::UNDERLINED),
                    current
                );
            }
        }
    }
    Ok(())
}

#[test]
fn heading_badges_are_plain_and_built_ins_end_with_the_fork_hint() -> Result {
    for size in SIZES {
        let (workspace, mut state) = fixture()?;
        for (path, badge, color, last) in [
            (
                &["conventional"][..],
                Some("◇ read-only"),
                DIM,
                Some("f fork to customize"),
            ),
            (
                &["conventional", "feat"][..],
                Some("◇ read-only"),
                DIM,
                Some("f fork to customize"),
            ),
            (&["team"][..], None, DIM, None),
            (
                &["draft-ideas"][..],
                Some("▲ invalid"),
                theme().blocks,
                Some("Add a type before this taxonomy can be included in a project."),
            ),
        ] {
            select(&mut state, path)?;
            let terminal = draw(&mut state, size)?;
            let buffer = terminal.backend().buffer();
            let rows = inspector(&state, buffer);
            let heading = &rows[0];
            match badge {
                Some(badge) => {
                    assert!(heading.text.trim_end().ends_with(badge), "{}", heading.text);
                    for cell in heading.cells(badge)? {
                        assert_eq!((cell.fg, cell.bg), (color, JET_BLACK));
                    }
                }
                None => assert_eq!(heading.text.trim(), format!("Taxonomy {}", path[0])),
            }
            // A type can run past the Inspector; a page down shows its end.
            press(&mut state, &workspace, &[KeyCode::PageDown]);
            let terminal = draw(&mut state, size)?;
            let rows = inspector(&state, terminal.backend().buffer());
            let text = rows
                .iter()
                .map(|row| row.text.trim())
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            assert!(!text.contains("new blank"), "{text}");
            match last {
                Some(last) => assert!(text.ends_with(last), "{path:?}: {text}"),
                None => assert!(!text.contains("fork"), "{text}"),
            }
            if last == Some("f fork to customize") {
                let hint = rows
                    .iter()
                    .find(|row| row.text.trim() == "f fork to customize")
                    .ok_or("hint")?;
                let key = hint.cells("f ")?[0];
                assert_eq!(key.fg, theme().text);
                assert!(key.modifier.contains(Modifier::BOLD));
                for cell in hint.cells("fork to customize")? {
                    assert_eq!(cell.fg, DIM);
                }
            }
        }
    }
    Ok(())
}
