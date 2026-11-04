use std::path::Path;

use gitserious_app::HomeState;
use gitserious_core::VersionedTaxonomy;
use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::theme::theme;

const WORDMARK: [&str; 5] = [
    "▄████ ███ █████ ▄███ ████ ███▄ ███ ▄██▄ █  █ ▄███",
    "█      █    █   █    █    █  █  █  █  █ █  █ █   ",
    "█  ██  █    █   ████ ███  ███   █  █  █ █  █ ████",
    "█   █  █    █      █ █    █  █  █  █  █ █  █    █",
    "▀███▀ ███   █   ███▀ ████ █  █ ███ ▀██▀ ▀██▀ ███▀",
];
/// Every Home line aligns to the wordmark's edges.
const COLUMN_WIDTH: u16 = 49;

pub(super) const TAGLINE: &str = "Why did this commit message explain nothing?";

/// The one step Home offers for its state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum HomeAction {
    InitializeGitAndGitserious,
    InitializeGitserious,
    GoToCommit,
}

impl HomeAction {
    pub(super) fn of(state: &HomeState) -> Option<Self> {
        match state {
            HomeState::NoRepository { .. } => Some(Self::InitializeGitAndGitserious),
            HomeState::Uninitialized { .. } => Some(Self::InitializeGitserious),
            HomeState::Initialized { .. } => Some(Self::GoToCommit),
            HomeState::Unavailable { .. } => None,
        }
    }

    /// Whether the action opens the initialization editor.
    pub(super) fn initializes(self) -> bool {
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
    pub(super) fn hint(self) -> (&'static str, &'static str) {
        match self {
            Self::InitializeGitAndGitserious | Self::InitializeGitserious => {
                ("enter", "initialize")
            }
            Self::GoToCommit => ("enter", "commit"),
        }
    }
}

/// Draws the wordmark, the tagline, where gitserious runs, and the next step, centered in `area`.
pub(super) fn render(frame: &mut Frame<'_>, area: Rect, state: &HomeState, error: Option<&str>) {
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
        HomeState::NoRepository { directory } => {
            lines.extend(identity(directory));
            lines.push(Line::default());
            lines.push(Line::styled("no git repository found", pending()).centered());
        }
        HomeState::Uninitialized { root } => lines.extend(identity(root.as_path())),
        HomeState::Initialized {
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
                        VersionedTaxonomy::new(default, *version)
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
        HomeState::Unavailable { directory, error } => {
            lines.extend(identity(directory));
            lines.push(Line::default());
            lines.push(Line::styled(format!("✗ {error}"), blocks()));
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

fn identity(path: &Path) -> [Line<'static>; 2] {
    [
        Line::styled(
            name(path),
            Style::new().fg(theme().text).add_modifier(Modifier::BOLD),
        ),
        Line::styled(path.display().to_string(), Style::new().fg(theme().dim)),
    ]
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
