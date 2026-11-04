//! Explicit copies between the project and the Library. A copy is independent: nothing keeps
//! the two in sync afterward.

use gitserious_app::{Collision, ConfigurationSession, CopyIn, TaxonomyOrigin};
use gitserious_core::{Taxonomy, TaxonomyId};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph};

use super::{Notice, Screen, State};
use crate::theme::{
    block_title, frame_style, navigation_key_style, row_style, section_heading_style,
};

impl State {
    /// Lists the Library's taxonomies to copy into the project.
    pub(super) fn open_import(&mut self) {
        if self.catalog().is_none() {
            self.status = Some(Notice::error(format!(
                "Adding from the Library reads the Library. {}",
                self.library_error()
            )));
            return;
        }
        self.screen = Screen::Import { selected: 0 };
    }

    pub(super) fn import_key(&mut self, key: KeyEvent, selected: usize) {
        let rows = self.catalog().unwrap_or_default();
        match key.code {
            KeyCode::Up => {
                self.screen = Screen::Import {
                    selected: selected.saturating_sub(1),
                };
            }
            KeyCode::Down => {
                self.screen = Screen::Import {
                    selected: (selected + 1).min(rows.len().saturating_sub(1)),
                };
            }
            KeyCode::Enter => {
                if let Some(taxonomy) = rows.get(selected) {
                    self.import(taxonomy);
                }
            }
            KeyCode::Esc => self.screen = Screen::Home,
            _ => {}
        }
    }

    /// Copies a Library taxonomy into the project draft, or asks how when it would collide.
    fn import(&mut self, taxonomy: &Taxonomy) {
        let Some(draft) = self
            .project
            .as_ref()
            .and_then(ConfigurationSession::project_draft)
        else {
            return;
        };
        match draft.collision(taxonomy) {
            Collision::Absent => self.copy_into_project(taxonomy.id(), CopyIn::New),
            Collision::Identical => {
                self.screen = Screen::Home;
                self.status = Some(Notice::info(format!(
                    "The project already has this \"{}\".",
                    taxonomy.id()
                )));
            }
            Collision::Differs | Collision::Reserved => {
                self.screen = Screen::ImportConflict(taxonomy.id().clone());
            }
        }
    }

    /// Stages the copy of Library taxonomy `id` and selects it in the project outline.
    pub(super) fn copy_into_project(&mut self, id: &TaxonomyId, how: CopyIn) {
        self.screen = Screen::Home;
        let Some(taxonomy) = self
            .catalog()
            .and_then(|rows| rows.into_iter().find(|taxonomy| taxonomy.id() == id))
        else {
            return;
        };
        let Some(draft) = self
            .project
            .as_mut()
            .and_then(ConfigurationSession::project_draft_mut)
        else {
            return;
        };
        match draft.copy_in(&taxonomy, how) {
            Ok(node) => {
                self.project_taxonomies.select(node);
                self.status = Some(Notice::info(format!(
                    "Copied \"{id}\" into the project. Ctrl+S reviews before applying."
                )));
            }
            Err(error) => self.status = Some(Notice::error(error)),
        }
    }

    pub(super) fn render_import(&self, frame: &mut Frame<'_>, area: Rect, selected: usize) {
        let draft = self
            .project
            .as_ref()
            .and_then(ConfigurationSession::project_draft);
        let rows = self.catalog().unwrap_or_default();
        let items = rows
            .iter()
            .enumerate()
            .map(|(index, taxonomy)| {
                let state = match draft.map(|draft| draft.collision(taxonomy)) {
                    Some(Collision::Identical) => "already in project",
                    Some(Collision::Differs | Collision::Reserved) => "differs from project copy",
                    _ => "new",
                };
                let origin = self
                    .global
                    .as_ref()
                    .and_then(|session| session.catalog().ok())
                    .and_then(|catalog| catalog.origin(taxonomy.id()))
                    .map_or("", TaxonomyOrigin::as_str);
                ListItem::new(format!(
                    "{:<20} v{:<3} {origin:<9} {state}",
                    taxonomy.id().as_str(),
                    taxonomy.version().get(),
                ))
                .style(row_style(index))
            })
            .collect::<Vec<_>>();
        let mut list = ListState::default();
        list.select(Some(selected));
        let parts = Layout::vertical([Constraint::Length(2), Constraint::Min(1)]).split(area);
        frame.render_widget(
            Paragraph::new("copy a Library taxonomy into this project")
                .style(section_heading_style()),
            parts[0],
        );
        frame.render_stateful_widget(
            List::new(items)
                .block(
                    Block::bordered()
                        .border_style(frame_style())
                        .title(block_title("Add from Library")),
                )
                .highlight_style(navigation_key_style()),
            parts[1],
            &mut list,
        );
    }
}
