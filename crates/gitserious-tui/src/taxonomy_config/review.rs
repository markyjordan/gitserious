//! The review: what apply would write, every problem that keeps it from applying, and apply.

use gitserious_app::{ConfigurationDestination, ConfigurationWorkspace, LibraryDraft, Problem};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph, Wrap};

use super::naming::problem_sentence;
use super::{Notice, Screen, State, hanging_wrap};
use crate::theme::frame_style;

const PROBLEM: Style = Style::new().fg(Color::Red);

impl State {
    pub(super) fn open_review(&mut self) {
        let destination = self.active_scope;
        let should_review = self
            .session(destination)
            .is_some_and(|session| session.is_dirty() || session.update_available());
        if should_review {
            if let Some(error) = self
                .session(destination)
                .and_then(|session| session.review().err())
            {
                self.status = Some(Notice::error(error));
            } else {
                self.scroll = 0;
                self.screen = Screen::Review(destination);
            }
        } else {
            self.status = Some(Notice::info(
                "No changes or taxonomy updates to apply.".into(),
            ));
        }
    }

    pub(super) fn review_key(
        &mut self,
        key: KeyEvent,
        destination: ConfigurationDestination,
        workspace: &dyn ConfigurationWorkspace,
    ) -> bool {
        match key.code {
            KeyCode::Enter if self.refuse_apply(destination) => {}
            KeyCode::Enter => {
                let Some(session) = self.session(destination).cloned() else {
                    return false;
                };
                match workspace.save(&session) {
                    Ok(saved) => {
                        self.replace_session(destination, Ok(saved));
                        if destination == ConfigurationDestination::Global {
                            self.replace_session(
                                ConfigurationDestination::Project,
                                workspace.load(ConfigurationDestination::Project),
                            );
                        }
                        self.screen = Screen::Home;
                        self.status = Some(Notice::info("Configuration applied.".into()));
                    }
                    Err(error) => {
                        self.screen = Screen::Home;
                        self.status = Some(Notice::error(error));
                    }
                }
            }
            KeyCode::Esc => self.screen = Screen::Home,
            KeyCode::Up => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::Down => self.scroll = self.scroll.saturating_add(1),
            _ => {}
        }
        false
    }

    /// Says why `destination` cannot apply while its review lists problems. Returns whether it
    /// refused.
    pub(super) fn refuse_apply(&mut self, destination: ConfigurationDestination) -> bool {
        let count = self
            .session(destination)
            .and_then(|session| session.review().ok())
            .map_or(0, |review| review.problems.len());
        if count > 0 {
            let problems = if count == 1 {
                "1 problem is".to_owned()
            } else {
                format!("{count} problems are")
            };
            self.status = Some(Notice::error(format!(
                "Apply is disabled until {problems} fixed."
            )));
        }
        count > 0
    }

    /// Draws the review and returns its footer hints, which leave out apply while it is
    /// disabled.
    pub(super) fn render_review(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        destination: ConfigurationDestination,
    ) -> Vec<(&'static str, &'static str)> {
        let width = usize::from(area.width.saturating_sub(2));
        let review = self
            .session(destination)
            .and_then(|session| session.review().ok());
        let (lines, blocked) = match (review, self.library()) {
            (Some(review), library) => {
                let mut lines = library
                    .map(|library| problem_rows(library, &review.problems, width))
                    .unwrap_or_default();
                lines.extend(hanging_wrap(&review.changes, width));
                (lines, !review.problems.is_empty())
            }
            (None, _) => (
                hanging_wrap("Configuration cannot be reviewed.", width),
                false,
            ),
        };
        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
        let viewport_height = area.height.saturating_sub(2);
        let total_rows =
            u16::try_from(paragraph.line_count(area.width.saturating_sub(2))).unwrap_or(u16::MAX);
        self.scroll = self.scroll.min(total_rows.saturating_sub(viewport_height));
        let title = if blocked {
            "Review changes — apply disabled"
        } else {
            "Review changes — Enter to apply"
        };
        frame.render_widget(
            paragraph
                .scroll((self.scroll, 0))
                .block(Block::bordered().border_style(frame_style()).title(title)),
            area,
        );
        let mut hints = vec![("esc", "back"), ("↑/↓", "scroll")];
        if !blocked {
            hints.insert(0, ("enter", "apply"));
        }
        hints
    }
}

/// Every problem under the path of the node it sits on, then a blank row; nothing without
/// problems.
fn problem_rows(library: &LibraryDraft, problems: &[Problem], width: usize) -> Vec<Line<'static>> {
    if problems.is_empty() {
        return Vec::new();
    }
    let mut lines = vec![Line::styled(
        "Problems",
        PROBLEM.add_modifier(Modifier::BOLD),
    )];
    let mut shown = None;
    for problem in problems {
        let path = path(library, problem);
        if shown.as_ref() != Some(&path) {
            lines.extend(hanging_wrap(&format!("  {path}"), width));
            shown = Some(path);
        }
        let sentence = format!("    ▲ {}", problem_sentence(library, problem));
        lines.extend(
            hanging_wrap(&sentence, width)
                .into_iter()
                .map(|line| line.style(PROBLEM)),
        );
    }
    lines.push(Line::default());
    lines
}

/// The names from the taxonomy down to the problem's node, or what a selection problem is about.
fn path(library: &LibraryDraft, problem: &Problem) -> String {
    problem
        .node()
        .and_then(|node| library.node(node))
        .map_or_else(
            || "Default and available taxonomies".into(),
            |node| {
                node.names()
                    .into_iter()
                    .map(|name| if name.is_empty() { "(no name)" } else { name })
                    .collect::<Vec<_>>()
                    .join(" › ")
            },
        )
}

#[cfg(test)]
mod tests {
    use gitserious_app::{ConfigurationSession, DraftEdit, DraftField, NodeRef, Policy};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{Event, KeyModifiers};
    use ratatui::style::Modifier;

    use super::super::tests::Workspace;
    use super::super::{Category, ConfigurationDestination, PendingAction, State};
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn press(state: &mut State, workspace: &Workspace, codes: &[KeyCode]) {
        for &code in codes {
            state.handle_event(
                Event::Key(KeyEvent::new(code, KeyModifiers::NONE)),
                workspace,
            );
        }
    }

    /// A fork of conventional whose first property is conditional with a rationale and no
    /// condition.
    fn empty_condition(state: &mut State) -> TestResult {
        let library = state
            .global
            .as_mut()
            .and_then(ConfigurationSession::library_mut)
            .ok_or("the Library loads")?;
        let source = library.find_path(&["conventional"]).ok_or("conventional")?;
        let fork = library.fork(&source)?;
        let Some(NodeRef::Taxonomy(taxonomy)) = library.node(&fork) else {
            return Err("the fork is a taxonomy".into());
        };
        let property = taxonomy.types()[0].properties()[0].id().clone();
        library.edit(&property, DraftEdit::Policy(Policy::Conditional))?;
        library.edit(&property, DraftEdit::Text(DraftField::Condition, ""))?;
        library.edit(
            &property,
            DraftEdit::Text(DraftField::Rationale, "Because."),
        )?;
        Ok(())
    }

    fn rows(terminal: &Terminal<TestBackend>) -> Vec<String> {
        let buffer = terminal.backend().buffer();
        buffer
            .content
            .chunks(usize::from(buffer.area.width))
            .map(|row| row.iter().map(ratatui::buffer::Cell::symbol).collect())
            .collect()
    }

    /// The cell where `text` starts on screen.
    fn find(terminal: &Terminal<TestBackend>, text: &str) -> Option<(u16, u16)> {
        rows(terminal).iter().enumerate().find_map(|(y, row)| {
            let x = row.find(text).map(|at| row[..at].chars().count())?;
            Some((u16::try_from(x).ok()?, u16::try_from(y).ok()?))
        })
    }

    #[test]
    fn an_empty_condition_shows_on_its_field_row_and_footer_and_blocks_apply() -> TestResult {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        empty_condition(&mut state)?;
        press(
            &mut state,
            &workspace,
            &[
                KeyCode::Char('2'),
                KeyCode::Down,
                KeyCode::Down,
                KeyCode::Down,
                KeyCode::Right,
                KeyCode::Right,
                KeyCode::Right,
                KeyCode::Down,
                KeyCode::Char('e'),
                KeyCode::Enter,
                KeyCode::Enter,
                KeyCode::Enter,
            ],
        );
        let mut terminal = Terminal::new(TestBackend::new(87, 40))?;
        terminal.draw(|frame| state.render(frame))?;
        let screen = rows(&terminal).join("\n");
        assert!(
            screen.contains("▲ A conditional policy needs a condition."),
            "{screen}"
        );
        assert_eq!(screen.matches("▲ invalid 1").count(), 3, "{screen}");
        let footer = rows(&terminal).pop().unwrap_or_default();
        assert!(
            footer.starts_with(" EDIT  · property ·  ▲ invalid 1 "),
            "{footer}"
        );
        let buffer = terminal.backend().buffer();
        let label = find(&terminal, "Condition").ok_or("the Condition label")?;
        assert_eq!(buffer[label].fg, Color::Red);
        let placeholder = find(&terminal, "hen does this apply?").ok_or("the placeholder")?;
        assert!(buffer[placeholder].modifier.contains(Modifier::ITALIC));
        let counter = find(&terminal, "0/80").ok_or("the counter")?;
        assert_eq!(buffer[counter].fg, Color::Red);

        state.handle_event(
            Event::Key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL)),
            &workspace,
        );
        assert!(matches!(
            state.screen,
            Screen::Review(ConfigurationDestination::Global)
        ));
        terminal.draw(|frame| state.render(frame))?;
        let screen = rows(&terminal).join("\n");
        assert!(
            screen.contains("Review changes — apply disabled"),
            "{screen}"
        );
        assert!(
            screen.contains("    ▲ A conditional policy needs a condition."),
            "{screen}"
        );
        press(&mut state, &workspace, &[KeyCode::Enter]);
        assert!(matches!(state.screen, Screen::Review(_)));
        assert_eq!(workspace.saves(), 0);
        Ok(())
    }

    #[test]
    fn the_unapplied_changes_dialog_does_not_apply_a_library_with_problems() -> TestResult {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        empty_condition(&mut state)?;
        state.active_scope = ConfigurationDestination::Global;
        state.category = Category::Project;
        state.request_project_action(PendingAction::ReviewProject);
        assert!(matches!(state.screen, Screen::ScopeChange(_)));
        press(&mut state, &workspace, &[KeyCode::Char('a')]);
        assert!(matches!(state.screen, Screen::Home));
        assert_eq!(workspace.saves(), 0);
        assert!(matches!(
            &state.status,
            Some(Notice::Error(message)) if message == "Apply is disabled until 1 problem is fixed."
        ));
        Ok(())
    }
}
