use crate::library_lookup::LibraryLookup;
use gitserious_app::DraftField;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Cell;

use super::tests::Workspace;
use super::*;

pub(super) type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

const DIM: Color = theme().dim;
const SIZES: [(u16, u16); 3] = [(87, 40), (125, 51), (100, 30)];

/// Keys that take focus from the outline, and how to tell that they did.
type FocusAway = (&'static [KeyCode], fn(&State) -> bool);

pub(super) fn press(state: &mut State, workspace: &Workspace, keys: &[KeyCode]) {
    for &code in keys {
        assert!(!state.handle_event(
            Event::Key(KeyEvent::new(code, KeyModifiers::NONE)),
            workspace
        ));
    }
}

/// The Library with the built-ins, the custom `team`, and `draft-ideas`, saved with no types,
/// with conventional open under the cursor.
pub(super) fn fixture() -> Result<(Workspace, State)> {
    let workspace = super::copy_tests::team()?;
    let mut state = State::new(&workspace);
    press(
        &mut state,
        &workspace,
        &[KeyCode::Char('2'), KeyCode::Char('n')],
    );
    state.handle_event(Event::Paste("Draft ideas".into()), &workspace);
    press(&mut state, &workspace, &[KeyCode::Enter]);
    state.handle_event(
        Event::Paste("Ideas that have not earned a type yet.".into()),
        &workspace,
    );
    press(&mut state, &workspace, &[KeyCode::Esc]);
    select(&mut state, &["conventional"])?;
    press(&mut state, &workspace, &[KeyCode::Enter]);
    Ok((workspace, state))
}

pub(super) fn select(state: &mut State, path: &[&str]) -> Result {
    let id = state
        .library()
        .and_then(|library| library.find_path(path))
        .ok_or("node missing")?;
    state.taxonomies.select(id);
    Ok(())
}

/// The drawn frame and the outline's rows, each with its row on screen.
struct Frame {
    terminal: Terminal<TestBackend>,
    outline: Rect,
}

impl Frame {
    fn draw(state: &mut State, (width, height): (u16, u16)) -> Result<Self> {
        let mut terminal = Terminal::new(TestBackend::new(width, height))?;
        terminal.draw(|frame| state.render(frame))?;
        assert_eq!(terminal.backend().buffer()[(0, 0)].bg, theme().ground);
        let outline = state.taxonomies.observe(state.library()).outline_area;
        Ok(Self { terminal, outline })
    }

    fn line(&self, y: u16) -> String {
        let buffer = self.terminal.backend().buffer();
        (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect()
    }

    fn rows(&self) -> Vec<String> {
        let buffer = self.terminal.backend().buffer();
        (self.outline.y..self.outline.bottom())
            .map(|y| {
                (self.outline.x..self.outline.right())
                    .map(|x| buffer[(x, y)].symbol())
                    .collect()
            })
            .collect()
    }

    /// The outline row holding `text`, as its screen row and its text without trailing blanks.
    fn row(&self, text: &str) -> Result<(u16, String)> {
        let rows = self.rows();
        let index = rows
            .iter()
            .position(|row| row.contains(text))
            .ok_or_else(|| format!("no row holds {text:?}:\n{}", rows.join("\n")))?;
        Ok((
            self.outline.y + u16::try_from(index)?,
            rows[index].trim_end().to_owned(),
        ))
    }

    /// The cell where `text` starts in the outline row that holds `marker`.
    fn cell(&self, marker: &str, text: &str) -> Result<&Cell> {
        let (y, row) = self.row(marker)?;
        let start = row
            .find(text)
            .ok_or_else(|| format!("{text:?} not in {row:?}"))?;
        let x = self.outline.x + u16::try_from(row[..start].chars().count())?;
        Ok(&self.terminal.backend().buffer()[(x, y)])
    }

    /// Every cell of the outline row that holds `marker`, edge to edge.
    fn row_cells(&self, marker: &str) -> Result<Vec<&Cell>> {
        let (y, _) = self.row(marker)?;
        let buffer = self.terminal.backend().buffer();
        Ok((self.outline.x..self.outline.right())
            .map(|x| &buffer[(x, y)])
            .collect())
    }
}

fn colors(cell: &Cell) -> (Color, Color) {
    (cell.fg, cell.bg)
}

#[test]
fn taxonomy_rows_name_their_origin_type_count_and_version() -> Result {
    for size in SIZES {
        let (_, mut state) = fixture()?;
        let frame = Frame::draw(&mut state, size)?;
        for (marker, label, end) in [
            ("▾ conventional ", "built-in", "11 types  v1"),
            ("▸ research ", "built-in", "10 types  v1"),
            ("▸ infra-ops ", "built-in", "10 types  v1"),
            ("▸ team ", "custom", "3 types"),
            ("▸ draft-ideas ", "custom", "▲ invalid"),
        ] {
            let (_, row) = frame.row(marker)?;
            assert!(row.starts_with(&format!(" {marker}")), "{size:?}: {row:?}");
            let status_at = row.rfind(end).ok_or_else(|| format!("{size:?}: {row:?}"))?;
            assert!(
                row[..status_at].trim_end().ends_with(label),
                "{size:?}: {row:?}"
            );
            let name = marker.trim_start_matches(['▾', '▸', ' ']);
            let name_at = row.find(name).ok_or_else(|| format!("{size:?}: {row:?}"))?;
            assert!(
                row[name_at + name.len()..status_at]
                    .trim()
                    .starts_with(label),
                "{size:?}: {row:?}"
            );
        }
        let custom = frame.cell("▸ team ", "custom")?;
        assert_eq!(colors(custom), (theme().text, theme().ground));
        assert!(custom.modifier.contains(Modifier::BOLD));
        assert_eq!(frame.cell("▸ research ", "v1")?.fg, DIM);
        assert_eq!(frame.cell("▸ research ", "10 types")?.fg, DIM);
        let invalid = frame.cell("▸ draft-ideas ", "▲ invalid")?;
        assert_eq!(colors(invalid), (theme().blocks, theme().ground));
        let (_, last) = frame.row("+ new taxonomy")?;
        assert_eq!(
            frame
                .rows()
                .iter()
                .map(|row| row.trim_end())
                .rfind(|row| !row.is_empty()),
            Some(last.as_str())
        );
        assert_eq!(last, " + new taxonomy");
        assert_eq!(frame.cell("+ new taxonomy", "+ new")?.fg, DIM);
    }
    Ok(())
}

#[test]
fn add_rows_are_dim() -> Result {
    for size in SIZES {
        let (workspace, mut state) = fixture()?;
        select(&mut state, &["team"])?;
        press(
            &mut state,
            &workspace,
            &[KeyCode::Enter, KeyCode::Down, KeyCode::Enter],
        );
        let frame = Frame::draw(&mut state, size)?;
        assert!(
            frame
                .row("│  └─ + add property")?
                .1
                .ends_with("+ add property")
        );
        assert!(frame.row(" └─ + add type")?.1.ends_with("+ add type"));
        for label in ["+ add property", "+ add type"] {
            assert_eq!(colors(frame.cell(label, label)?), (DIM, theme().ground));
            assert_eq!(frame.cell(label, "└─")?.fg, theme().dim);
        }
    }
    Ok(())
}

#[test]
fn enter_on_the_last_row_creates_a_taxonomy() -> Result {
    for size in SIZES {
        let (workspace, mut state) = fixture()?;
        press(&mut state, &workspace, &[KeyCode::Down; 30]);
        assert_eq!(
            state.taxonomies.observe(state.library()).cursor_path,
            ["+ new taxonomy"]
        );
        let frame = Frame::draw(&mut state, size)?;
        assert!(
            frame
                .row_cells("+ new taxonomy")?
                .iter()
                .all(|cell| colors(cell) == (theme().ground, theme().cursor))
        );
        let footer = super::tests::footer_text(&frame.line(size.1 - 1));
        assert!(footer.contains("enter new"), "{footer}");

        press(&mut state, &workspace, &[KeyCode::Enter]);
        assert!(state.taxonomies.editing());
        state.handle_event(Event::Paste("Fresh ideas".into()), &workspace);
        let frame = Frame::draw(&mut state, size)?;
        let rows = frame
            .rows()
            .into_iter()
            .map(|row| row.trim_end().to_owned())
            .collect::<Vec<_>>();
        let at = rows
            .iter()
            .position(|row| row.starts_with("▌▾ Fresh ideas") && row.contains("custom"))
            .ok_or_else(|| rows.join("\n"))?;
        assert_eq!(rows[at + 1], " └─ + add type");
        assert_eq!(rows[at + 2], " + new taxonomy");
        let text = frame
            .terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(Cell::symbol)
            .collect::<String>();
        assert!(text.contains("Taxonomy Fresh ideas"), "{text}");
    }
    Ok(())
}

#[test]
fn the_cursor_is_a_bone_bar_with_focus_and_a_raised_row_without() -> Result {
    for size in SIZES {
        let (workspace, mut state) = fixture()?;
        let frame = Frame::draw(&mut state, size)?;
        let bar = frame.row_cells("▾ conventional ")?;
        assert!(bar.iter().all(|cell| cell.bg == theme().cursor), "{size:?}");
        for text in ["conventional", "built-in", "v1", "11 types"] {
            let cell = frame.cell("▾ conventional ", text)?;
            assert_eq!(colors(cell), (theme().ground, theme().cursor));
            assert!(cell.modifier.contains(Modifier::BOLD));
        }
        // The rail, EDIT in the Inspector, and a dialog each take focus from the outline.
        press(&mut state, &workspace, &[KeyCode::Enter]);
        select(&mut state, &["team"])?;
        let away: [FocusAway; 3] = [
            (&[KeyCode::Left], |state| {
                state.home_focus == HomeFocus::Menu
            }),
            (&[KeyCode::Right, KeyCode::Char('e')], |state| {
                state.taxonomies.editing()
            }),
            (&[KeyCode::Esc, KeyCode::Char('d')], |state| {
                matches!(state.screen, Screen::ConfirmRemove(_))
            }),
        ];
        for (keys, moved) in away {
            press(&mut state, &workspace, keys);
            assert!(moved(&state), "{keys:?}");
            let frame = Frame::draw(&mut state, size)?;
            let row = frame.row_cells("▸ team ")?;
            assert!(row.iter().all(|cell| cell.bg == theme().band), "{keys:?}");
            assert_eq!(frame.cell("▸ team ", "team")?.fg, theme().text);
            assert_eq!(frame.cell("▸ team ", "custom")?.fg, theme().text);
            assert_eq!(frame.cell("▸ team ", "3 types")?.fg, theme().text);
        }
    }
    Ok(())
}

#[test]
fn narrow_rows_drop_the_count_then_the_version_before_the_name() -> Result {
    for (name, end) in [
        ("an-exceptionally-long-taxonomy", " v1"),
        ("an-exceptionally-long-taxonomy-names", "built-in"),
        ("an-exceptionally-long-taxonomy-names-too", "built-in"),
    ] {
        let (_, mut state) = fixture()?;
        let library = state
            .global
            .as_mut()
            .and_then(ConfigurationSession::library_mut)
            .ok_or("Library")?;
        let draft = library.find_path(&["draft-ideas"]).ok_or("draft-ideas")?;
        library.edit(&draft, DraftEdit::Text(DraftField::Name, name))?;
        let frame = Frame::draw(&mut state, (MINIMUM_WIDTH, 40))?;
        let (_, row) = frame.row("▾ conventional ")?;
        assert!(row.ends_with(end), "{name}: {row:?}");
        assert!(!row.contains("types"), "{name}: {row:?}");
        let (_, row) = frame.row("▸ an-exceptionally")?;
        assert!(row.ends_with("custom"), "{name}: {row:?}");
        assert_eq!(row.contains('…'), name.len() == 40, "{name}: {row:?}");
    }
    Ok(())
}
