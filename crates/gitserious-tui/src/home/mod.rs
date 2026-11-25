use std::path::Path;

use crate::shared::paths::path_lines;
use crate::shared::work_tree::guidance;
use gitserious_app::{ConfigurationWorkspace, ProjectOverview, ProjectWorkspace};
use gitserious_core::{TaxonomyReference, built_in_taxonomies};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

mod init_editor;

use crate::theme::theme;
use init_editor::{EditorOutcome, InitEditor};

const WORDMARK: [&str; 5] = [
    "▄████ ███ █████ ▄███ ████ ███▄ ███ ▄██▄ █  █ ▄███",
    "█      █    █   █    █    █  █  █  █  █ █  █ █   ",
    "█  ██  █    █   ████ ███  ███   █  █  █ █  █ ████",
    "█   █  █    █      █ █    █  █  █  █  █ █  █    █",
    "▀███▀ ███   █   ███▀ ████ █  █ ███ ▀██▀ ▀██▀ ███▀",
];
/// Every Home line aligns to the wordmark's edges.
const COLUMN_WIDTH: u16 = 49;

pub(crate) const TAGLINE: &str = "Why did this commit message explain nothing?";

/// The one step Home offers for its state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HomeAction {
    InitializeGitAndGitserious,
    InitializeGitserious,
    GoToCommit,
}

impl HomeAction {
    fn of(state: &ProjectOverview) -> Option<Self> {
        match state {
            ProjectOverview::NoRepository { .. } => Some(Self::InitializeGitAndGitserious),
            ProjectOverview::Uninitialized { .. } => Some(Self::InitializeGitserious),
            ProjectOverview::Initialized { .. } => Some(Self::GoToCommit),
            ProjectOverview::Unavailable { .. } => None,
        }
    }

    /// Whether the action opens the initialization editor.
    pub(crate) fn initializes(self) -> bool {
        self != Self::GoToCommit
    }

    fn label(self) -> &'static str {
        match self {
            Self::InitializeGitAndGitserious => "initialize git and gitserious",
            Self::InitializeGitserious => "initialize gitserious",
            Self::GoToCommit => "Go to Commit",
        }
    }

    /// The footer's `enter` hint.
    fn hint(self) -> (&'static str, &'static str) {
        match self {
            Self::InitializeGitAndGitserious | Self::InitializeGitserious => {
                ("enter", "initialize")
            }
            Self::GoToCommit => ("enter", "commit"),
        }
    }
}

/// The project overview Home shows, and the editor that initializes the project.
pub(crate) struct HomeView {
    overview: ProjectOverview,
    /// Open while the user chooses the new project's taxonomies.
    editor: Option<InitEditor>,
    /// Why the last initialization from Home failed.
    error: Option<String>,
    /// Commit opened the editor, so closing it returns there.
    returns_to_commit: bool,
}

/// What Home asks the root to do.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HomeRequest {
    GoToCommit,
    /// Open the initialization editor.
    Initialize,
    /// The project now exists, so any page that loaded it before is stale.
    Initialized,
}

impl HomeView {
    pub(crate) fn new(project: &dyn ProjectWorkspace) -> Self {
        Self {
            overview: project.overview(),
            editor: None,
            error: None,
            returns_to_commit: false,
        }
    }

    /// Opens the editor for Commit, which it returns to once initialization ends either way.
    pub(crate) fn initialize_for_commit(
        &mut self,
        project: &dyn ProjectWorkspace,
        config: &dyn ConfigurationWorkspace,
    ) {
        self.refresh(project);
        self.error = None;
        self.offer_initialization(config);
        self.returns_to_commit = true;
    }

    /// Whether the editor that just closed was Commit's, clearing it.
    pub(crate) fn take_return_to_commit(&mut self) -> bool {
        std::mem::take(&mut self.returns_to_commit)
    }

    /// Rereads the overview, and closes the editor once there is nothing to initialize.
    pub(crate) fn enter(&mut self, project: &dyn ProjectWorkspace) {
        self.refresh(project);
        self.error = None;
        if !self.action().is_some_and(HomeAction::initializes) {
            self.editor = None;
        }
    }

    pub(crate) fn refresh(&mut self, project: &dyn ProjectWorkspace) {
        self.overview = project.overview();
    }

    pub(crate) fn action(&self) -> Option<HomeAction> {
        HomeAction::of(&self.overview)
    }

    pub(crate) fn editing(&self) -> bool {
        self.editor.is_some()
    }

    /// Whether Home's step opens the initialization editor.
    pub(crate) fn can_initialize(&self) -> bool {
        self.action().is_some_and(HomeAction::initializes)
    }

    /// Whether the open editor takes `key` before the root's keys. It leaves tab, q, and slash to the root.
    pub(crate) fn captures(&self, key: &KeyEvent) -> bool {
        self.editing()
            && !matches!(
                key.code,
                KeyCode::Tab | KeyCode::BackTab | KeyCode::Char('q' | '/')
            )
    }

    /// Home's palette commands.
    pub(crate) fn commands(&self) -> Vec<(&'static str, HomeRequest)> {
        if self.can_initialize() {
            vec![("Initialize gitserious", HomeRequest::Initialize)]
        } else {
            Vec::new()
        }
    }

    /// Opens the editor on the built-in taxonomies and the Library's.
    pub(crate) fn offer_initialization(&mut self, config: &dyn ConfigurationWorkspace) {
        let library = config.library_taxonomies().unwrap_or_else(|error| {
            self.error = Some(format!("Library taxonomies are unavailable: {error}"));
            Vec::new()
        });
        let creates_repository = matches!(self.overview, ProjectOverview::NoRepository { .. });
        self.editor = Some(InitEditor::new(
            built_in_taxonomies(),
            &library,
            creates_repository,
        ));
    }

    /// Hands `key` to the open editor, or takes Home's step on enter.
    pub(crate) fn key(
        &mut self,
        key: KeyEvent,
        project: &dyn ProjectWorkspace,
    ) -> Option<HomeRequest> {
        let Some(editor) = &mut self.editor else {
            return match self.action() {
                _ if key.code != KeyCode::Enter => None,
                Some(HomeAction::GoToCommit) => Some(HomeRequest::GoToCommit),
                Some(_) => Some(HomeRequest::Initialize),
                None => None,
            };
        };
        match editor.key(key) {
            EditorOutcome::Stay => None,
            EditorOutcome::Back => {
                self.editor = None;
                self.error = None;
                self.take_return_to_commit()
                    .then_some(HomeRequest::GoToCommit)
            }
            EditorOutcome::Initialize(configuration) => {
                let outcome = match project.initialize(&configuration) {
                    Ok(()) => {
                        self.editor = None;
                        self.error = None;
                        Some(HomeRequest::Initialized)
                    }
                    Err(error) => {
                        self.error = Some(error);
                        None
                    }
                };
                self.refresh(project);
                outcome
            }
        }
    }

    pub(crate) fn render(&self, frame: &mut Frame<'_>, area: Rect) {
        let error = self.error.as_deref();
        match (&self.editor, &self.overview) {
            (Some(editor), ProjectOverview::Uninitialized { root }) => {
                editor.render(frame, area, root.as_path(), error);
            }
            (Some(editor), ProjectOverview::NoRepository { directory }) => {
                editor.render(frame, area, directory, error);
            }
            _ => render(frame, area, &self.overview, error),
        }
    }

    pub(crate) fn hints(&self) -> Vec<(&'static str, &'static str)> {
        self.editor.as_ref().map_or_else(
            || self.action().map(HomeAction::hint).into_iter().collect(),
            InitEditor::hints,
        )
    }
}

/// Draws the wordmark, the tagline, where gitserious runs, and the next step, centered in `area`.
fn render(frame: &mut Frame<'_>, area: Rect, state: &ProjectOverview, error: Option<&str>) {
    let text = Style::new().fg(theme().text);
    let dim = Style::new().fg(theme().dim);
    let bold = text.add_modifier(Modifier::BOLD);
    let mut lines: Vec<Line<'static>> = WORDMARK
        .iter()
        .map(|row| Line::styled(*row, text))
        .collect();
    lines.push(Line::default());
    lines.push(Line::styled(TAGLINE, dim).centered());
    lines.push(Line::default());
    match state {
        ProjectOverview::NoRepository { directory } => {
            lines.extend(identity(directory));
            lines.push(Line::default());
            lines.push(Line::styled("no git repository found", pending()).centered());
        }
        ProjectOverview::Uninitialized { root } => lines.extend(identity(root.as_path())),
        ProjectOverview::Initialized {
            root,
            default,
            version,
            policy_untracked,
        } => {
            lines.push(Line::from(vec![
                Span::styled("✓ ", dim),
                Span::styled(name(root.as_path()), bold),
                Span::styled(
                    format!(
                        " is initialized with {}",
                        TaxonomyReference::new(default, *version)
                    ),
                    dim,
                ),
            ]));
            if *policy_untracked {
                lines.push(Line::default());
                lines.push(Line::styled("◐ gitserious.toml is untracked.", pending()));
                lines.push(Line::styled(
                    "  Commit it so the policy travels with the repo:",
                    dim,
                ));
                lines.push(Line::styled("  git add gitserious.toml", bold));
            }
        }
        ProjectOverview::Unavailable {
            directory,
            error,
            missing,
        } => {
            lines.extend(identity(directory));
            lines.push(Line::default());
            let reason = missing.as_ref().map_or_else(|| error.clone(), guidance);
            lines.push(Line::styled(format!("✗ {reason}"), blocks()));
        }
    }
    if let Some(action) = HomeAction::of(state) {
        lines.push(Line::default());
        lines.push(action_row(action));
    }
    if let Some(error) = error {
        lines.push(Line::default());
        lines.push(Line::styled(format!("✗ {error}"), blocks()));
    }
    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    let height = u16::try_from(paragraph.line_count(COLUMN_WIDTH)).unwrap_or(u16::MAX);
    let [row] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(area);
    let [column] = Layout::horizontal([Constraint::Length(COLUMN_WIDTH)])
        .flex(Flex::Center)
        .areas(row);
    frame.render_widget(paragraph, column);
}

fn pending() -> Style {
    Style::new().fg(theme().pending)
}

fn blocks() -> Style {
    Style::new().fg(theme().blocks)
}

fn name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// The directory's name, then its whole path broken after separators.
fn identity(path: &Path) -> Vec<Line<'static>> {
    std::iter::once(Line::styled(
        name(path),
        Style::new().fg(theme().text).add_modifier(Modifier::BOLD),
    ))
    .chain(
        path_lines(&path.display().to_string(), usize::from(COLUMN_WIDTH))
            .into_iter()
            .map(|line| Line::styled(line, Style::new().fg(theme().dim))),
    )
    .collect()
}

fn action_row(action: HomeAction) -> Line<'static> {
    let label = format!(" {}", action.label());
    let key = "enter ";
    let gap = usize::from(COLUMN_WIDTH).saturating_sub(label.len() + key.len());
    Line::styled(
        format!("{label}{}{key}", " ".repeat(gap)),
        Style::new()
            .fg(theme().ground)
            .bg(theme().cursor)
            .add_modifier(Modifier::BOLD),
    )
}
