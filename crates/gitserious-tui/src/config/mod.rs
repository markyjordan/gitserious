mod editor;
mod form;
mod presentation;
mod taxonomy_form;
mod typeset_form;

use std::io::{self, IsTerminal};

use gitserious_app::{
    ConfigurationDestination, ConfigurationEditor, ConfigurationSession, ConfigurationWorkspace,
};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

use presentation::{Definition, Kind, entries};

/// Terminal configuration browser and reviewed editing adapter.
#[derive(Clone, Copy, Debug, Default)]
pub struct RatatuiConfigurationEditor;

impl ConfigurationEditor for RatatuiConfigurationEditor {
    fn edit(&self, workspace: &dyn ConfigurationWorkspace) -> Result<(), String> {
        if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
            return Err("configuration editing requires an interactive terminal".into());
        }
        ratatui::run(|terminal| {
            let _paste = PasteGuard::enable()?;
            let mut state = State::new(workspace);
            loop {
                terminal.draw(|frame| state.render(frame))?;
                if state.event(&event::read()?, workspace) {
                    return Ok::<_, io::Error>(());
                }
            }
        })
        .map_err(|error| format!("configuration editing failed: {error}"))
    }
}

struct PasteGuard;
impl PasteGuard {
    fn enable() -> io::Result<Self> {
        ratatui::crossterm::execute!(io::stdout(), event::EnableBracketedPaste)?;
        Ok(Self)
    }
}
impl Drop for PasteGuard {
    fn drop(&mut self) {
        let _ = ratatui::crossterm::execute!(io::stdout(), event::DisableBracketedPaste);
    }
}

enum Screen {
    Browse,
    Edit,
    Details(String),
    Review(String),
    Confirm(Leave),
}
#[derive(Clone, Copy)]
enum Leave {
    Quit,
    Destination(ConfigurationDestination),
}

struct State {
    session: Option<ConfigurationSession>,
    destination: ConfigurationDestination,
    kind: Kind,
    selected: usize,
    list: ListState,
    screen: Screen,
    scroll: u16,
    max_scroll: u16,
    status: String,
    too_small: bool,
    editor: Option<editor::Editor>,
}

impl State {
    fn new(workspace: &dyn ConfigurationWorkspace) -> Self {
        let mut state = Self {
            session: None,
            destination: ConfigurationDestination::Global,
            kind: Kind::Taxonomy,
            selected: 0,
            list: ListState::default(),
            screen: Screen::Browse,
            scroll: 0,
            max_scroll: 0,
            status: String::new(),
            too_small: false,
            editor: None,
        };
        state.load(workspace, ConfigurationDestination::Global);
        state
    }

    fn load(
        &mut self,
        workspace: &dyn ConfigurationWorkspace,
        destination: ConfigurationDestination,
    ) {
        match workspace.load(destination) {
            Ok(session) => {
                self.session = Some(session);
                self.destination = destination;
                self.selected = 0;
                self.list = ListState::default();
                self.status.clear();
            }
            Err(error) => self.status = error,
        }
    }

    fn selected(&self) -> Option<Definition> {
        self.session.as_ref().and_then(|session| {
            entries(session.custom(), self.kind)
                .get(self.selected)
                .cloned()
        })
    }

    fn leave(&mut self, action: Leave, workspace: &dyn ConfigurationWorkspace) -> bool {
        if self
            .session
            .as_ref()
            .is_some_and(ConfigurationSession::is_dirty)
        {
            self.screen = Screen::Confirm(action);
            return false;
        }
        self.finish_leave(action, workspace)
    }

    fn finish_leave(&mut self, action: Leave, workspace: &dyn ConfigurationWorkspace) -> bool {
        self.screen = Screen::Browse;
        match action {
            Leave::Quit => true,
            Leave::Destination(destination) => {
                self.load(workspace, destination);
                false
            }
        }
    }

    fn event(&mut self, event: &Event, workspace: &dyn ConfigurationWorkspace) -> bool {
        if let Event::Paste(text) = event {
            if !self.too_small
                && matches!(self.screen, Screen::Edit)
                && let Some(editor) = &mut self.editor
                && let Err(error) = editor.form_mut().paste(text)
            {
                self.status = error;
            }
            return false;
        }
        let Event::Key(key) = event else {
            return false;
        };
        if key.kind == KeyEventKind::Release {
            return false;
        }
        if self.too_small {
            match self.screen {
                Screen::Confirm(_)
                    if matches!(
                        key.code,
                        KeyCode::Char('y' | 'n') | KeyCode::Esc | KeyCode::Enter
                    ) => {}
                Screen::Browse if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) => {}
                Screen::Edit if key.code == KeyCode::Esc => {}
                Screen::Edit
                    if self
                        .editor
                        .as_ref()
                        .is_some_and(|editor| editor.form().confirming_discard())
                        && matches!(key.code, KeyCode::Char('y' | 'n') | KeyCode::Enter) => {}
                _ => return false,
            }
        }
        match &self.screen {
            Screen::Edit => self.edit_key(*key),
            Screen::Confirm(action) => {
                let action = *action;
                match key.code {
                    KeyCode::Char('y') => return self.finish_leave(action, workspace),
                    KeyCode::Esc | KeyCode::Enter | KeyCode::Char('n') => {
                        self.screen = Screen::Browse;
                    }
                    _ => {}
                }
            }
            Screen::Review(_) | Screen::Details(_) => match key.code {
                KeyCode::Esc => self.screen = Screen::Browse,
                KeyCode::Down => self.scroll = self.scroll.saturating_add(1).min(self.max_scroll),
                KeyCode::Up => self.scroll = self.scroll.saturating_sub(1),
                KeyCode::PageDown => {
                    self.scroll = self.scroll.saturating_add(10).min(self.max_scroll);
                }
                KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(10),
                KeyCode::Enter if matches!(self.screen, Screen::Review(_)) => {
                    if let Some(session) = &self.session {
                        match workspace.save(session) {
                            Ok(saved) => {
                                self.session = Some(saved);
                                self.screen = Screen::Browse;
                                self.status = "Configuration saved.".into();
                            }
                            Err(error) => {
                                self.status = error;
                                self.screen = Screen::Browse;
                            }
                        }
                    }
                }
                _ => {}
            },
            Screen::Browse => return self.browse_key(key.code, key.modifiers, workspace),
        }
        false
    }

    fn browse_key(
        &mut self,
        key: KeyCode,
        modifiers: KeyModifiers,
        workspace: &dyn ConfigurationWorkspace,
    ) -> bool {
        match key {
            KeyCode::Char('n') => self.open_definition(true),
            KeyCode::Char('e') => self.open_definition(false),
            KeyCode::Char('d') => self.delete_definition(),
            KeyCode::Char('q') | KeyCode::Esc => return self.leave(Leave::Quit, workspace),
            KeyCode::Tab => {
                let destination = match self.destination {
                    ConfigurationDestination::Global => ConfigurationDestination::Project,
                    ConfigurationDestination::Project => ConfigurationDestination::Global,
                };
                return self.leave(Leave::Destination(destination), workspace);
            }
            KeyCode::Char('1' | '2' | '3') => {
                self.kind = match key {
                    KeyCode::Char('1') => Kind::Taxonomy,
                    KeyCode::Char('2') => Kind::Typeset,
                    _ => Kind::Template,
                };
                self.selected = 0;
                self.list = ListState::default();
            }
            KeyCode::Down => {
                let count = self
                    .session
                    .as_ref()
                    .map_or(0, |session| entries(session.custom(), self.kind).len());
                self.selected = self.selected.saturating_add(1).min(count.saturating_sub(1));
            }
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Enter => {
                if let Some(selected) = self.selected() {
                    self.screen = Screen::Details(selected.describe());
                    self.scroll = 0;
                }
            }
            KeyCode::Char('s') if modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(session) = &self.session {
                    if session.is_dirty() {
                        match session.validate() {
                            Ok(()) => {
                                self.screen = Screen::Review(presentation::review(session));
                                self.scroll = 0;
                                self.status.clear();
                            }
                            Err(error) => self.status = error,
                        }
                    } else {
                        self.status = "No changes to save.".into();
                    }
                }
            }
            _ => {}
        }
        false
    }

    fn render(&mut self, frame: &mut Frame<'_>) {
        let area = frame.area();
        frame.render_widget(
            Block::default().style(Style::default().bg(Color::Black).fg(Color::White)),
            area,
        );
        self.too_small = area.width < 60 || area.height < 16;
        if self.too_small {
            let message = if matches!(self.screen, Screen::Confirm(_))
                || self
                    .editor
                    .as_ref()
                    .is_some_and(|editor| editor.form().confirming_discard())
            {
                "Discard unsaved changes? y: discard | n/esc: keep editing"
            } else {
                "Configuration needs at least 60 columns and 16 rows. Resize to continue."
            };
            frame.render_widget(Paragraph::new(message).wrap(Wrap { trim: false }), area);
            return;
        }
        let [header, body, status, footer] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(2),
            Constraint::Length(2),
        ])
        .areas(area);
        self.render_header(frame, header);
        let hint = match &self.screen {
            Screen::Edit => {
                if let Some(editor) = &mut self.editor {
                    editor.form_mut().render(frame, body);
                }
                "tab/shift+tab: fields | ctrl+n/d: add/remove | alt+↑/↓: order\nctrl+s: stage | esc: back"
            }
            Screen::Browse => {
                let [list_area, detail_area] =
                    Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)])
                        .areas(body);
                let definitions = self
                    .session
                    .as_ref()
                    .map_or_else(Vec::new, |session| entries(session.custom(), self.kind));
                self.selected = self.selected.min(definitions.len().saturating_sub(1));
                self.list.select(if definitions.is_empty() {
                    None
                } else {
                    Some(self.selected)
                });
                let list = List::new(
                    definitions
                        .iter()
                        .map(|value| ListItem::new(value.label()))
                        .collect::<Vec<_>>(),
                )
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(self.kind.label()),
                )
                .highlight_style(Style::default().fg(Color::Black).bg(Color::Yellow));
                frame.render_stateful_widget(list, list_area, &mut self.list);
                let detail = definitions.get(self.selected).map_or_else(
                    || "No definitions in this destination.".to_owned(),
                    Definition::describe,
                );
                frame.render_widget(
                    Paragraph::new(detail)
                        .wrap(Wrap { trim: false })
                        .block(Block::bordered().title("Definition — enter to inspect")),
                    detail_area,
                );
                "tab: destination | 1/2/3: kinds | enter: inspect | n/e/d: definitions\nctrl+s: review changes | q: quit"
            }
            Screen::Details(text) => {
                self.render_text(frame, body, text.clone(), "Definition");
                "↑/↓, page up/down: scroll | esc: back"
            }
            Screen::Review(text) => {
                self.render_text(frame, body, text.clone(), "Review changes — enter to apply");
                "↑/↓, page up/down: scroll | enter: apply | esc: keep editing"
            }
            Screen::Confirm(_) => {
                frame.render_widget(Paragraph::new("Discard all unsaved configuration changes?\n\ny: discard    enter / esc / n: keep editing").block(Block::bordered().title("Discard changes")), body);
                "y: discard | enter/esc/n: keep editing"
            }
        };
        frame.render_widget(
            Paragraph::new(self.status.as_str())
                .wrap(Wrap { trim: false })
                .style(Style::default().fg(Color::Yellow)),
            status,
        );
        frame.render_widget(
            Paragraph::new(hint).style(Style::default().fg(Color::Black).bg(Color::Yellow)),
            footer,
        );
    }

    fn render_text(&mut self, frame: &mut Frame<'_>, area: Rect, text: String, title: &str) {
        let paragraph = Paragraph::new(text).wrap(Wrap { trim: false });
        let lines = paragraph.line_count(area.width.saturating_sub(2));
        self.max_scroll =
            u16::try_from(lines.saturating_sub(usize::from(area.height.saturating_sub(2))))
                .unwrap_or(u16::MAX);
        self.scroll = self.scroll.min(self.max_scroll);
        frame.render_widget(
            paragraph
                .scroll((self.scroll, 0))
                .block(Block::bordered().title(title)),
            area,
        );
    }

    fn render_header(&self, frame: &mut Frame<'_>, area: Rect) {
        let scope = match self.destination {
            ConfigurationDestination::Global => "GLOBAL",
            ConfigurationDestination::Project => "PROJECT",
        };
        let location = self
            .session
            .as_ref()
            .and_then(ConfigurationSession::root)
            .map_or_else(
                || "Personal reusable definitions".to_owned(),
                |root| root.as_path().display().to_string(),
            );
        let dirty = if self
            .session
            .as_ref()
            .is_some_and(ConfigurationSession::is_dirty)
        {
            " • unsaved changes"
        } else {
            ""
        };
        frame.render_widget(
            Paragraph::new(format!("Configure gitserious | {scope}{dirty}\n{location}"))
                .style(Style::default().fg(Color::Yellow)),
            area,
        );
    }

    fn open_definition(&mut self, create: bool) {
        let Some(session) = &self.session else {
            return;
        };
        let original = if create { None } else { self.selected() };
        if original.as_ref().is_some_and(Definition::is_builtin) {
            self.status = "Built-in definitions are read-only.".into();
            return;
        }
        let result = match self.kind {
            Kind::Taxonomy => {
                let original = match original {
                    Some(Definition::Taxonomy(value)) => Some(value),
                    _ => None,
                };
                Ok(editor::Editor::Taxonomy(taxonomy_form::TaxonomyForm::new(
                    original,
                )))
            }
            Kind::Typeset => {
                let original = match original {
                    Some(Definition::Typeset(value)) => Some(value),
                    _ => None,
                };
                let taxonomies = entries(session.custom(), Kind::Taxonomy)
                    .into_iter()
                    .filter_map(|value| match value {
                        Definition::Taxonomy(value) => Some(value),
                        _ => None,
                    })
                    .collect();
                typeset_form::TypesetForm::new(taxonomies, original).map(editor::Editor::Typeset)
            }
            Kind::Template => Err("Select Taxonomies or Typesets to author definitions.".into()),
        };
        match result {
            Ok(editor) => {
                self.editor = Some(editor);
                self.screen = Screen::Edit;
                self.status.clear();
            }
            Err(error) => self.status = error,
        }
    }

    fn delete_definition(&mut self) {
        let edit = match self.selected() {
            Some(Definition::Taxonomy(value)) => {
                gitserious_app::ConfigurationEdit::DeleteTaxonomy(value.id().clone())
            }
            Some(Definition::Typeset(value)) => gitserious_app::ConfigurationEdit::DeleteTypeset {
                taxonomy: value.taxonomy().clone(),
                typeset: value.id().clone(),
            },
            _ => return,
        };
        if let Some(session) = &mut self.session {
            match session.stage([edit]) {
                Ok(()) => {
                    self.status = "Deletion staged. ctrl+s reviews changes before saving.".into();
                }
                Err(error) => self.status = error,
            }
        }
    }

    fn edit_key(&mut self, key: event::KeyEvent) {
        let Some(editor) = &mut self.editor else {
            return;
        };
        match editor.key(key) {
            Ok(form::FormAction::Cancel) => {
                self.editor = None;
                self.screen = Screen::Browse;
                self.status.clear();
            }
            Ok(form::FormAction::Submit) => {
                let result = editor.submit().and_then(|edits| {
                    self.session
                        .as_mut()
                        .ok_or("missing editing session")?
                        .stage(edits)
                });
                match result {
                    Ok(()) => {
                        self.editor = None;
                        self.screen = Screen::Browse;
                        self.status = "Change staged. ctrl+s reviews changes before saving.".into();
                    }
                    Err(error) => self.status = error,
                }
            }
            Ok(form::FormAction::Continue) => {}
            Err(error) => self.status = error,
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/config_browser.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/unit/config_forms.rs"]
mod form_tests;
