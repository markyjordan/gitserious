use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, List, ListItem, ListState, Paragraph, Wrap},
};
use tui_textarea::TextArea;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Group {
    Metadata,
    Item(usize),
}

pub(super) struct Field {
    pub label: String,
    pub group: Group,
    pub readonly: bool,
    pub multiline: bool,
    pub options: Vec<String>,
    area: TextArea<'static>,
}

impl Field {
    pub(super) fn new(
        label: impl Into<String>,
        value: &str,
        multiline: bool,
        readonly: bool,
        group: Group,
    ) -> Self {
        let mut area = TextArea::from(value.split('\n').map(str::to_owned).collect::<Vec<_>>());
        area.set_style(Style::default().fg(Color::White).bg(Color::Black));
        area.set_cursor_line_style(Style::default());
        Self {
            label: label.into(),
            group,
            readonly,
            multiline,
            options: Vec::new(),
            area,
        }
    }

    pub(super) fn value(&self) -> String {
        self.area.lines().join("\n")
    }

    pub(super) fn set_value(&mut self, value: &str) {
        self.area.select_all();
        self.area.insert_str(value);
        self.area.cancel_selection();
    }
}

pub(super) enum FormAction {
    Continue,
    Submit,
    Cancel,
}

pub(super) struct Form {
    pub title: String,
    pub fields: Vec<Field>,
    pub focus: usize,
    initial: Vec<String>,
    discard: bool,
    list: ListState,
}

impl Form {
    pub(super) fn parse<T, E: std::fmt::Display>(
        &self,
        index: usize,
        parse: impl FnOnce(String) -> Result<T, E>,
    ) -> Result<T, String> {
        parse(self.fields[index].value())
            .map_err(|error| format!("{}: {error}", self.fields[index].label))
    }
    pub(super) fn new(title: impl Into<String>, fields: Vec<Field>) -> Self {
        let initial = fields.iter().map(Field::value).collect();
        Self {
            title: title.into(),
            fields,
            focus: 0,
            initial,
            discard: false,
            list: ListState::default(),
        }
    }

    pub(super) fn is_dirty(&self) -> bool {
        self.fields.iter().map(Field::value).collect::<Vec<_>>() != self.initial
    }

    pub(super) fn confirming_discard(&self) -> bool {
        self.discard
    }

    pub(super) fn paste(&mut self, text: &str) -> Result<(), String> {
        let field = &mut self.fields[self.focus];
        if field.readonly || !field.options.is_empty() {
            return Ok(());
        }
        if !field.multiline && text.contains(['\n', '\r']) {
            return Err("This field accepts one line; paste was not inserted.".into());
        }
        field.area.insert_str(text);
        Ok(())
    }

    pub(super) fn key(&mut self, key: KeyEvent) -> FormAction {
        if self.discard {
            match key.code {
                KeyCode::Char('y') => return FormAction::Cancel,
                KeyCode::Char('n') | KeyCode::Esc | KeyCode::Enter => self.discard = false,
                _ => {}
            }
            return FormAction::Continue;
        }
        match key.code {
            KeyCode::Esc => {
                if self.is_dirty() {
                    self.discard = true;
                } else {
                    return FormAction::Cancel;
                }
            }
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return FormAction::Submit;
            }
            KeyCode::Tab => self.focus = (self.focus + 1) % self.fields.len(),
            KeyCode::BackTab => {
                self.focus = (self.focus + self.fields.len() - 1) % self.fields.len();
            }
            KeyCode::Enter if !self.fields[self.focus].multiline => {
                self.focus = (self.focus + 1) % self.fields.len();
            }
            _ => self.input(key),
        }
        FormAction::Continue
    }

    fn input(&mut self, key: KeyEvent) {
        let field = &mut self.fields[self.focus];
        if field.readonly {
            return;
        }
        if field.options.is_empty() {
            field.area.input(key);
            return;
        }
        let index = field
            .options
            .iter()
            .position(|value| *value == field.value())
            .unwrap_or(0);
        let index = match key.code {
            KeyCode::Left | KeyCode::Up => (index + field.options.len() - 1) % field.options.len(),
            KeyCode::Right | KeyCode::Down | KeyCode::Char(' ') => {
                (index + 1) % field.options.len()
            }
            _ => return,
        };
        field.set_value(&field.options[index].clone());
    }

    pub(super) fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        if self.discard {
            frame.render_widget(
                Paragraph::new(
                    "Discard this form's changes?\n\ny: discard | enter/esc/n: keep editing",
                )
                .block(Block::bordered().title("Discard form")),
                area,
            );
            return;
        }
        let [left, right] =
            Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)])
                .areas(area);
        self.focus = self.focus.min(self.fields.len().saturating_sub(1));
        self.list.select(Some(self.focus));
        let items = self
            .fields
            .iter()
            .map(|field| ListItem::new(field.label.clone()))
            .collect::<Vec<_>>();
        frame.render_stateful_widget(
            List::new(items)
                .block(Block::bordered().title(self.title.as_str()))
                .highlight_style(Style::default().fg(Color::Black).bg(Color::Yellow)),
            left,
            &mut self.list,
        );
        let field = &mut self.fields[self.focus];
        if field.readonly {
            frame.render_widget(
                Paragraph::new(field.value())
                    .wrap(Wrap { trim: false })
                    .block(Block::bordered().title(format!("{} (fixed)", field.label))),
                right,
            );
        } else if field.options.is_empty() {
            field
                .area
                .set_block(Block::bordered().title(field.label.clone()));
            frame.render_widget(&field.area, right);
        } else {
            frame.render_widget(
                Paragraph::new(format!(
                    "{}\n\n← / →: choose\n{}",
                    field.value(),
                    field.options.join(" | ")
                ))
                .wrap(Wrap { trim: false })
                .block(Block::bordered().title(field.label.as_str())),
                right,
            );
        }
    }
}
