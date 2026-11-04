use std::path::Path;

use gitserious_app::ProjectConfiguration;
use gitserious_core::Taxonomy;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use unicode_segmentation::UnicodeSegmentation;

use crate::theme::theme;

/// The narrowest pane that still fits a taxonomy row or a preview line.
const MINIMUM_PANE_WIDTH: u16 = 44;
const TYPE_COLUMN_WIDTH: usize = 13;

/// One taxonomy the new project could start with.
struct Candidate {
    taxonomy: Taxonomy,
    custom: bool,
    included: bool,
}

/// What a key asks of the shell.
pub(super) enum EditorOutcome {
    Stay,
    Back,
    Initialize(ProjectConfiguration),
}

/// Chooses the taxonomies, their order, and the default for `gitserious.toml`.
///
/// The first included row is the default, the first taxonomy a commit considers.
pub(super) struct InitEditor {
    rows: Vec<Candidate>,
    /// A taxonomy row, or `rows.len()` for the Initialize row.
    cursor: usize,
    /// The taxonomy the preview shows, kept while the cursor is on the Initialize row.
    previewed: usize,
}

impl InitEditor {
    /// Built-ins start included in their order; custom taxonomies start excluded after them.
    pub(super) fn new(built_ins: &[Taxonomy], custom: &[Taxonomy]) -> Self {
        let rows = built_ins
            .iter()
            .map(|taxonomy| (taxonomy, false))
            .chain(custom.iter().map(|taxonomy| (taxonomy, true)))
            .map(|(taxonomy, custom)| Candidate {
                taxonomy: taxonomy.clone(),
                custom,
                included: !custom,
            })
            .collect();
        Self {
            rows,
            cursor: 0,
            previewed: 0,
        }
    }

    fn default_row(&self) -> Option<usize> {
        self.rows.iter().position(|row| row.included)
    }

    fn on_initialize_row(&self) -> bool {
        self.cursor == self.rows.len()
    }

    pub(super) fn key(&mut self, key: KeyEvent) -> EditorOutcome {
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        match key.code {
            KeyCode::Esc => return EditorOutcome::Back,
            KeyCode::Up | KeyCode::Down if alt => {
                let target = if key.code == KeyCode::Up {
                    self.cursor.checked_sub(1)
                } else {
                    Some(self.cursor + 1).filter(|next| *next < self.rows.len())
                };
                if let Some(target) = target.filter(|_| !self.on_initialize_row()) {
                    self.rows.swap(self.cursor, target);
                    self.cursor = target;
                }
            }
            KeyCode::Up => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Down => self.cursor = (self.cursor + 1).min(self.rows.len()),
            KeyCode::Char(' ') if !self.on_initialize_row() => {
                let included = self.rows.iter().filter(|row| row.included).count();
                let row = &mut self.rows[self.cursor];
                if !row.included || included > 1 {
                    row.included = !row.included;
                }
            }
            KeyCode::Enter if self.on_initialize_row() => {
                let chosen = self
                    .rows
                    .iter()
                    .filter(|row| row.included)
                    .map(|row| row.taxonomy.clone())
                    .collect();
                if let Ok(configuration) = ProjectConfiguration::with_first_as_default(chosen) {
                    return EditorOutcome::Initialize(configuration);
                }
            }
            KeyCode::Enter => {
                let mut row = self.rows.remove(self.cursor);
                row.included = true;
                self.rows.insert(0, row);
                self.cursor = 0;
            }
            _ => {}
        }
        if !self.on_initialize_row() {
            self.previewed = self.cursor;
        }
        EditorOutcome::Stay
    }

    pub(super) fn hints(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            ("esc", "back"),
            ("space", "include"),
            if self.on_initialize_row() {
                ("enter", "initialize")
            } else {
                ("enter", "default")
            },
            ("alt+↑/↓", "order"),
        ]
    }

    /// Side by side on wide terminals, the preview under the list on tall ones.
    pub(super) fn render(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        root: &Path,
        error: Option<&str>,
    ) {
        let wide = u32::from(area.width) * 2 >= u32::from(area.height) * 5
            && area.width > MINIMUM_PANE_WIDTH * 2;
        let separator = Style::new().fg(theme().dim);
        if wide {
            let [list, preview] =
                Layout::horizontal([Constraint::Percentage(50), Constraint::Fill(1)]).areas(area);
            render_wrapped(frame, list, self.list_lines(root, error, list));
            let block = Block::new().borders(Borders::LEFT).border_style(separator);
            let inner = block.inner(preview);
            frame.render_widget(block, preview);
            self.render_preview(frame, inner, true);
        } else {
            let (head, tail) = self.frame_lines(root, error, area.width);
            let fixed = head.len() + tail.len();
            // A long Library scrolls inside the list so the preview keeps half the height.
            let height = (fixed + self.rows.len()).min(usize::from(area.height / 2).max(fixed + 1));
            let [list, rule, preview] = Layout::vertical([
                Constraint::Length(u16::try_from(height).unwrap_or(u16::MAX)),
                Constraint::Length(1),
                Constraint::Fill(1),
            ])
            .areas(area);
            render_wrapped(frame, list, self.list_lines(root, error, list));
            frame.render_widget(
                Block::new().borders(Borders::TOP).border_style(separator),
                rule,
            );
            self.render_preview(frame, preview, false);
        }
    }

    /// The title above the taxonomy rows and the Initialize row and error below them.
    fn frame_lines(
        &self,
        root: &Path,
        error: Option<&str>,
        width: u16,
    ) -> (Vec<Line<'static>>, Vec<Line<'static>>) {
        let text = Style::new().fg(theme().text);
        let dim = Style::new().fg(theme().dim);
        let name = root.file_name().map_or_else(
            || root.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        let width = usize::from(width);
        let mut head = vec![
            Line::default(),
            Line::styled(
                format!(" Initialize {name}"),
                text.add_modifier(Modifier::BOLD),
            ),
        ];
        head.extend(inset(&root.display().to_string(), dim, width));
        head.extend([Line::default(), Line::styled(" Taxonomies", dim)]);
        let initialize = spread(
            vec![Span::styled(" Initialize repository", text)],
            Span::styled("enter ", dim),
            width,
            self.on_initialize_row(),
        );
        let mut tail = vec![
            Line::default(),
            if self.on_initialize_row() {
                initialize
            } else {
                initialize.style(Style::new().bg(theme().band))
            },
        ];
        if let Some(error) = error {
            tail.push(Line::default());
            tail.extend(inset(
                &format!("✗ {error}"),
                Style::new().fg(theme().blocks),
                width,
            ));
        }
        (head, tail)
    }

    /// The title, the taxonomy rows that fit `area` around the selection, and the Initialize row.
    fn list_lines(&self, root: &Path, error: Option<&str>, area: Rect) -> Vec<Line<'static>> {
        let text = Style::new().fg(theme().text);
        let dim = Style::new().fg(theme().dim);
        let width = usize::from(area.width);
        let (mut lines, tail) = self.frame_lines(root, error, area.width);
        let visible = usize::from(area.height)
            .saturating_sub(lines.len() + tail.len())
            .max(1);
        let focus = self.cursor.min(self.rows.len().saturating_sub(1));
        let offset = (focus + 1).saturating_sub(visible);
        let default = self.default_row();
        let mut number = 0;
        for (index, row) in self.rows.iter().enumerate() {
            let selected = index == self.cursor;
            let (mark, label) = if row.included {
                number += 1;
                ("✓", number.to_string())
            } else {
                ("○", "–".to_owned())
            };
            let tag = if Some(index) == default {
                "  default"
            } else if row.custom {
                "  custom"
            } else {
                ""
            };
            if (offset..offset + visible).contains(&index) {
                let prefix = format!(" {mark} {label}  ");
                let right = format!("{} types ", row.taxonomy.commit_types().len());
                // Each row keeps one line, so the window arithmetic above stays exact.
                let budget = width.saturating_sub(prefix.len() + tag.len() + right.len() + 1);
                let left = vec![
                    Span::raw(prefix),
                    Span::styled(
                        truncate(&row.taxonomy.versioned().to_string(), budget),
                        if row.included { text } else { dim },
                    ),
                    Span::styled(tag, dim),
                ];
                lines.push(spread(left, Span::styled(right, dim), width, selected));
            }
        }
        lines.extend(tail);
        lines
    }

    /// `top_gap` aligns the title with the list's title when the panes sit side by side.
    fn render_preview(&self, frame: &mut Frame<'_>, area: Rect, top_gap: bool) {
        let Some(row) = self.rows.get(self.previewed) else {
            return;
        };
        let taxonomy = &row.taxonomy;
        let text = Style::new().fg(theme().text);
        let dim = Style::new().fg(theme().dim);
        let [area] = Layout::horizontal([Constraint::Fill(1)])
            .horizontal_margin(1)
            .areas(area);
        let width = usize::from(area.width);
        let mut lines = Vec::new();
        if top_gap {
            lines.push(Line::default());
        }
        lines.extend([
            Line::from(vec![
                Span::styled("Taxonomy ", dim),
                Span::styled(taxonomy.id().to_string(), text.add_modifier(Modifier::BOLD)),
            ]),
            Line::default(),
            Line::styled(taxonomy.description().to_string(), dim),
            Line::default(),
        ]);
        for commit_type in taxonomy.commit_types() {
            let properties = commit_type
                .properties()
                .iter()
                .map(|property| property.key().to_string())
                .collect::<Vec<_>>()
                .join(" · ");
            let id = commit_type.id().to_string();
            let budget = width.saturating_sub(TYPE_COLUMN_WIDTH.max(id.len() + 1));
            lines.push(Line::from(vec![
                Span::styled(format!("{id:<TYPE_COLUMN_WIDTH$}"), text),
                Span::styled(truncate(&properties, budget), dim),
            ]));
        }
        render_wrapped(frame, area, lines);
    }
}

/// `text` cut into lines that each keep the list's one-column inset.
fn inset(text: &str, style: Style, width: usize) -> Vec<Line<'static>> {
    let graphemes = text.graphemes(true).collect::<Vec<_>>();
    graphemes
        .chunks(width.saturating_sub(2).max(1))
        .map(|chunk| Line::styled(format!(" {}", chunk.concat()), style))
        .collect()
}

fn render_wrapped(frame: &mut Frame<'_>, area: Rect, lines: Vec<Line<'static>>) {
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

/// `left` and `right` at the edges of a `width`-wide row, on the cursor color when `selected`.
fn spread(
    mut left: Vec<Span<'static>>,
    right: Span<'static>,
    width: usize,
    selected: bool,
) -> Line<'static> {
    let used = left.iter().map(Span::width).sum::<usize>() + right.width();
    left.push(Span::raw(" ".repeat(width.saturating_sub(used))));
    left.push(right);
    if selected {
        let cursor = Style::new()
            .fg(theme().ground)
            .bg(theme().cursor)
            .add_modifier(Modifier::BOLD);
        Line::from(
            left.into_iter()
                .map(|span| span.style(cursor))
                .collect::<Vec<_>>(),
        )
    } else {
        Line::from(left)
    }
}

/// Cuts `text` to `budget` columns, ending with `…` when anything is cut.
fn truncate(text: &str, budget: usize) -> String {
    if text.graphemes(true).count() <= budget {
        return text.to_owned();
    }
    let kept = text
        .graphemes(true)
        .take(budget.saturating_sub(1))
        .collect::<String>();
    format!("{kept}…")
}

#[cfg(test)]
mod tests {
    use gitserious_core::{Description, TaxonomyId, TaxonomyVersion, built_in_taxonomies};
    use ratatui::backend::TestBackend;
    use ratatui::{Terminal, layout::Position};

    use super::*;

    fn team_changes() -> Result<Taxonomy, Box<dyn std::error::Error>> {
        Ok(Taxonomy::new(
            TaxonomyId::new("team-changes")?,
            TaxonomyVersion::new(3)?,
            Description::new("Changes our team tracks.")?,
            None,
            built_in_taxonomies()[0].commit_types()[..3].to_vec(),
        )?)
    }

    fn press(editor: &mut InitEditor, code: KeyCode) -> EditorOutcome {
        editor.key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn alt(editor: &mut InitEditor, code: KeyCode) {
        editor.key(KeyEvent::new(code, KeyModifiers::ALT));
    }

    fn initialize(editor: &mut InitEditor) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        for _ in 0..editor.rows.len() {
            press(editor, KeyCode::Down);
        }
        let EditorOutcome::Initialize(configuration) = press(editor, KeyCode::Enter) else {
            return Err("the Initialize row did not initialize".into());
        };
        let order = configuration
            .taxonomies()
            .iter()
            .map(|taxonomy| taxonomy.versioned().to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            configuration.default_taxonomy(),
            configuration.taxonomies()[0].id(),
            "the first taxonomy is the default"
        );
        Ok(order)
    }

    fn screen(
        editor: &InitEditor,
        width: u16,
        height: u16,
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let mut terminal = Terminal::new(TestBackend::new(width, height))?;
        terminal.draw(|frame| editor.render(frame, frame.area(), Path::new("/src/fresh"), None))?;
        let buffer = terminal.backend().buffer();
        Ok((0..height)
            .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
            .collect())
    }

    #[test]
    fn built_ins_start_included_with_conventional_first_and_library_taxonomies_excluded()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut editor = InitEditor::new(built_in_taxonomies(), &[team_changes()?]);
        let rows = screen(&editor, 87, 40)?;
        assert!(
            rows[5].starts_with(" ✓ 1  conventional-v1  default"),
            "{}",
            rows[5]
        );
        assert!(rows[5].ends_with("11 types "));
        assert!(rows[6].starts_with(" ✓ 2  research-v1"));
        assert!(rows[7].starts_with(" ✓ 3  infra-ops-v1"));
        assert!(rows[8].starts_with(" ○ –  team-changes-v3  custom"));
        assert!(rows[8].ends_with("3 types "));
        assert_eq!(
            initialize(&mut editor)?,
            ["conventional-v1", "research-v1", "infra-ops-v1"]
        );
        Ok(())
    }

    #[test]
    fn space_includes_enter_makes_the_default_and_alt_arrows_reorder()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut editor = InitEditor::new(built_in_taxonomies(), &[team_changes()?]);
        for _ in 0..3 {
            press(&mut editor, KeyCode::Down);
        }
        press(&mut editor, KeyCode::Char(' '));
        alt(&mut editor, KeyCode::Up);
        let rows = screen(&editor, 87, 40)?;
        assert!(
            rows[7].starts_with(" ✓ 3  team-changes-v3  custom"),
            "{}",
            rows[7]
        );
        assert!(rows[8].starts_with(" ✓ 4  infra-ops-v1"));

        press(&mut editor, KeyCode::Up);
        press(&mut editor, KeyCode::Char(' '));
        press(&mut editor, KeyCode::Down);
        press(&mut editor, KeyCode::Enter);
        let rows = screen(&editor, 87, 40)?;
        assert!(
            rows[5].starts_with(" ✓ 1  team-changes-v3  default"),
            "{}",
            rows[5]
        );
        assert!(rows[7].starts_with(" ○ –  research-v1"));
        assert_eq!(
            initialize(&mut editor)?,
            ["team-changes-v3", "conventional-v1", "infra-ops-v1"]
        );
        Ok(())
    }

    #[test]
    fn a_long_library_scrolls_and_keeps_the_selection_and_initialize_row_visible()
    -> Result<(), Box<dyn std::error::Error>> {
        let template = team_changes()?;
        let library = (1..=30)
            .map(|index| {
                let padding = "-fork".repeat(index % 3 * 12);
                Ok(Taxonomy::new(
                    TaxonomyId::new(format!("team{padding}-{index:02}"))?,
                    TaxonomyVersion::V1,
                    template.description().clone(),
                    None,
                    template.commit_types().to_vec(),
                )?)
            })
            .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
        let mut editor = InitEditor::new(built_in_taxonomies(), &library);
        for _ in 0..32 {
            press(&mut editor, KeyCode::Down);
        }
        for (width, height) in [(87, 40), (100, 30)] {
            let rows = screen(&editor, width, height)?;
            let selected = rows
                .iter()
                .position(|row| row.contains("team-30-v1"))
                .ok_or("the selected row scrolled out of view")?;
            let initialize = rows
                .iter()
                .position(|row| row.contains("Initialize repository"))
                .ok_or("the Initialize row scrolled out of view")?;
            assert_eq!(initialize, selected + 2, "{width}x{height}");
            assert!(
                rows.iter()
                    .any(|row| row.contains('…') && row.contains(" 3 types")),
                "long names are cut to keep one line per row"
            );
            assert!(
                !rows.iter().any(|row| row.contains("conventional-v1")),
                "the top scrolled away"
            );
            assert!(
                rows.iter().any(|row| row.contains("Taxonomy team-30")),
                "{width}x{height}"
            );
        }
        press(&mut editor, KeyCode::Down);
        let rows = screen(&editor, 87, 40)?;
        assert!(
            rows.iter().any(|row| row.contains("team-30-v1")),
            "the last row stays above Initialize"
        );
        Ok(())
    }

    #[test]
    fn a_long_path_wraps_inside_the_inset() -> Result<(), Box<dyn std::error::Error>> {
        let editor = InitEditor::new(built_in_taxonomies(), &[]);
        let root = Path::new("/").join("a".repeat(120)).join("fresh");
        let mut terminal = Terminal::new(TestBackend::new(87, 40))?;
        terminal.draw(|frame| editor.render(frame, frame.area(), &root, None))?;
        let buffer = terminal.backend().buffer();
        let rows: Vec<String> = (0..40)
            .map(|y| (0..87).map(|x| buffer[(x, y)].symbol()).collect())
            .collect();
        let path_rows = rows
            .iter()
            .filter(|row| row.contains("aaaa"))
            .collect::<Vec<_>>();
        assert_eq!(path_rows.len(), 2, "{rows:#?}");
        assert!(
            path_rows
                .iter()
                .all(|row| row.starts_with(" /") || row.starts_with(" a"))
        );
        assert!(rows.iter().any(|row| row.contains("Initialize repository")));
        Ok(())
    }

    #[test]
    fn the_last_included_taxonomy_cannot_be_excluded_and_esc_writes_nothing()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut editor = InitEditor::new(&built_in_taxonomies()[..1], &[]);
        press(&mut editor, KeyCode::Char(' '));
        assert!(screen(&editor, 87, 40)?[5].starts_with(" ✓ 1  conventional-v1"));
        assert!(matches!(
            press(&mut editor, KeyCode::Esc),
            EditorOutcome::Back
        ));
        Ok(())
    }

    #[test]
    fn the_preview_sits_beside_the_list_on_wide_terminals_and_under_it_on_tall_ones()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut editor = InitEditor::new(built_in_taxonomies(), &[]);
        for (width, height, wide) in [
            (87, 40, false),
            (100, 30, true),
            (120, 30, true),
            (140, 60, false),
        ] {
            let rows = screen(&editor, width, height)?;
            let title = rows
                .iter()
                .position(|row| row.contains("Taxonomy conventional"))
                .ok_or("missing preview")?;
            if wide {
                assert_eq!(title, 1, "{width}x{height}");
                assert!(rows[title].starts_with(" Initialize fresh"));
                assert!(rows.iter().all(|row| row.contains('│')), "{width}x{height}");
            } else {
                assert_eq!(rows[title - 1].trim_matches('─'), "", "{width}x{height}");
                assert!(rows[title - 2].contains("Initialize repository"));
            }
        }
        press(&mut editor, KeyCode::Down);
        assert!(
            screen(&editor, 100, 30)?
                .iter()
                .any(|row| row.contains("Taxonomy research"))
        );
        for _ in 0..3 {
            press(&mut editor, KeyCode::Down);
        }
        let rows = screen(&editor, 100, 30)?;
        assert!(
            rows.iter().any(|row| row.contains("Taxonomy infra-ops")),
            "the Initialize row keeps the last preview"
        );
        assert!(
            rows.iter().any(|row| row.trim_end().ends_with('…')),
            "long properties are cut"
        );
        let initialize = rows
            .iter()
            .position(|row| row.contains("Initialize repository"))
            .ok_or("missing Initialize row")?;
        let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
        terminal.draw(|frame| editor.render(frame, frame.area(), Path::new("/src/fresh"), None))?;
        let cell = terminal
            .backend()
            .buffer()
            .cell(Position::new(1, u16::try_from(initialize)?))
            .ok_or("missing cell")?;
        assert_eq!(cell.bg, theme().cursor);
        Ok(())
    }
}
