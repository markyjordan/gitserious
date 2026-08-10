use gitserious_app::CommitDraftAuthorOutcome;
use gitserious_core::{
    AuthoredProperty, CommitDraft, CommitMessage, CommitScope, CommitSubject, CommitTypeDefinition,
    PropertyMultiplicity, PropertyRequirement, PropertyValue, PropertyValues,
    render_commit_message,
};
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::style::{Color, Modifier, Style};
use tui_textarea::{AtomicRange, CursorMove, Input, TextArea, WrapMode};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Stage {
    SelectType,
    Compose,
    Review,
    Confirm,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Keymap {
    Conventional,
    Vim,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VimMode {
    Normal,
    Insert,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ConfirmationAction {
    Cancel,
    ChangeType,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ResumeStage {
    Compose,
    Review,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FieldId {
    Scope,
    Subject,
    Property(usize),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FieldKind {
    Scope,
    Subject,
    Property {
        definition_index: usize,
        value_index: usize,
    },
}

impl FieldKind {
    pub(crate) const fn id(self) -> FieldId {
        match self {
            Self::Scope => FieldId::Scope,
            Self::Subject => FieldId::Subject,
            Self::Property {
                definition_index, ..
            } => FieldId::Property(definition_index),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FieldStatus {
    Incomplete,
    Complete,
    Invalid,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ValidationIssue {
    pub(crate) field: Option<FieldId>,
    pub(crate) line: usize,
    pub(crate) message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HudField {
    pub(crate) id: FieldId,
    pub(crate) status: FieldStatus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DocumentSection {
    kind: FieldKind,
    heading_line: usize,
    end_line: usize,
    text: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct ParsedDocument {
    sections: Vec<DocumentSection>,
    issues: Vec<ValidationIssue>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BoundaryKind {
    Known(FieldId),
    Invalid,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Boundary {
    line: usize,
    kind: BoundaryKind,
}

pub(crate) struct ComposerState {
    pub(crate) editor: TextArea<'static>,
    pristine: Vec<String>,
    pub(crate) issues: Vec<ValidationIssue>,
}

impl ComposerState {
    fn new(definition: &CommitTypeDefinition) -> Self {
        let pristine = scaffold_lines(definition);
        let mut editor = text_area(pristine.clone(), definition);
        editor.move_cursor(CursorMove::Jump(1, 0));
        Self {
            editor,
            pristine,
            issues: Vec::new(),
        }
    }

    pub(crate) fn dirty(&self) -> bool {
        self.editor.lines() != self.pristine
    }

    pub(crate) fn current_field(&self, definition: &CommitTypeDefinition) -> Option<FieldKind> {
        let cursor_line = self.editor.cursor().0;
        self.parse(definition)
            .sections
            .into_iter()
            .find(|section| cursor_line >= section.heading_line && cursor_line < section.end_line)
            .map(|section| section.kind)
    }

    pub(crate) fn hud_fields(&self, definition: &CommitTypeDefinition) -> Vec<HudField> {
        let parsed = self.parse(definition);
        let mut issues = parsed.issues;
        issues.extend(self.issues.iter().cloned());
        let mut fields = Vec::with_capacity(definition.properties().len() + 2);
        for id in std::iter::once(FieldId::Scope)
            .chain(std::iter::once(FieldId::Subject))
            .chain((0..definition.properties().len()).map(FieldId::Property))
        {
            let sections = parsed
                .sections
                .iter()
                .filter(|section| section.kind.id() == id)
                .collect::<Vec<_>>();
            let invalid = issues.iter().any(|issue| issue.field == Some(id))
                || sections.iter().any(|section| match id {
                    FieldId::Scope => {
                        !section.text.is_empty() && CommitScope::new(&section.text).is_err()
                    }
                    FieldId::Subject => {
                        !section.text.is_empty() && CommitSubject::new(&section.text).is_err()
                    }
                    FieldId::Property(_) => false,
                });
            let complete = sections.iter().any(|section| !section.text.is_empty());
            fields.push(HudField {
                id,
                status: if invalid {
                    FieldStatus::Invalid
                } else if complete {
                    FieldStatus::Complete
                } else {
                    FieldStatus::Incomplete
                },
            });
        }
        fields
    }

    fn parse(&self, definition: &CommitTypeDefinition) -> ParsedDocument {
        parse_document(self.editor.lines(), definition)
    }

    fn validate(
        &mut self,
        definition: &CommitTypeDefinition,
    ) -> Option<(CommitDraft, CommitMessage)> {
        let parsed = self.parse(definition);
        let mut issues = parsed.issues.clone();
        let scope = parse_scope(&parsed, &mut issues);
        let subject = parse_subject(&parsed, &mut issues);
        let authored = build_properties(&parsed, definition, &mut issues);

        if !issues.is_empty() {
            issues.sort_by_key(|issue| issue.line);
            let first_line = issues[0].line;
            self.issues = issues;
            self.editor
                .move_cursor(CursorMove::Jump(terminal_line(first_line), 0));
            return None;
        }
        let subject = subject?;
        let draft = match CommitDraft::new(definition.id().clone(), scope, subject, authored) {
            Ok(draft) => draft,
            Err(error) => {
                self.issues = vec![ValidationIssue {
                    field: self.current_field(definition).map(FieldKind::id),
                    line: self.editor.cursor().0,
                    message: error.to_string(),
                }];
                return None;
            }
        };
        match render_commit_message(definition, &draft) {
            Ok(message) => {
                self.issues.clear();
                Some((draft, message))
            }
            Err(errors) => {
                self.issues = errors
                    .as_slice()
                    .iter()
                    .map(|error| ValidationIssue {
                        field: self.current_field(definition).map(FieldKind::id),
                        line: self.editor.cursor().0,
                        message: error.to_string(),
                    })
                    .collect();
                None
            }
        }
    }

    fn edit_preserving_headings(
        &mut self,
        definition: &CommitTypeDefinition,
        edit: impl FnOnce(&mut TextArea<'static>),
    ) {
        let previous = self.editor.clone();
        let previous_cursor = self.editor.cursor();
        edit(&mut self.editor);
        if headings_are_intact(self.editor.lines(), definition) {
            apply_heading_guards(&mut self.editor, definition);
            skip_heading_cursor(&mut self.editor, definition, previous_cursor);
            self.issues.clear();
        } else {
            self.editor = previous;
        }
    }

    fn move_cursor(&mut self, definition: &CommitTypeDefinition, movement: CursorMove) {
        let previous_cursor = self.editor.cursor();
        self.editor.move_cursor(movement);
        skip_heading_cursor(&mut self.editor, definition, previous_cursor);
        self.issues.clear();
    }
}

fn text_area(lines: Vec<String>, definition: &CommitTypeDefinition) -> TextArea<'static> {
    let mut editor = TextArea::new(lines);
    editor.set_wrap_mode(WrapMode::WordOrGlyph);
    editor.set_cursor_line_style(Style::default());
    apply_heading_guards(&mut editor, definition);
    editor
}

fn apply_heading_guards(editor: &mut TextArea<'static>, definition: &CommitTypeDefinition) {
    let ranges = editor
        .lines()
        .iter()
        .enumerate()
        .filter(|(_, line)| exact_heading(line, definition).is_some())
        .map(|(row, line)| AtomicRange {
            row,
            start_col: 0,
            end_col: line.chars().count(),
        })
        .collect::<Vec<_>>();
    editor.set_atomic_ranges(ranges.iter().copied());
    editor.clear_custom_highlight();
    for range in ranges {
        editor.custom_highlight(
            ((range.row, range.start_col), (range.row, range.end_col)),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
            1,
        );
    }
}

fn expected_heading_signature(definition: &CommitTypeDefinition) -> Vec<FieldId> {
    std::iter::once(FieldId::Scope)
        .chain(std::iter::once(FieldId::Subject))
        .chain((0..definition.properties().len()).map(FieldId::Property))
        .collect()
}

fn headings_are_intact(lines: &[String], definition: &CommitTypeDefinition) -> bool {
    let headings = lines
        .iter()
        .enumerate()
        .filter_map(|(line_index, line)| exact_heading(line, definition).map(|id| (line_index, id)))
        .collect::<Vec<_>>();
    let signature = headings.iter().map(|(_, id)| *id).collect::<Vec<_>>();
    let separated = headings
        .windows(2)
        .all(|pair| pair[1].0.saturating_sub(pair[0].0) >= 3);
    let trailing_input = headings
        .last()
        .is_some_and(|(line, _)| lines.len().saturating_sub(*line) >= 3);

    signature == expected_heading_signature(definition) && separated && trailing_input
}

fn terminal_line(line: usize) -> u16 {
    u16::try_from(line).unwrap_or(u16::MAX)
}

fn terminal_column(column: usize) -> u16 {
    u16::try_from(column).unwrap_or(u16::MAX)
}

fn skip_heading_cursor(
    editor: &mut TextArea<'static>,
    definition: &CommitTypeDefinition,
    previous_cursor: (usize, usize),
) {
    let cursor = editor.cursor();
    let Some(line) = editor.lines().get(cursor.0) else {
        return;
    };
    if exact_heading(line, definition).is_none() {
        return;
    }

    let target_line = if cursor > previous_cursor || cursor.0 == 0 {
        cursor.0.saturating_add(1)
    } else {
        cursor.0.saturating_sub(1)
    };
    editor.move_cursor(CursorMove::Jump(
        terminal_line(target_line),
        terminal_column(previous_cursor.1),
    ));
}

fn scaffold_lines(definition: &CommitTypeDefinition) -> Vec<String> {
    let mut lines = Vec::with_capacity((definition.properties().len() + 2) * 3);
    for heading in std::iter::once("scope".to_owned())
        .chain(std::iter::once("subject".to_owned()))
        .chain(
            definition
                .properties()
                .iter()
                .map(|property| property.key().to_string()),
        )
    {
        lines.push(format!("{heading}:"));
        lines.push(String::new());
        lines.push(String::new());
    }
    lines
}

fn parse_document(lines: &[String], definition: &CommitTypeDefinition) -> ParsedDocument {
    let mut parsed = ParsedDocument::default();
    let mut boundaries = Vec::new();
    for (line_index, line) in lines.iter().enumerate() {
        if let Some(id) = exact_heading(line, definition) {
            boundaries.push(Boundary {
                line: line_index,
                kind: BoundaryKind::Known(id),
            });
        } else if looks_like_heading(line) || looks_like_malformed_known_heading(line, definition) {
            parsed.issues.push(ValidationIssue {
                field: None,
                line: line_index,
                message: format!("unknown or malformed field header {line:?}"),
            });
            boundaries.push(Boundary {
                line: line_index,
                kind: BoundaryKind::Invalid,
            });
        }
    }

    let mut occurrences = vec![0; definition.properties().len()];
    for (index, boundary) in boundaries.iter().copied().enumerate() {
        let BoundaryKind::Known(id) = boundary.kind else {
            continue;
        };
        let end_line = boundaries
            .get(index + 1)
            .map_or(lines.len(), |next| next.line);
        let text = section_text(&lines[boundary.line + 1..end_line]);
        let kind = match id {
            FieldId::Scope => FieldKind::Scope,
            FieldId::Subject => FieldKind::Subject,
            FieldId::Property(definition_index) => {
                let value_index = occurrences[definition_index];
                occurrences[definition_index] += 1;
                FieldKind::Property {
                    definition_index,
                    value_index,
                }
            }
        };
        parsed.sections.push(DocumentSection {
            kind,
            heading_line: boundary.line,
            end_line,
            text,
        });
    }

    for id in std::iter::once(FieldId::Scope)
        .chain(std::iter::once(FieldId::Subject))
        .chain((0..definition.properties().len()).map(FieldId::Property))
    {
        let matching = parsed
            .sections
            .iter()
            .filter(|section| section.kind.id() == id)
            .collect::<Vec<_>>();
        let repeatable = matches!(id, FieldId::Property(index) if definition.properties()[index].multiplicity() == PropertyMultiplicity::Multiple);
        if matching.len() > 1 && !repeatable {
            for duplicate in matching.into_iter().skip(1) {
                parsed.issues.push(ValidationIssue {
                    field: Some(id),
                    line: duplicate.heading_line,
                    message: format!("field {} may appear only once", field_name(id, definition)),
                });
            }
        }
    }
    parsed
}

fn exact_heading(line: &str, definition: &CommitTypeDefinition) -> Option<FieldId> {
    match line {
        "scope:" => Some(FieldId::Scope),
        "subject:" => Some(FieldId::Subject),
        _ => definition
            .properties()
            .iter()
            .position(|property| line == format!("{}:", property.key()))
            .map(FieldId::Property),
    }
}

fn looks_like_heading(line: &str) -> bool {
    let Some(name) = line.strip_suffix(':') else {
        return false;
    };
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn looks_like_malformed_known_heading(line: &str, definition: &CommitTypeDefinition) -> bool {
    let trimmed = line.trim();
    trimmed == "scope"
        || trimmed == "subject"
        || definition
            .properties()
            .iter()
            .any(|property| trimmed == property.key().as_str())
}

fn section_text(lines: &[String]) -> String {
    let start = lines
        .iter()
        .position(|line| !line.trim().is_empty())
        .unwrap_or(lines.len());
    let end = lines
        .iter()
        .rposition(|line| !line.trim().is_empty())
        .map_or(start, |index| index + 1);
    lines[start..end].join("\n")
}

fn parse_scope(parsed: &ParsedDocument, issues: &mut Vec<ValidationIssue>) -> Option<CommitScope> {
    let section = parsed
        .sections
        .iter()
        .find(|section| section.kind == FieldKind::Scope)?;
    if section.text.is_empty() {
        return None;
    }
    match CommitScope::new(&section.text) {
        Ok(scope) => Some(scope),
        Err(error) => {
            issues.push(ValidationIssue {
                field: Some(FieldId::Scope),
                line: section.heading_line + 1,
                message: error.to_string(),
            });
            None
        }
    }
}

fn parse_subject(
    parsed: &ParsedDocument,
    issues: &mut Vec<ValidationIssue>,
) -> Option<CommitSubject> {
    let Some(section) = parsed
        .sections
        .iter()
        .find(|section| section.kind == FieldKind::Subject)
    else {
        issues.push(ValidationIssue {
            field: Some(FieldId::Subject),
            line: 0,
            message: "restore the subject field header".to_owned(),
        });
        return None;
    };
    match CommitSubject::new(&section.text) {
        Ok(subject) => Some(subject),
        Err(error) => {
            issues.push(ValidationIssue {
                field: Some(FieldId::Subject),
                line: section.heading_line + 1,
                message: error.to_string(),
            });
            None
        }
    }
}

fn build_properties(
    parsed: &ParsedDocument,
    definition: &CommitTypeDefinition,
    issues: &mut Vec<ValidationIssue>,
) -> Vec<AuthoredProperty> {
    let mut authored = Vec::new();
    for (definition_index, property) in definition.properties().iter().enumerate() {
        let sections = parsed
            .sections
            .iter()
            .filter(|section| section.kind.id() == FieldId::Property(definition_index))
            .collect::<Vec<_>>();
        let first_line = sections
            .first()
            .map_or(0, |section| section.heading_line + 1);
        let values = sections
            .iter()
            .filter(|section| !section.text.is_empty())
            .filter_map(|section| match PropertyValue::new(&section.text) {
                Ok(value) => Some(value),
                Err(error) => {
                    issues.push(ValidationIssue {
                        field: Some(FieldId::Property(definition_index)),
                        line: section.heading_line + 1,
                        message: error.to_string(),
                    });
                    None
                }
            })
            .collect::<Vec<_>>();
        if values.is_empty() {
            if property.requirement() == &PropertyRequirement::Required {
                issues.push(ValidationIssue {
                    field: Some(FieldId::Property(definition_index)),
                    line: first_line,
                    message: format!("complete required property {:?}", property.key()),
                });
            }
            continue;
        }
        let values = match property.multiplicity() {
            PropertyMultiplicity::Single => PropertyValues::single(values[0].clone()),
            PropertyMultiplicity::Multiple => match PropertyValues::multiple(values) {
                Ok(values) => values,
                Err(error) => {
                    issues.push(ValidationIssue {
                        field: Some(FieldId::Property(definition_index)),
                        line: first_line,
                        message: error.to_string(),
                    });
                    continue;
                }
            },
        };
        authored.push(AuthoredProperty::new(property.key().clone(), values));
    }
    authored
}

fn field_name(id: FieldId, definition: &CommitTypeDefinition) -> String {
    match id {
        FieldId::Scope => "scope".to_owned(),
        FieldId::Subject => "subject".to_owned(),
        FieldId::Property(index) => definition.properties()[index].key().to_string(),
    }
}

pub(crate) struct ReviewState {
    pub(crate) draft: CommitDraft,
    pub(crate) message: CommitMessage,
    pub(crate) scroll: u16,
}

pub(crate) struct AuthoringSession<'a> {
    pub(crate) definitions: &'a [CommitTypeDefinition],
    pub(crate) selected_type: usize,
    pub(crate) preselected: bool,
    pub(crate) stage: Stage,
    pub(crate) composer: ComposerState,
    pub(crate) review: Option<ReviewState>,
    pub(crate) keymap: Keymap,
    pub(crate) vim_mode: VimMode,
    pub(crate) confirmation: ConfirmationAction,
    confirmation_resume: ResumeStage,
    pub(crate) too_small: bool,
}

impl<'a> AuthoringSession<'a> {
    pub(crate) fn new(
        definitions: &'a [CommitTypeDefinition],
        preselected_index: Option<usize>,
    ) -> Self {
        let selected_type = preselected_index.unwrap_or(0);
        Self {
            definitions,
            selected_type,
            preselected: preselected_index.is_some(),
            stage: if preselected_index.is_some() {
                Stage::Compose
            } else {
                Stage::SelectType
            },
            composer: ComposerState::new(&definitions[selected_type]),
            review: None,
            keymap: Keymap::Conventional,
            vim_mode: VimMode::Insert,
            confirmation: ConfirmationAction::Cancel,
            confirmation_resume: ResumeStage::Compose,
            too_small: false,
        }
    }

    pub(crate) fn definition(&self) -> &CommitTypeDefinition {
        &self.definitions[self.selected_type]
    }

    pub(crate) fn handle_event(&mut self, event: Event) -> Option<CommitDraftAuthorOutcome> {
        if self.too_small {
            return self.handle_too_small(&event);
        }
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => self.handle_key(key),
            Event::Paste(text) if self.stage == Stage::Compose => {
                if self.keymap == Keymap::Conventional || self.vim_mode == VimMode::Insert {
                    let definition = self.definition().clone();
                    self.composer
                        .edit_preserving_headings(&definition, |editor| {
                            editor.insert_str(text);
                        });
                }
                None
            }
            Event::Resize(_, _)
            | Event::FocusGained
            | Event::FocusLost
            | Event::Mouse(_)
            | Event::Paste(_)
            | Event::Key(_) => None,
        }
    }

    fn handle_too_small(&mut self, event: &Event) -> Option<CommitDraftAuthorOutcome> {
        let Event::Key(key) = event else {
            return None;
        };
        if key.kind != KeyEventKind::Press {
            return None;
        }
        if self.stage == Stage::Confirm {
            return self.handle_confirmation_key(*key);
        }
        if matches!(key.code, KeyCode::Esc | KeyCode::Char('q')) || control(*key, 'c') {
            self.request_confirmation(ConfirmationAction::Cancel, ResumeStage::Compose)
        } else {
            None
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> Option<CommitDraftAuthorOutcome> {
        match self.stage {
            Stage::SelectType => self.handle_picker_key(key),
            Stage::Compose => self.handle_composer_key(key),
            Stage::Review => self.handle_review_key(key),
            Stage::Confirm => self.handle_confirmation_key(key),
        }
    }

    fn handle_picker_key(&mut self, key: KeyEvent) -> Option<CommitDraftAuthorOutcome> {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected_type = if self.selected_type == 0 {
                    self.definitions.len() - 1
                } else {
                    self.selected_type - 1
                };
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected_type = (self.selected_type + 1) % self.definitions.len();
            }
            KeyCode::Home => self.selected_type = 0,
            KeyCode::End => self.selected_type = self.definitions.len() - 1,
            KeyCode::Enter => {
                self.composer = ComposerState::new(self.definition());
                self.stage = Stage::Compose;
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                return Some(CommitDraftAuthorOutcome::Cancelled);
            }
            _ if control(key, 'c') => return Some(CommitDraftAuthorOutcome::Cancelled),
            _ => {}
        }
        None
    }

    fn handle_composer_key(&mut self, key: KeyEvent) -> Option<CommitDraftAuthorOutcome> {
        if control(key, 't') {
            self.keymap = match self.keymap {
                Keymap::Conventional => Keymap::Vim,
                Keymap::Vim => Keymap::Conventional,
            };
            self.vim_mode = if self.keymap == Keymap::Vim {
                VimMode::Normal
            } else {
                VimMode::Insert
            };
            return None;
        }
        if control(key, 's') {
            let definition = self.definition().clone();
            if let Some((draft, message)) = self.composer.validate(&definition) {
                self.review = Some(ReviewState {
                    draft,
                    message,
                    scroll: 0,
                });
                self.stage = Stage::Review;
            }
            return None;
        }
        if control(key, 'c') {
            return self.request_confirmation(ConfirmationAction::Cancel, ResumeStage::Compose);
        }
        match self.keymap {
            Keymap::Conventional => {
                if key.code == KeyCode::Esc {
                    let action = if self.preselected {
                        ConfirmationAction::Cancel
                    } else {
                        ConfirmationAction::ChangeType
                    };
                    return self.request_confirmation(action, ResumeStage::Compose);
                }
                self.input_key(key);
            }
            Keymap::Vim => match self.vim_mode {
                VimMode::Insert => {
                    if key.code == KeyCode::Esc {
                        self.vim_mode = VimMode::Normal;
                    } else {
                        self.input_key(key);
                    }
                }
                VimMode::Normal => return self.handle_vim_normal_key(key),
            },
        }
        None
    }

    fn input_key(&mut self, key: KeyEvent) {
        let definition = self.definition().clone();
        self.composer
            .edit_preserving_headings(&definition, |editor| {
                editor.input(Input::from(key));
            });
    }

    fn handle_vim_normal_key(&mut self, key: KeyEvent) -> Option<CommitDraftAuthorOutcome> {
        let definition = self.definition().clone();
        match key.code {
            KeyCode::Char('h') | KeyCode::Left => {
                self.composer.move_cursor(&definition, CursorMove::Back);
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.composer.move_cursor(&definition, CursorMove::Down);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.composer.move_cursor(&definition, CursorMove::Up);
            }
            KeyCode::Char('l') | KeyCode::Right => {
                self.composer.move_cursor(&definition, CursorMove::Forward);
            }
            KeyCode::Char('w') => {
                self.composer
                    .move_cursor(&definition, CursorMove::WordForward);
            }
            KeyCode::Char('b') => {
                self.composer.move_cursor(&definition, CursorMove::WordBack);
            }
            KeyCode::Char('0') | KeyCode::Home => {
                self.composer.move_cursor(&definition, CursorMove::Head);
            }
            KeyCode::Char('$') | KeyCode::End => {
                self.composer.move_cursor(&definition, CursorMove::End);
            }
            KeyCode::Char('i') => self.vim_mode = VimMode::Insert,
            KeyCode::Char('a') => {
                self.composer.move_cursor(&definition, CursorMove::Forward);
                self.vim_mode = VimMode::Insert;
            }
            KeyCode::Char('x') | KeyCode::Delete => {
                self.composer
                    .edit_preserving_headings(&definition, |editor| {
                        editor.delete_next_char();
                    });
            }
            KeyCode::Char('u') => {
                self.composer
                    .edit_preserving_headings(&definition, |editor| {
                        editor.undo();
                    });
            }
            _ if control(key, 'r') => {
                self.composer
                    .edit_preserving_headings(&definition, |editor| {
                        editor.redo();
                    });
            }
            KeyCode::Char('q') => {
                let action = if self.preselected {
                    ConfirmationAction::Cancel
                } else {
                    ConfirmationAction::ChangeType
                };
                return self.request_confirmation(action, ResumeStage::Compose);
            }
            _ => {}
        }
        self.composer.issues.clear();
        None
    }

    fn handle_review_key(&mut self, key: KeyEvent) -> Option<CommitDraftAuthorOutcome> {
        match key.code {
            KeyCode::Enter => {
                return self
                    .review
                    .take()
                    .map(|review| CommitDraftAuthorOutcome::Authored(review.draft));
            }
            KeyCode::Esc => {
                self.review = None;
                self.stage = Stage::Compose;
            }
            KeyCode::Char('q') => {
                return self.request_confirmation(ConfirmationAction::Cancel, ResumeStage::Review);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(review) = &mut self.review {
                    review.scroll = review.scroll.saturating_sub(1);
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(review) = &mut self.review {
                    review.scroll = review.scroll.saturating_add(1);
                }
            }
            KeyCode::PageUp => {
                if let Some(review) = &mut self.review {
                    review.scroll = review.scroll.saturating_sub(10);
                }
            }
            KeyCode::PageDown => {
                if let Some(review) = &mut self.review {
                    review.scroll = review.scroll.saturating_add(10);
                }
            }
            _ if control(key, 'c') => {
                return self.request_confirmation(ConfirmationAction::Cancel, ResumeStage::Review);
            }
            _ => {}
        }
        None
    }

    fn handle_confirmation_key(&mut self, key: KeyEvent) -> Option<CommitDraftAuthorOutcome> {
        match key.code {
            KeyCode::Char('y') => match self.confirmation {
                ConfirmationAction::Cancel => Some(CommitDraftAuthorOutcome::Cancelled),
                ConfirmationAction::ChangeType => {
                    self.composer = ComposerState::new(self.definition());
                    self.review = None;
                    self.stage = Stage::SelectType;
                    None
                }
            },
            KeyCode::Char('n') | KeyCode::Esc | KeyCode::Enter => {
                self.stage = match self.confirmation_resume {
                    ResumeStage::Compose => Stage::Compose,
                    ResumeStage::Review => Stage::Review,
                };
                None
            }
            _ => None,
        }
    }

    fn request_confirmation(
        &mut self,
        action: ConfirmationAction,
        resume: ResumeStage,
    ) -> Option<CommitDraftAuthorOutcome> {
        if !self.composer.dirty() {
            return match action {
                ConfirmationAction::Cancel => Some(CommitDraftAuthorOutcome::Cancelled),
                ConfirmationAction::ChangeType => {
                    self.composer = ComposerState::new(self.definition());
                    self.review = None;
                    self.stage = Stage::SelectType;
                    None
                }
            };
        }
        self.confirmation = action;
        self.confirmation_resume = resume;
        self.stage = Stage::Confirm;
        None
    }
}

fn control(key: KeyEvent, character: char) -> bool {
    key.code == KeyCode::Char(character) && key.modifiers.contains(KeyModifiers::CONTROL)
}
