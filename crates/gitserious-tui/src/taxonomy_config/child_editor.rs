use std::borrow::Cow;

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Cell, Paragraph, Row, Table, TableState, Widget};
use tui_textarea::{CursorMove, CursorRenderMode, TextArea, WrapMode};

use super::naming::{NameField, edited_prose, name_error, name_heading_line, single_line};
use super::{
    ConditionId, CreateFocus, CreateTextEditors, PropertyCondition, PropertyDraft,
    PropertyMultiplicity, PropertyRequirement, TypeDraft, contains, policy_label, render_counter,
    render_metadata_frame, taxonomy_heading_line,
};
use crate::theme::{JET_BLACK, focus_frame_style, row_style, selected_style};

const TYPE_LIMIT: usize = 80;
const PROPERTY_LIMIT: usize = 100;
const ADD_PROPERTY: &str = "+ add property";

pub(super) enum ChildAction {
    Continue,
    Complete,
    Back,
    AddProperty,
    EditProperty(usize),
    Error(String),
}

pub(super) struct TypeEditorState {
    pub target: Option<usize>,
    pub draft: TypeDraft,
    pub text: CreateTextEditors,
    pub focus: CreateFocus,
    pub selected: usize,
    pub areas: [Rect; 2],
    pub offset: usize,
    initial: TypeDraft,
}

impl TypeEditorState {
    pub fn new(target: Option<usize>, draft: TypeDraft) -> Self {
        Self {
            target,
            text: CreateTextEditors::from_values(
                &draft.id,
                &draft.description,
                TYPE_LIMIT,
                target.is_none(),
            ),
            initial: draft.clone(),
            draft,
            focus: CreateFocus::Name,
            selected: 0,
            areas: [Rect::default(); 2],
            offset: 0,
        }
    }

    pub fn candidate(&self) -> Result<TypeDraft, String> {
        if self.text.used() > TYPE_LIMIT {
            return Err("Type name and description are limited to 80 characters.".into());
        }
        let mut candidate = self.current();
        candidate.description = edited_prose(candidate.description, &self.initial.description);
        candidate
            .build()
            .map(|definition| TypeDraft::from(&definition))
    }

    fn current(&self) -> TypeDraft {
        let mut draft = self.draft.clone();
        draft.id = self.text.name.lines().join("\n");
        draft.description = self.text.description.lines().join("\n");
        draft
    }

    pub fn is_dirty(&self) -> bool {
        self.current() != self.initial
    }

    pub fn paste(&mut self, text: &str) -> ChildAction {
        if self.focus == CreateFocus::Types {
            return ChildAction::Continue;
        }
        paste_metadata(&mut self.text, self.focus, text, "Type", TYPE_LIMIT)
    }

    pub fn key(&mut self, key: KeyEvent) -> ChildAction {
        if key.code == KeyCode::Esc {
            return ChildAction::Back;
        }
        if key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return ChildAction::Complete;
        }
        if key.code == KeyCode::Char('n') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return ChildAction::AddProperty;
        }
        if self.focus == CreateFocus::Types {
            let count = self.draft.properties.len() + 1;
            match key.code {
                KeyCode::Up if key.modifiers.contains(KeyModifiers::ALT) && self.selected > 1 => {
                    self.draft
                        .properties
                        .swap(self.selected - 1, self.selected - 2);
                    self.selected -= 1;
                }
                KeyCode::Down
                    if key.modifiers.contains(KeyModifiers::ALT)
                        && self.selected > 0
                        && self.selected < count - 1 =>
                {
                    self.draft.properties.swap(self.selected - 1, self.selected);
                    self.selected += 1;
                }
                KeyCode::Up if self.selected == 0 => self.focus = CreateFocus::Description,
                KeyCode::Up => self.selected -= 1,
                KeyCode::Down => self.selected = (self.selected + 1).min(count - 1),
                KeyCode::Enter if self.selected == 0 => return ChildAction::AddProperty,
                KeyCode::Enter => return ChildAction::EditProperty(self.selected - 1),
                KeyCode::Char('n') if key.modifiers.is_empty() => return ChildAction::AddProperty,
                KeyCode::Char('d') if key.modifiers.is_empty() && self.selected > 0 => {
                    self.draft.properties.remove(self.selected - 1);
                    self.selected -= 1;
                }
                _ => {}
            }
            return ChildAction::Continue;
        }
        match key.code {
            KeyCode::Down if self.focus == CreateFocus::Name => {
                self.focus = CreateFocus::Description
            }
            KeyCode::Up | KeyCode::Down if self.focus == CreateFocus::Description => {
                let before = self.text.description.cursor();
                self.text
                    .description
                    .move_cursor(if key.code == KeyCode::Up {
                        CursorMove::Up
                    } else {
                        CursorMove::Down
                    });
                if self.text.description.cursor() == before {
                    self.focus = if key.code == KeyCode::Up {
                        CreateFocus::Name
                    } else {
                        CreateFocus::Types
                    };
                }
            }
            KeyCode::Enter
                if key.modifiers.contains(KeyModifiers::ALT)
                    && self.focus == CreateFocus::Description =>
            {
                return self.paste("\n");
            }
            KeyCode::Enter if key.modifiers.is_empty() => {
                self.focus = if self.focus == CreateFocus::Name {
                    CreateFocus::Description
                } else {
                    CreateFocus::Types
                };
            }
            _ if text_key(key)
                && !self.text.edit(self.focus, |area| {
                    area.input(key);
                }) =>
            {
                return limit_error("Type", TYPE_LIMIT);
            }
            _ => {}
        }
        ChildAction::Continue
    }

    pub fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let [metadata, properties] =
            Layout::vertical([Constraint::Length(8), Constraint::Min(4)]).areas(area);
        self.areas = [metadata, properties];
        render_metadata_frame(frame, metadata, "Type", &mut self.text, self.focus);
        let focused = self.focus == CreateFocus::Types;
        let mut rows = vec![
            Row::new(vec![
                Cell::from(if self.selected == 0 { "›" } else { " " }),
                Cell::from(ADD_PROPERTY),
                Cell::from(""),
                Cell::from(""),
            ])
            .style(row_style(0)),
        ];
        rows.extend(
            self.draft
                .properties
                .iter()
                .enumerate()
                .map(|(index, property)| {
                    Row::new(vec![
                        Cell::from(if self.selected == index + 1 {
                            "›"
                        } else {
                            " "
                        }),
                        Cell::from(property.key.as_str()),
                        Cell::from(property.description.as_str()),
                        Cell::from(policy_label(&property.requirement, property.multiplicity)),
                    ])
                    .style(row_style(index + 1))
                }),
        );
        let name_width = self
            .draft
            .properties
            .iter()
            .map(|property| property.key.len())
            .fold(ADD_PROPERTY.len(), usize::max);
        let name_width = u16::try_from(name_width)
            .unwrap_or(u16::MAX)
            .min(properties.width.saturating_sub(2) / 3);
        let policy_width = self
            .draft
            .properties
            .iter()
            .map(|property| policy_label(&property.requirement, property.multiplicity).len())
            .fold("recommended".len(), usize::max);
        let policy_width = u16::try_from(policy_width).unwrap_or(u16::MAX);
        let mut state = TableState::default();
        state.select(Some(self.selected.min(rows.len() - 1)));
        let style = focus_frame_style(focused);
        frame.render_stateful_widget(
            Table::new(
                rows,
                [
                    Constraint::Length(1),
                    Constraint::Length(name_width),
                    Constraint::Min(1),
                    Constraint::Length(policy_width),
                ],
            )
            .column_spacing(2)
            .block(
                Block::bordered()
                    .title("Properties")
                    .border_style(style)
                    .title_style(style),
            )
            .row_highlight_style(selected_style(focused)),
            properties,
            &mut state,
        );
        self.offset = state.offset();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PropertyFocus {
    Name,
    Description,
    Policy,
    ConditionName,
    ConditionRationale,
    Multiplicity,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Policy {
    Required,
    Recommended,
    Conditional,
    Optional,
}

impl Policy {
    const ORDER: [Self; 4] = [
        Self::Required,
        Self::Recommended,
        Self::Conditional,
        Self::Optional,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Required => "required",
            Self::Recommended => "recommended",
            Self::Conditional => "conditional",
            Self::Optional => "optional",
        }
    }
}

const MULTIPLICITIES: [PropertyMultiplicity; 2] =
    [PropertyMultiplicity::Single, PropertyMultiplicity::Multiple];

const fn multiplicity_label(value: PropertyMultiplicity) -> &'static str {
    match value {
        PropertyMultiplicity::Single => "single",
        PropertyMultiplicity::Multiple => "multiple",
    }
}

fn cycle<T: Copy + PartialEq>(order: &[T], current: T, forward: bool) -> T {
    let step = if forward { 1 } else { order.len() - 1 };
    order
        .iter()
        .position(|value| *value == current)
        .map_or(current, |index| order[(index + step) % order.len()])
}

pub(super) struct PropertyEditorState {
    pub target: Option<usize>,
    pub text: CreateTextEditors,
    pub focus: PropertyFocus,
    pub condition_name: TextArea<'static>,
    pub condition_rationale: TextArea<'static>,
    policy: Policy,
    multiplicity: PropertyMultiplicity,
    viewport_top: u16,
    initial: PropertyFields,
}

#[derive(Debug, Eq, PartialEq)]
struct PropertyFields {
    name: String,
    description: String,
    policy: Policy,
    multiplicity: PropertyMultiplicity,
    condition_name: String,
    condition_rationale: String,
}

impl PropertyEditorState {
    pub fn new(target: Option<usize>, draft: PropertyDraft) -> Self {
        let (policy, name, rationale) = match &draft.requirement {
            PropertyRequirement::Required => (Policy::Required, String::new(), String::new()),
            PropertyRequirement::Recommended => (Policy::Recommended, String::new(), String::new()),
            PropertyRequirement::Optional => (Policy::Optional, String::new(), String::new()),
            PropertyRequirement::Conditional(condition) => (
                Policy::Conditional,
                condition.id().to_string(),
                condition.rationale().to_owned(),
            ),
        };
        let mut condition_name = TextArea::new(vec![name.clone()]);
        let mut condition_rationale =
            TextArea::new(rationale.split('\n').map(str::to_owned).collect());
        for area in [&mut condition_name, &mut condition_rationale] {
            area.set_style(Style::default().fg(Color::White).bg(JET_BLACK));
            area.set_cursor_line_style(Style::default());
            area.set_cursor_render_mode(CursorRenderMode::Hidden);
        }
        condition_rationale.set_wrap_mode(WrapMode::WordOrGlyph);
        Self {
            target,
            text: CreateTextEditors::from_values(
                &draft.key,
                &draft.description,
                PROPERTY_LIMIT,
                target.is_none(),
            ),
            focus: PropertyFocus::Name,
            condition_name,
            condition_rationale,
            policy,
            multiplicity: draft.multiplicity,
            viewport_top: 0,
            initial: PropertyFields {
                name: draft.key,
                description: draft.description,
                policy,
                multiplicity: draft.multiplicity,
                condition_name: name,
                condition_rationale: rationale,
            },
        }
    }

    pub fn candidate(&self) -> Result<PropertyDraft, String> {
        if self.text.used() > PROPERTY_LIMIT {
            return Err("Property name and description are limited to 100 characters.".into());
        }
        let requirement = match self.policy {
            Policy::Required => PropertyRequirement::Required,
            Policy::Recommended => PropertyRequirement::Recommended,
            Policy::Optional => PropertyRequirement::Optional,
            Policy::Conditional => {
                let name = self.condition_name.lines().join("\n");
                let id = ConditionId::from_label(&name)
                    .map_err(|error| name_error(NameField::Condition, &name, &error))?;
                let rationale = edited_prose(
                    self.condition_rationale.lines().join("\n"),
                    &self.initial.condition_rationale,
                );
                PropertyRequirement::Conditional(
                    PropertyCondition::new(id, rationale)
                        .map_err(|error| format!("Condition rationale: {error}"))?,
                )
            }
        };
        let candidate = PropertyDraft {
            key: self.text.name.lines().join("\n"),
            description: edited_prose(
                self.text.description.lines().join("\n"),
                &self.initial.description,
            ),
            requirement,
            multiplicity: self.multiplicity,
        };
        candidate
            .build()
            .map(|definition| PropertyDraft::from(&definition))
    }

    pub fn is_dirty(&self) -> bool {
        PropertyFields {
            name: self.text.name.lines().join("\n"),
            description: self.text.description.lines().join("\n"),
            policy: self.policy,
            multiplicity: self.multiplicity,
            condition_name: self.condition_name.lines().join("\n"),
            condition_rationale: self.condition_rationale.lines().join("\n"),
        } != self.initial
    }

    pub fn paste(&mut self, text: &str) -> ChildAction {
        match self.focus {
            PropertyFocus::Name => paste_metadata(
                &mut self.text,
                CreateFocus::Name,
                text,
                "Property",
                PROPERTY_LIMIT,
            ),
            PropertyFocus::Description => paste_metadata(
                &mut self.text,
                CreateFocus::Description,
                text,
                "Property",
                PROPERTY_LIMIT,
            ),
            PropertyFocus::ConditionName => {
                self.condition_name.insert_str(single_line(text));
                ChildAction::Continue
            }
            PropertyFocus::ConditionRationale => {
                self.condition_rationale.insert_str(text);
                ChildAction::Continue
            }
            PropertyFocus::Policy | PropertyFocus::Multiplicity => ChildAction::Continue,
        }
    }

    pub fn key(&mut self, key: KeyEvent) -> ChildAction {
        if key.code == KeyCode::Esc {
            return ChildAction::Back;
        }
        if key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return ChildAction::Complete;
        }
        match key.code {
            KeyCode::Left | KeyCode::Right if self.focus == PropertyFocus::Policy => {
                self.policy = cycle(&Policy::ORDER, self.policy, key.code == KeyCode::Right)
            }
            KeyCode::Left | KeyCode::Right if self.focus == PropertyFocus::Multiplicity => {
                self.multiplicity = cycle(
                    &MULTIPLICITIES,
                    self.multiplicity,
                    key.code == KeyCode::Right,
                )
            }
            KeyCode::Enter
                if key.modifiers.contains(KeyModifiers::ALT)
                    && matches!(
                        self.focus,
                        PropertyFocus::Description | PropertyFocus::ConditionRationale
                    ) =>
            {
                return self.paste("\n");
            }
            KeyCode::Enter if key.modifiers.is_empty() => self.next_section(),
            KeyCode::Up | KeyCode::Down => self.vertical(key.code == KeyCode::Down),
            _ if text_key(key) => {
                let focus = match self.focus {
                    PropertyFocus::Name => Some(CreateFocus::Name),
                    PropertyFocus::Description => Some(CreateFocus::Description),
                    _ => None,
                };
                if let Some(focus) = focus {
                    if !self.text.edit(focus, |area| {
                        area.input(key);
                    }) {
                        return limit_error("Property", PROPERTY_LIMIT);
                    }
                } else {
                    match self.focus {
                        PropertyFocus::ConditionName => {
                            self.condition_name.input(key);
                        }
                        PropertyFocus::ConditionRationale => {
                            self.condition_rationale.input(key);
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        ChildAction::Continue
    }

    fn next_section(&mut self) {
        self.focus = match self.focus {
            PropertyFocus::Name => PropertyFocus::Description,
            PropertyFocus::Description => PropertyFocus::Policy,
            PropertyFocus::Policy if self.policy == Policy::Conditional => {
                PropertyFocus::ConditionName
            }
            PropertyFocus::ConditionName => PropertyFocus::ConditionRationale,
            PropertyFocus::Policy
            | PropertyFocus::ConditionRationale
            | PropertyFocus::Multiplicity => PropertyFocus::Multiplicity,
        };
    }

    fn vertical(&mut self, down: bool) {
        match self.focus {
            PropertyFocus::Name if down => self.focus = PropertyFocus::Description,
            PropertyFocus::Description => {
                let before = self.text.description.cursor();
                self.text.description.move_cursor(if down {
                    CursorMove::Down
                } else {
                    CursorMove::Up
                });
                if self.text.description.cursor() == before {
                    self.focus = if down {
                        PropertyFocus::Policy
                    } else {
                        PropertyFocus::Name
                    };
                }
            }
            PropertyFocus::Policy if !down => self.focus = PropertyFocus::Description,
            PropertyFocus::Policy if self.policy == Policy::Conditional => {
                self.focus = PropertyFocus::ConditionName
            }
            PropertyFocus::Policy => self.focus = PropertyFocus::Multiplicity,
            PropertyFocus::ConditionName => {
                self.focus = if down {
                    PropertyFocus::ConditionRationale
                } else {
                    PropertyFocus::Policy
                }
            }
            PropertyFocus::ConditionRationale => {
                let before = self.condition_rationale.cursor();
                self.condition_rationale.move_cursor(if down {
                    CursorMove::Down
                } else {
                    CursorMove::Up
                });
                if self.condition_rationale.cursor() == before {
                    self.focus = if down {
                        PropertyFocus::Multiplicity
                    } else {
                        PropertyFocus::ConditionName
                    };
                }
            }
            PropertyFocus::Multiplicity if !down => {
                self.focus = if self.policy == Policy::Conditional {
                    PropertyFocus::ConditionRationale
                } else {
                    PropertyFocus::Policy
                }
            }
            PropertyFocus::Name | PropertyFocus::Multiplicity => {}
        }
    }

    pub fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        frame.render_widget(
            Block::default().style(Style::default().fg(Color::White).bg(JET_BLACK)),
            area,
        );
        let conditional = self.policy == Policy::Conditional;
        let multiplicity_row: u16 = if conditional { 16 } else { 9 };
        let content_height = multiplicity_row + 2;
        let desired_height = content_height + 2;
        let pane = Rect::new(area.x, area.y, area.width, desired_height.min(area.height));
        let style = focus_frame_style(true);
        frame.render_widget(
            Block::bordered()
                .title("Property")
                .border_style(style)
                .title_style(style),
            pane,
        );
        let width = pane.width.saturating_sub(2);
        let viewport_height = pane.height.saturating_sub(2);
        if width == 0 || viewport_height == 0 {
            return;
        }
        let (start, end): (u16, u16) = match self.focus {
            PropertyFocus::Name => (0, 1),
            PropertyFocus::Description => (3, 5),
            PropertyFocus::Policy => (6, 7),
            PropertyFocus::ConditionName => (9, 10),
            PropertyFocus::ConditionRationale => (12, 14),
            PropertyFocus::Multiplicity => (multiplicity_row, multiplicity_row + 1),
        };
        self.viewport_top = self
            .viewport_top
            .min(content_height.saturating_sub(viewport_height));
        if start < self.viewport_top {
            self.viewport_top = start;
        }
        if end >= self.viewport_top + viewport_height {
            self.viewport_top = (end + 1).saturating_sub(viewport_height);
        }
        self.viewport_top = self
            .viewport_top
            .min(content_height.saturating_sub(viewport_height));

        let mut content = Buffer::empty(Rect::new(0, 0, width, content_height));
        for cell in &mut content.content {
            cell.set_style(Style::default().fg(Color::White).bg(JET_BLACK));
        }
        Paragraph::new(name_heading_line(
            "Name",
            &self.text.name.lines().join("\n"),
            width,
        ))
        .render(Rect::new(0, 0, width, 1), &mut content);
        for (y, label) in [
            (3, "Description"),
            (6, "Policy"),
            (multiplicity_row, "Multiplicity"),
        ] {
            Paragraph::new(taxonomy_heading_line(label, width))
                .render(Rect::new(0, y, width, 1), &mut content);
        }
        render_property_input(
            &mut content,
            &self.text.name,
            Rect::new(0, 1, width, 1),
            self.focus == PropertyFocus::Name,
        );
        render_property_input(
            &mut content,
            &self.text.description,
            Rect::new(0, 4, width, 2),
            self.focus == PropertyFocus::Description,
        );
        render_choice(
            &mut content,
            Rect::new(0, 7, width, 1),
            self.policy.label(),
            self.focus == PropertyFocus::Policy,
        );
        render_choice(
            &mut content,
            Rect::new(0, multiplicity_row + 1, width, 1),
            multiplicity_label(self.multiplicity),
            self.focus == PropertyFocus::Multiplicity,
        );
        if conditional {
            Paragraph::new(name_heading_line(
                "Condition name",
                &self.condition_name.lines().join("\n"),
                width,
            ))
            .render(Rect::new(0, 9, width, 1), &mut content);
            Paragraph::new(taxonomy_heading_line("Condition rationale", width))
                .render(Rect::new(0, 12, width, 1), &mut content);
            render_property_input(
                &mut content,
                &self.condition_name,
                Rect::new(0, 10, width, 1),
                self.focus == PropertyFocus::ConditionName,
            );
            render_property_input(
                &mut content,
                &self.condition_rationale,
                Rect::new(0, 13, width, 2),
                self.focus == PropertyFocus::ConditionRationale,
            );
        }
        for y in 0..viewport_height {
            for x in 0..width {
                frame.buffer_mut()[(pane.x + 1 + x, pane.y + 1 + y)] =
                    content[(x, self.viewport_top + y)].clone();
            }
        }
        render_counter(frame, pane, self.text.used(), PROPERTY_LIMIT);
    }
}

fn render_choice(buffer: &mut Buffer, area: Rect, label: &str, focused: bool) {
    let style = Style::default().fg(Color::White);
    let choice = if focused {
        Span::styled(
            format!("‹ {label} ›"),
            style.add_modifier(Modifier::REVERSED),
        )
    } else {
        Span::styled(label, style)
    };
    Paragraph::new(choice).render(area, buffer);
}

fn render_property_input(buffer: &mut Buffer, text: &TextArea<'_>, area: Rect, focused: bool) {
    Widget::render(text, area, buffer);
    if focused
        && let Some(cursor) = text.rendered_cursor_position()
        && contains(area, cursor.x, cursor.y)
    {
        buffer[(cursor.x, cursor.y)].set_style(Style::default().add_modifier(Modifier::REVERSED));
    }
}

fn text_key(key: KeyEvent) -> bool {
    matches!(
        key.code,
        KeyCode::Backspace
            | KeyCode::Delete
            | KeyCode::Left
            | KeyCode::Right
            | KeyCode::PageUp
            | KeyCode::PageDown
    ) || matches!(key.code, KeyCode::Char(_) if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT)
}

fn limit_error(owner: &str, limit: usize) -> ChildAction {
    ChildAction::Error(format!(
        "{owner} name and description are limited to {limit} characters."
    ))
}

fn paste_metadata(
    text: &mut CreateTextEditors,
    focus: CreateFocus,
    value: &str,
    owner: &str,
    limit: usize,
) -> ChildAction {
    let value = if focus == CreateFocus::Name {
        single_line(value)
    } else {
        Cow::Borrowed(value)
    };
    if text.paste(focus, &value) {
        limit_error(owner, limit)
    } else {
        ChildAction::Continue
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn new_text_limits_are_independent_and_copied_metadata_is_editable()
    -> Result<(), Box<dyn std::error::Error>> {
        let draft = TypeDraft {
            id: "feat".into(),
            description: "x".repeat(76),
            schema_version: 3,
            properties: Vec::new(),
        };
        let mut new_type = TypeEditorState::new(None, draft.clone());
        new_type.focus = CreateFocus::Description;
        assert!(matches!(
            new_type.key(key(KeyCode::Char('x'))),
            ChildAction::Error(_)
        ));
        assert_eq!(new_type.text.used(), 80);
        let mut copied = TypeEditorState::new(
            Some(0),
            TypeDraft {
                description: "x".repeat(120),
                ..draft
            },
        );
        copied.focus = CreateFocus::Description;
        assert!(matches!(copied.paste("extra"), ChildAction::Continue));
        assert!(copied.candidate().is_err());
        copied.text.description.select_all();
        copied.paste("Edited feature");
        assert_eq!(copied.candidate()?.schema_version, 3);

        let mut property = PropertyEditorState::new(
            None,
            PropertyDraft {
                key: "intent".into(),
                description: "x".repeat(94),
                requirement: PropertyRequirement::Required,
                multiplicity: PropertyMultiplicity::Single,
            },
        );
        property.focus = PropertyFocus::Description;
        assert!(matches!(property.paste("extra"), ChildAction::Error(_)));
        assert_eq!(property.text.used(), 100);
        assert!(property.candidate().is_ok());
        property.text.description.move_cursor(CursorMove::End);
        property.key(key(KeyCode::Backspace));
        property.paste("AB");
        assert_eq!(property.text.used(), 100);
        assert!(property.text.description.lines()[0].ends_with('A'));
        Ok(())
    }

    #[test]
    fn conditional_fields_are_required_and_copied_policy_data_is_preserved()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut property = PropertyEditorState::new(None, PropertyDraft::empty());
        property.paste("evidence");
        property.key(key(KeyCode::Enter));
        property.paste("Supporting results");
        property.key(key(KeyCode::Enter));
        property.key(key(KeyCode::Right));
        assert!(matches!(
            property.candidate()?.requirement,
            PropertyRequirement::Recommended
        ));
        property.key(key(KeyCode::Right));
        assert!(property.candidate().is_err());
        property.key(key(KeyCode::Enter));
        property.paste("behavior-changed");
        assert!(property.candidate().is_err());
        property.key(key(KeyCode::Enter));
        property.paste(&"Explain when evidence is required. ".repeat(20));
        let PropertyRequirement::Conditional(condition) = property.candidate()?.requirement else {
            return Err("missing condition".into());
        };
        assert_eq!(condition.id().as_str(), "behavior-changed");
        assert!(condition.rationale().len() > 100);
        assert_eq!(property.text.used(), "evidenceSupporting results".len());

        let mut copied = PropertyEditorState::new(
            Some(0),
            PropertyDraft {
                key: "notes".into(),
                description: "x".repeat(140),
                requirement: PropertyRequirement::Optional,
                multiplicity: PropertyMultiplicity::Multiple,
            },
        );
        copied.focus = PropertyFocus::Description;
        assert!(matches!(copied.paste("extra"), ChildAction::Continue));
        assert!(copied.candidate().is_err());
        copied.text.description.select_all();
        copied.paste("Edited notes");
        let candidate = copied.candidate()?;
        assert_eq!(candidate.requirement, PropertyRequirement::Optional);
        assert_eq!(candidate.multiplicity, PropertyMultiplicity::Multiple);
        copied.focus = PropertyFocus::Policy;
        copied.key(key(KeyCode::Right));
        assert_eq!(
            copied.candidate()?.requirement,
            PropertyRequirement::Required
        );
        Ok(())
    }

    #[test]
    fn policy_arrows_reach_all_four_levels_and_left_undoes_right()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut property = PropertyEditorState::new(None, PropertyDraft::empty());
        property.focus = PropertyFocus::Policy;
        let mut walk = |code| {
            (0..4)
                .map(|_| {
                    property.key(key(code));
                    property.policy.label()
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            walk(KeyCode::Right),
            ["recommended", "conditional", "optional", "required"]
        );
        assert_eq!(
            walk(KeyCode::Left),
            ["optional", "conditional", "recommended", "required"]
        );

        let mut loaded = PropertyEditorState::new(
            Some(0),
            PropertyDraft {
                key: "notes".into(),
                description: "Free-form notes".into(),
                requirement: PropertyRequirement::Optional,
                multiplicity: PropertyMultiplicity::Single,
            },
        );
        loaded.focus = PropertyFocus::Policy;
        for _ in 0..4 {
            let start = loaded.policy;
            loaded.key(key(KeyCode::Left));
            loaded.key(key(KeyCode::Right));
            assert_eq!(loaded.policy, start);
            loaded.key(key(KeyCode::Right));
        }
        assert_eq!(
            loaded.candidate()?.requirement,
            PropertyRequirement::Optional
        );
        Ok(())
    }

    #[test]
    fn multiplicity_row_toggles_into_the_completed_draft_and_counts_as_a_change()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut property = PropertyEditorState::new(None, PropertyDraft::empty());
        property.paste("notes");
        property.key(key(KeyCode::Enter));
        property.paste("Free-form notes");
        property.key(key(KeyCode::Enter));
        property.key(key(KeyCode::Enter));
        assert_eq!(property.focus, PropertyFocus::Multiplicity);
        property.key(key(KeyCode::Right));
        assert_eq!(
            property.candidate()?.multiplicity,
            PropertyMultiplicity::Multiple
        );
        property.key(key(KeyCode::Left));
        assert_eq!(
            property.candidate()?.multiplicity,
            PropertyMultiplicity::Single
        );
        property.key(key(KeyCode::Left));
        property.key(key(KeyCode::Up));
        property.key(key(KeyCode::Left));
        assert_eq!(
            property.candidate()?,
            PropertyDraft {
                key: "notes".into(),
                description: "Free-form notes".into(),
                requirement: PropertyRequirement::Optional,
                multiplicity: PropertyMultiplicity::Multiple,
            }
        );

        let draft = PropertyDraft {
            key: "evidence".into(),
            description: "Supporting results".into(),
            requirement: PropertyRequirement::Conditional(PropertyCondition::new(
                ConditionId::new("changed")?,
                "first\nsecond",
            )?),
            multiplicity: PropertyMultiplicity::Single,
        };
        let mut loaded = PropertyEditorState::new(Some(0), draft.clone());
        loaded.focus = PropertyFocus::ConditionRationale;
        loaded.key(key(KeyCode::Down));
        assert_eq!(loaded.focus, PropertyFocus::ConditionRationale);
        loaded.key(key(KeyCode::Down));
        assert_eq!(loaded.focus, PropertyFocus::Multiplicity);
        assert!(!loaded.is_dirty());
        loaded.key(key(KeyCode::Right));
        assert!(loaded.is_dirty());
        assert_eq!(
            loaded.candidate()?,
            PropertyDraft {
                multiplicity: PropertyMultiplicity::Multiple,
                ..draft.clone()
            }
        );
        loaded.key(key(KeyCode::Right));
        assert!(!loaded.is_dirty());
        loaded.key(key(KeyCode::Up));
        assert_eq!(loaded.focus, PropertyFocus::ConditionRationale);
        assert_eq!(loaded.candidate()?, draft);
        Ok(())
    }

    #[test]
    fn conditional_property_fits_compact_and_wide_views_with_a_single_cursor()
    -> Result<(), Box<dyn std::error::Error>> {
        let draft = PropertyDraft {
            key: "intent".into(),
            description: "why".into(),
            requirement: PropertyRequirement::Conditional(PropertyCondition::new(
                ConditionId::new("changed")?,
                "long rationale\n".repeat(20),
            )?),
            multiplicity: PropertyMultiplicity::Single,
        };
        let mut property = PropertyEditorState::new(Some(0), draft.clone());
        assert_eq!(property.candidate()?, draft);
        property.focus = PropertyFocus::ConditionRationale;
        property.condition_rationale.move_cursor(CursorMove::Bottom);
        let data_cursor = property.condition_rationale.cursor();
        for (width, height) in [(60, 14), (120, 26)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height))?;
            terminal.draw(|frame| {
                let area = frame.area();
                property.render(frame, area);
            })?;
            let pane_height = height.min(20);
            let buffer = terminal.backend().buffer();
            let row = |y| {
                (1..width - 1)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            };
            assert_eq!(property.viewport_top, if height < 20 { 3 } else { 0 });
            assert_eq!(buffer[(0, 0)].fg, Color::Yellow);
            assert_eq!(buffer[(0, pane_height - 1)].symbol(), "└");
            for (offset, label) in [
                (1_u16, "Name"),
                (4, "Description"),
                (7, "Policy"),
                (10, "Condition name"),
                (13, "Condition rationale"),
                (17, "Multiplicity"),
            ] {
                if let Some(y) = offset.checked_sub(property.viewport_top)
                    && y > 0
                    && y < pane_height - 1
                {
                    assert_eq!(buffer[(1, y)].fg, Color::White);
                    assert!(row(y).starts_with(label));
                }
            }
            for offset in [9_u16, 12, 16] {
                if let Some(y) = offset.checked_sub(property.viewport_top)
                    && y < pane_height - 1
                {
                    assert!(row(y).trim().is_empty());
                }
            }
            assert!(row(pane_height - 1).contains("9/100"));
            assert_eq!(
                buffer
                    .content
                    .iter()
                    .filter(|cell| cell.modifier.contains(Modifier::REVERSED))
                    .count(),
                1
            );
            let cursor = property
                .condition_rationale
                .rendered_cursor_position()
                .ok_or("missing cursor")?;
            let visible_cursor = (1 + cursor.x, 1 + cursor.y - property.viewport_top);
            assert!(
                visible_cursor.1 >= 14 - property.viewport_top
                    && visible_cursor.1 <= 15 - property.viewport_top
            );
            assert!(buffer[visible_cursor].modifier.contains(Modifier::REVERSED));
            assert_eq!(property.condition_rationale.cursor(), data_cursor);
            for y in pane_height..height {
                assert!(row(y).trim().is_empty());
                assert_eq!(buffer[(0, y)].bg, JET_BLACK);
            }
        }
        Ok(())
    }

    #[test]
    fn compact_property_navigation_policy_changes_and_resize_keep_the_focus_visible()
    -> Result<(), Box<dyn std::error::Error>> {
        let draft = PropertyDraft {
            key: "intent".into(),
            description: "why".into(),
            requirement: PropertyRequirement::Conditional(PropertyCondition::new(
                ConditionId::new("changed")?,
                "first\nsecond\nthird",
            )?),
            multiplicity: PropertyMultiplicity::Single,
        };
        let mut property = PropertyEditorState::new(Some(0), draft.clone());
        let mut compact = Terminal::new(TestBackend::new(60, 18))?;
        let draw_compact = |property: &mut PropertyEditorState,
                            terminal: &mut Terminal<TestBackend>| {
            terminal
                .draw(|frame| property.render(frame, Rect::new(0, 3, 60, 14)))
                .map(|_| ())
        };
        for focus in [
            PropertyFocus::Name,
            PropertyFocus::Description,
            PropertyFocus::Policy,
            PropertyFocus::ConditionName,
            PropertyFocus::ConditionRationale,
        ] {
            assert_eq!(property.focus, focus);
            draw_compact(&mut property, &mut compact)?;
            assert_eq!(
                property.viewport_top,
                if focus == PropertyFocus::ConditionRationale {
                    3
                } else {
                    0
                }
            );
            if focus != PropertyFocus::Policy {
                let positions = compact
                    .backend()
                    .buffer()
                    .content
                    .iter()
                    .enumerate()
                    .filter(|(_, cell)| cell.modifier.contains(Modifier::REVERSED))
                    .map(|(index, _)| index / 60)
                    .collect::<Vec<_>>();
                assert_eq!(positions.len(), 1);
                assert!(positions[0] > 3 && positions[0] < 16);
            }
            if focus != PropertyFocus::ConditionRationale {
                property.key(key(KeyCode::Down));
            }
        }
        let data_cursor = property.condition_rationale.cursor();
        property.key(key(KeyCode::Up));
        property.key(key(KeyCode::Up));
        assert_eq!(property.focus, PropertyFocus::Policy);
        property.key(key(KeyCode::Right));
        assert_eq!(property.policy, Policy::Optional);
        draw_compact(&mut property, &mut compact)?;
        assert_eq!(property.viewport_top, 0);
        assert_eq!(compact.backend().buffer()[(0, 15)].symbol(), "└");
        for y in 16..18 {
            assert!((0..60).all(|x| compact.backend().buffer()[(x, y)].symbol() == " "));
        }
        property.key(key(KeyCode::Right));
        assert_eq!(property.policy, Policy::Required);
        draw_compact(&mut property, &mut compact)?;
        assert_eq!(compact.backend().buffer()[(0, 15)].symbol(), "└");
        property.key(key(KeyCode::Left));
        property.key(key(KeyCode::Left));
        property.key(key(KeyCode::Down));
        property.key(key(KeyCode::Down));
        draw_compact(&mut property, &mut compact)?;
        assert_eq!(property.viewport_top, 3);
        let mut wide = Terminal::new(TestBackend::new(120, 30))?;
        wide.draw(|frame| property.render(frame, Rect::new(0, 3, 120, 26)))?;
        assert_eq!(property.viewport_top, 0);
        assert_eq!(wide.backend().buffer()[(0, 22)].symbol(), "└");
        assert_eq!(property.condition_rationale.cursor(), data_cursor);
        draw_compact(&mut property, &mut compact)?;
        assert_eq!(property.viewport_top, 3);
        assert_eq!(property.candidate()?, draft);
        Ok(())
    }
    #[test]
    fn loaded_prose_stays_byte_for_byte_and_edited_prose_is_trimmed()
    -> Result<(), Box<dyn std::error::Error>> {
        let draft = TypeDraft {
            id: "feat".into(),
            description: " Padded feature \n".into(),
            schema_version: 2,
            properties: Vec::new(),
        };
        let mut copied = TypeEditorState::new(Some(0), draft.clone());
        assert_eq!(copied.candidate()?, draft);
        copied.focus = CreateFocus::Description;
        copied.text.description.select_all();
        copied.paste(" Edited feature \n\n");
        assert_eq!(copied.candidate()?.description, "Edited feature");
        copied.text.description.select_all();
        copied.paste(" \n ");
        assert!(copied.candidate().is_err());
        Ok(())
    }
}
