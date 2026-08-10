use gitserious_app::CommitDraftAuthorOutcome;
use gitserious_core::{
    AuthoredProperty, CommitDraft, CommitMessage, CommitScope, CommitSubject, CommitTypeDefinition,
    PropertyMultiplicity, PropertyRequirement, PropertyValue, PropertyValues,
    render_commit_message,
};
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use tui_textarea::{CursorMove, Input, TextArea, WrapMode};

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
pub(crate) enum FieldKind {
    Scope,
    Subject,
    Property {
        definition_index: usize,
        value_index: usize,
    },
}

pub(crate) struct FieldState {
    pub(crate) kind: FieldKind,
    pub(crate) editor: TextArea<'static>,
}

impl FieldState {
    fn new(kind: FieldKind) -> Self {
        let mut editor = TextArea::default();
        editor.set_wrap_mode(WrapMode::WordOrGlyph);
        Self { kind, editor }
    }

    pub(crate) fn text(&self) -> String {
        self.editor.lines().join("\n")
    }

    fn is_empty(&self) -> bool {
        self.text().is_empty()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ValidationIssue {
    pub(crate) field: usize,
    pub(crate) message: String,
}

pub(crate) struct ComposerState {
    pub(crate) fields: Vec<FieldState>,
    pub(crate) focused: usize,
    pub(crate) issues: Vec<ValidationIssue>,
}

impl ComposerState {
    fn new(definition: &CommitTypeDefinition) -> Self {
        let mut fields = vec![
            FieldState::new(FieldKind::Scope),
            FieldState::new(FieldKind::Subject),
        ];
        fields.extend(
            definition
                .properties()
                .iter()
                .enumerate()
                .map(|(definition_index, _)| {
                    FieldState::new(FieldKind::Property {
                        definition_index,
                        value_index: 0,
                    })
                }),
        );
        Self {
            fields,
            focused: 0,
            issues: Vec::new(),
        }
    }

    pub(crate) fn current(&self) -> &FieldState {
        &self.fields[self.focused]
    }

    pub(crate) fn current_mut(&mut self) -> &mut FieldState {
        &mut self.fields[self.focused]
    }

    pub(crate) fn dirty(&self) -> bool {
        self.fields.iter().any(|field| !field.is_empty())
    }

    fn next(&mut self) {
        self.focused = (self.focused + 1) % self.fields.len();
    }

    fn previous(&mut self) {
        self.focused = if self.focused == 0 {
            self.fields.len() - 1
        } else {
            self.focused - 1
        };
    }

    fn add_repeatable_value(&mut self, definition: &CommitTypeDefinition) {
        let FieldKind::Property {
            definition_index, ..
        } = self.current().kind
        else {
            return;
        };
        if definition.properties()[definition_index].multiplicity()
            != PropertyMultiplicity::Multiple
        {
            return;
        }
        let insertion = self
            .fields
            .iter()
            .rposition(|field| {
                matches!(
                    field.kind,
                    FieldKind::Property {
                        definition_index: candidate,
                        ..
                    } if candidate == definition_index
                )
            })
            .map_or(self.focused + 1, |index| index + 1);
        let value_index = self
            .fields
            .iter()
            .filter(|field| {
                matches!(
                    field.kind,
                    FieldKind::Property {
                        definition_index: candidate,
                        ..
                    } if candidate == definition_index
                )
            })
            .count();
        self.fields.insert(
            insertion,
            FieldState::new(FieldKind::Property {
                definition_index,
                value_index,
            }),
        );
        self.focused = insertion;
        self.issues.clear();
    }

    fn current_is_repeatable(&self, definition: &CommitTypeDefinition) -> bool {
        match self.current().kind {
            FieldKind::Property {
                definition_index, ..
            } => {
                definition.properties()[definition_index].multiplicity()
                    == PropertyMultiplicity::Multiple
            }
            FieldKind::Scope | FieldKind::Subject => false,
        }
    }

    fn remove_repeatable_value(&mut self, definition: &CommitTypeDefinition) {
        let FieldKind::Property {
            definition_index, ..
        } = self.current().kind
        else {
            return;
        };
        if definition.properties()[definition_index].multiplicity()
            != PropertyMultiplicity::Multiple
        {
            return;
        }
        let count = self
            .fields
            .iter()
            .filter(|field| {
                matches!(
                    field.kind,
                    FieldKind::Property {
                        definition_index: candidate,
                        ..
                    } if candidate == definition_index
                )
            })
            .count();
        if count == 1 {
            self.fields[self.focused] = FieldState::new(FieldKind::Property {
                definition_index,
                value_index: 0,
            });
        } else {
            self.fields.remove(self.focused);
            self.focused = self.focused.min(self.fields.len() - 1);
            self.renumber_values(definition_index);
        }
        self.issues.clear();
    }

    fn renumber_values(&mut self, definition_index: usize) {
        let mut value_index = 0;
        for field in &mut self.fields {
            if let FieldKind::Property {
                definition_index: candidate,
                value_index: current,
            } = &mut field.kind
                && *candidate == definition_index
            {
                *current = value_index;
                value_index += 1;
            }
        }
    }

    fn validate(
        &mut self,
        definition: &CommitTypeDefinition,
    ) -> Option<(CommitDraft, CommitMessage)> {
        let mut issues = Vec::new();
        let (scope, subject) = self.parse_header_fields(&mut issues);
        let authored = self.build_properties(definition, &mut issues);

        if !issues.is_empty() {
            self.focused = issues[0].field;
            self.issues = issues;
            return None;
        }
        let subject = subject?;
        let draft = match CommitDraft::new(definition.id().clone(), scope, subject, authored) {
            Ok(draft) => draft,
            Err(error) => {
                self.issues = vec![ValidationIssue {
                    field: self.focused,
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
                        field: self.focused,
                        message: error.to_string(),
                    })
                    .collect();
                None
            }
        }
    }

    fn parse_header_fields(
        &self,
        issues: &mut Vec<ValidationIssue>,
    ) -> (Option<CommitScope>, Option<CommitSubject>) {
        let scope_text = self.fields[0].text();
        let scope = if scope_text.is_empty() {
            None
        } else {
            match CommitScope::new(scope_text) {
                Ok(scope) => Some(scope),
                Err(error) => {
                    issues.push(ValidationIssue {
                        field: 0,
                        message: error.to_string(),
                    });
                    None
                }
            }
        };
        let subject = CommitSubject::new(self.fields[1].text()).map_or_else(
            |error| {
                issues.push(ValidationIssue {
                    field: 1,
                    message: error.to_string(),
                });
                None
            },
            Some,
        );
        (scope, subject)
    }

    fn build_properties(
        &self,
        definition: &CommitTypeDefinition,
        issues: &mut Vec<ValidationIssue>,
    ) -> Vec<AuthoredProperty> {
        let mut authored = Vec::new();
        for (definition_index, property) in definition.properties().iter().enumerate() {
            let matching = self.property_fields(definition_index);
            let first_field = matching.first().map_or(1, |(index, _)| *index);
            let values = Self::build_property_values(&matching, issues);
            if values.is_empty() {
                if property.requirement() == &PropertyRequirement::Required {
                    issues.push(ValidationIssue {
                        field: first_field,
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
                            field: first_field,
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

    fn property_fields(&self, definition_index: usize) -> Vec<(usize, &FieldState)> {
        self.fields
            .iter()
            .enumerate()
            .filter(|(_, field)| {
                matches!(
                    field.kind,
                    FieldKind::Property {
                        definition_index: candidate,
                        ..
                    } if candidate == definition_index
                )
            })
            .collect()
    }

    fn build_property_values(
        matching: &[(usize, &FieldState)],
        issues: &mut Vec<ValidationIssue>,
    ) -> Vec<PropertyValue> {
        let mut values = Vec::new();
        for (field_index, field) in matching {
            let text = field.text();
            if text.trim().is_empty() {
                continue;
            }
            match PropertyValue::new(text) {
                Ok(value) => values.push(value),
                Err(error) => issues.push(ValidationIssue {
                    field: *field_index,
                    message: error.to_string(),
                }),
            }
        }
        values
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
                    self.composer.current_mut().editor.insert_str(text);
                    self.composer.issues.clear();
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
        if key.code == KeyCode::F(2) {
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
        match key.code {
            KeyCode::Tab => {
                self.composer.next();
                return None;
            }
            KeyCode::BackTab => {
                self.composer.previous();
                return None;
            }
            _ if control(key, 'n') => {
                let definition = self.definition().clone();
                if self.composer.current_is_repeatable(&definition) {
                    self.composer.add_repeatable_value(&definition);
                    return None;
                }
            }
            _ if control(key, 'd') => {
                let definition = self.definition().clone();
                if self.composer.current_is_repeatable(&definition) {
                    self.composer.remove_repeatable_value(&definition);
                    return None;
                }
            }
            _ => {}
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
        let single_line = matches!(
            self.composer.current().kind,
            FieldKind::Scope | FieldKind::Subject
        );
        if single_line && key.code == KeyCode::Enter {
            self.composer.next();
            return;
        }
        self.composer.current_mut().editor.input(Input::from(key));
        self.composer.issues.clear();
    }

    fn handle_vim_normal_key(&mut self, key: KeyEvent) -> Option<CommitDraftAuthorOutcome> {
        let editor = &mut self.composer.current_mut().editor;
        match key.code {
            KeyCode::Char('h') | KeyCode::Left => editor.move_cursor(CursorMove::Back),
            KeyCode::Char('j') | KeyCode::Down => editor.move_cursor(CursorMove::Down),
            KeyCode::Char('k') | KeyCode::Up => editor.move_cursor(CursorMove::Up),
            KeyCode::Char('l') | KeyCode::Right => editor.move_cursor(CursorMove::Forward),
            KeyCode::Char('w') => editor.move_cursor(CursorMove::WordForward),
            KeyCode::Char('b') => editor.move_cursor(CursorMove::WordBack),
            KeyCode::Char('0') | KeyCode::Home => editor.move_cursor(CursorMove::Head),
            KeyCode::Char('$') | KeyCode::End => editor.move_cursor(CursorMove::End),
            KeyCode::Char('i') => self.vim_mode = VimMode::Insert,
            KeyCode::Char('a') => {
                editor.move_cursor(CursorMove::Forward);
                self.vim_mode = VimMode::Insert;
            }
            KeyCode::Char('x') | KeyCode::Delete => {
                editor.delete_next_char();
            }
            KeyCode::Char('u') => {
                editor.undo();
            }
            _ if control(key, 'r') => {
                editor.redo();
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
