//! What a property sees of one state: the editor, the stored Library, and the frame.

use gitserious_app::{DraftField, GlobalConfiguration, LibraryDraft, ProjectSnapshot};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::config::taxonomies::{Observation, ObservedMode as Mode};
use crate::config::test_support::{Workspace, footer_text};
use crate::config::{Category, HomeFocus, Notice, ProjectSection, Screen, State};
use crate::shared::components::confirmation_dialog;

/// Everything a user can observe or that later input can affect, as a property sees it.
#[derive(Debug)]
#[expect(dead_code, reason = "the fingerprint reads these fields through Debug")]
pub(super) struct Snapshot {
    pub(super) screen: Screen,
    pub(super) category: Category,
    pub(super) home_focus: HomeFocus,
    pub(super) project_section: ProjectSection,
    pub(super) taxonomies: Observation,
    pub(super) overlay: Option<String>,
    pub(super) status: Option<String>,
    pub(super) global: Option<LibraryDraft>,
    pub(super) project: Option<ProjectSnapshot>,
    /// Whether the Library has changes apply would write.
    pub(super) unapplied: bool,
    /// The count of unapplied changes the root's Config tab shows.
    pub(super) load_errors: [String; 2],
    /// The Library the fake workspace stores. Config never writes the project.
    pub(super) saved: LibraryDraft,
    pub(super) dialog: String,
    pub(super) dialog_hints: Option<Vec<String>>,
    pub(super) too_small: bool,
    pub(super) frame: String,
}

impl Snapshot {
    pub(super) fn of(
        state: &State,
        workspace: &Workspace,
        terminal: &Terminal<TestBackend>,
    ) -> Self {
        let buffer = terminal.backend().buffer();
        Self {
            screen: state.screen.clone(),
            category: state.category,
            home_focus: state.home_focus,
            project_section: state.project_section,
            taxonomies: state.taxonomies.observe(state.library()),
            overlay: state
                .home_overlay
                .as_ref()
                .map(|overlay| format!("{overlay:?}")),
            status: state.status.as_ref().map(|notice| match notice {
                Notice::Error(message) => format!("error: {message}"),
            }),
            global: state.global.clone(),
            project: state.project.clone(),
            unapplied: state.library_dirty(),
            load_errors: [state.global_error.clone(), state.project_error.clone()],
            saved: workspace.global.borrow().clone(),
            dialog: format!("{:?}", state.dialog),
            dialog_hints: state
                .confirmation()
                .map(|question| confirmation_dialog::hints(question.choices))
                .or_else(|| state.taxonomies.picker_hints())
                .map(|hints| {
                    hints
                        .into_iter()
                        .map(|(hint, label)| format!("{hint} {label}"))
                        .collect()
                }),
            too_small: state.too_small,
            frame: buffer
                .content
                .chunks(usize::from(buffer.area.width))
                .map(|row| row.iter().map(ratatui::buffer::Cell::symbol).collect())
                .collect::<Vec<String>>()
                .join("\n"),
        }
    }

    pub(super) fn library(&self) -> Option<&LibraryDraft> {
        self.global.as_ref()
    }

    pub(super) fn footer(&self) -> String {
        footer_text(self.frame.lines().last().unwrap_or_default())
    }

    pub(super) fn staged_library(&self) -> Option<GlobalConfiguration> {
        self.library().and_then(|library| library.build().ok())
    }

    /// The Library the fake workspace stores: the last applied session's Library.
    pub(super) fn stored_library(&self) -> Option<GlobalConfiguration> {
        self.saved.build().ok()
    }

    /// Whether the Taxonomies outline takes `input` with no overlay in the way.
    pub(super) fn outline_takes_keys(&self) -> bool {
        at_outline(self) && self.overlay.is_none()
    }
}

pub(super) fn at_outline(snapshot: &Snapshot) -> bool {
    matches!(snapshot.screen, Screen::Home)
        && snapshot.category == Category::Taxonomies
        && snapshot.home_focus == HomeFocus::Content
}

pub(super) fn selects(snapshot: &Snapshot, path: &[&str]) -> bool {
    snapshot
        .taxonomies
        .cursor_path
        .iter()
        .map(String::as_str)
        .eq(path.iter().copied())
}

/// The field EDIT sits on, if the outline is in EDIT.
pub(super) fn editing(snapshot: &Snapshot) -> Option<DraftField> {
    match &snapshot.taxonomies.mode {
        Mode::Edit { field, .. } if at_outline(snapshot) => Some(*field),
        _ => None,
    }
}
