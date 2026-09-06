use gitserious_app::{CommitAuthoringContext, CommitDraftAuthorOutcome, CommitTemplate};
use gitserious_core::{
    AuthoredProperty, CommitDraft, CommitMessage, CommitScope, CommitSubject, CommitTypeDefinition,
    ConditionalApplicability, PropertyMultiplicity, PropertyRequirement, PropertyResponse,
    PropertyValue, PropertyValues, render_commit_message, validate_commit_draft_report,
};
use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use tui_textarea::{AtomicRange, CursorMove, CursorRenderMode, Input, TextArea, WrapMode};

pub(crate) const SCOPE_VALUE_LINE: usize = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Stage {
    SelectType,
    Compose,
    Review,
    Confirm,
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
    Description,
    Property(usize),
    BreakingChange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FieldKind {
    Scope,
    Description,
    Property {
        definition_index: usize,
        value_index: usize,
    },
    BreakingChange,
}

impl FieldKind {
    pub(crate) const fn id(self) -> FieldId {
        match self {
            Self::Scope => FieldId::Scope,
            Self::Description => FieldId::Description,
            Self::Property {
                definition_index, ..
            } => FieldId::Property(definition_index),
            Self::BreakingChange => FieldId::BreakingChange,
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
enum MessageSection {
    Subject,
    Body,
    Footer,
}

impl MessageSection {
    const fn label(self) -> &'static str {
        match self {
            Self::Subject => "Message Subject",
            Self::Body => "Message Body",
            Self::Footer => "Message Footer",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DocumentMarker {
    Section(MessageSection),
    Field(FieldId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BoundaryKind {
    Known(FieldId),
    Group,
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
    pub(crate) applicability: Vec<Option<ConditionalApplicability>>,
    pub(crate) warnings: Vec<String>,
}

impl ComposerState {
    fn new(definition: &CommitTypeDefinition) -> Self {
        let pristine = scaffold_lines(definition);
        let mut editor = text_area(pristine.clone(), definition);
        editor.move_cursor(CursorMove::Jump(terminal_line(SCOPE_VALUE_LINE), 0));
        Self {
            editor,
            pristine,
            issues: Vec::new(),
            applicability: vec![None; definition.properties().len()],
            warnings: Vec::new(),
        }
    }

    pub(crate) fn dirty(&self) -> bool {
        self.editor.lines() != self.pristine || self.applicability.iter().any(Option::is_some)
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
        let mut fields = Vec::with_capacity(definition.properties().len() + 3);
        for id in std::iter::once(FieldId::Scope)
            .chain(std::iter::once(FieldId::Description))
            .chain((0..definition.properties().len()).map(FieldId::Property))
            .chain(std::iter::once(FieldId::BreakingChange))
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
                    FieldId::Description => {
                        !section.text.is_empty() && CommitSubject::new(&section.text).is_err()
                    }
                    FieldId::Property(_) | FieldId::BreakingChange => false,
                });
            let has_value = sections.iter().any(|section| !section.text.is_empty());
            let decision = match id {
                FieldId::Property(index) => self.applicability[index],
                _ => None,
            };
            let conditional = matches!(id, FieldId::Property(index) if matches!(definition.properties()[index].requirement(), PropertyRequirement::Conditional(_)));
            let invalid =
                invalid || (decision == Some(ConditionalApplicability::DoesNotApply) && has_value);
            let complete = if conditional {
                match decision {
                    Some(ConditionalApplicability::Applies) => has_value,
                    Some(ConditionalApplicability::DoesNotApply) => !has_value,
                    None => false,
                }
            } else {
                has_value
            };
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
        let subject = parse_description(&parsed, &mut issues);
        let authored = build_properties(&parsed, definition, &mut issues);
        let breaking_change = parse_breaking_change(&parsed, &mut issues);

        if !issues.is_empty() {
            issues.sort_by_key(|issue| issue.line);
            let first_line = issues[0].line;
            self.issues = issues;
            self.editor
                .move_cursor(CursorMove::Jump(terminal_line(first_line), 0));
            return None;
        }
        let subject = subject?;
        let responses = definition
            .properties()
            .iter()
            .enumerate()
            .map(|(index, property)| {
                PropertyResponse::new(
                    property.key().clone(),
                    authored
                        .iter()
                        .find(|item| item.key() == property.key())
                        .map(|item| item.values().clone()),
                    self.applicability[index],
                )
            })
            .collect();
        let draft =
            match CommitDraft::from_responses(definition.id().clone(), scope, subject, responses) {
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
        let draft = match breaking_change {
            Some(value) => draft.with_breaking_change(value),
            None => draft,
        };
        let report = validate_commit_draft_report(definition, &draft);
        self.warnings = report
            .warnings()
            .iter()
            .map(|issue| issue.kind().to_string())
            .collect();
        match render_commit_message(definition, &draft) {
            Ok(message) => {
                self.issues.clear();
                Some((draft, message))
            }
            Err(errors) => {
                self.issues = errors
                    .as_slice()
                    .iter()
                    .map(|error| {
                        let field = validation_field(error, definition);
                        ValidationIssue {
                            field,
                            line: parsed
                                .sections
                                .iter()
                                .find(|section| Some(section.kind.id()) == field)
                                .map_or(self.editor.cursor().0, |section| section.heading_line + 1),
                            message: error.to_string(),
                        }
                    })
                    .collect();
                if let Some(issue) = self.issues.first() {
                    self.editor
                        .move_cursor(CursorMove::Jump(terminal_line(issue.line), 0));
                }
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
            skip_noneditable_cursor(&mut self.editor, definition, previous_cursor);
            self.issues.clear();
        } else {
            self.editor = previous;
        }
    }

    fn edit_within_current_field(
        &mut self,
        definition: &CommitTypeDefinition,
        edit: impl FnOnce(&mut TextArea<'static>),
    ) {
        let previous_editor = self.editor.clone();
        let previous_issues = self.issues.clone();
        let previous_field = self.current_field(definition).map(FieldKind::id);
        if previous_field.is_none() {
            return;
        }
        self.edit_preserving_headings(definition, edit);
        if self.current_field(definition).map(FieldKind::id) != previous_field {
            self.editor = previous_editor;
            self.issues = previous_issues;
        }
    }

    fn move_cursor(&mut self, definition: &CommitTypeDefinition, movement: CursorMove) {
        let previous_cursor = self.editor.cursor();
        self.editor.move_cursor(movement);
        skip_noneditable_cursor(&mut self.editor, definition, previous_cursor);
        self.issues.clear();
    }

    fn move_horizontally_within_field(&mut self, definition: &CommitTypeDefinition, key: KeyEvent) {
        let previous = self.editor.clone();
        let previous_cursor = self.editor.cursor();
        let previous_field = self.current_field(definition).map(FieldKind::id);
        self.editor.input(Input::from(key));
        skip_noneditable_cursor(&mut self.editor, definition, previous_cursor);
        let current_field = self.current_field(definition).map(FieldKind::id);
        if previous_field.is_none() || current_field != previous_field {
            self.editor = previous;
        } else {
            self.issues.clear();
        }
    }

    fn advance_on_enter(&mut self, definition: &CommitTypeDefinition) -> bool {
        let cursor_line = self.editor.cursor().0;
        let parsed = self.parse(definition);
        let Some(current_index) = parsed.sections.iter().position(|section| {
            cursor_line >= section.heading_line && cursor_line < section.end_line
        }) else {
            return false;
        };
        let current = &parsed.sections[current_index];
        let should_advance = matches!(current.kind, FieldKind::Scope | FieldKind::Description)
            || current.text.is_empty();
        if !should_advance {
            return false;
        }
        if let Some(next) = parsed.sections.get(current_index + 1) {
            self.editor
                .move_cursor(CursorMove::Jump(terminal_line(next.heading_line + 1), 0));
            self.issues.clear();
        }
        true
    }
}

fn validation_field(
    error: &gitserious_core::CommitValidationError,
    definition: &CommitTypeDefinition,
) -> Option<FieldId> {
    use gitserious_core::{CommitValidationError as E, PropertyValidationIssueKind as P};
    let key = match error {
        E::UnknownProperty(key) | E::MissingRequired(key) | E::Multiplicity { key, .. } => key,
        E::PropertyResponse(kind) => match kind {
            P::UnknownProperty(key)
            | P::DuplicateProperty(key)
            | P::MissingRequired(key)
            | P::MissingRecommended(key)
            | P::MissingConditionalDecision(key)
            | P::MissingApplicableValue(key)
            | P::ValueForNonApplicableProperty(key)
            | P::UnexpectedConditionalDecision(key)
            | P::Multiplicity { key, .. } => key,
        },
        E::UnknownCommitType { .. } | E::TypeMismatch { .. } => return None,
    };
    definition
        .properties()
        .iter()
        .position(|property| property.key() == key)
        .map(FieldId::Property)
}

fn text_area(lines: Vec<String>, definition: &CommitTypeDefinition) -> TextArea<'static> {
    let mut editor = TextArea::new(lines);
    editor.set_wrap_mode(WrapMode::WordOrGlyph);
    editor.set_cursor_line_style(Style::default());
    editor.set_cursor_render_mode(CursorRenderMode::Hidden);
    apply_heading_guards(&mut editor, definition);
    editor
}

fn apply_heading_guards(editor: &mut TextArea<'static>, definition: &CommitTypeDefinition) {
    let ranges = editor
        .lines()
        .iter()
        .enumerate()
        .filter_map(|(row, line)| {
            structural_marker(line, definition).map(|marker| {
                (
                    AtomicRange {
                        row,
                        start_col: 0,
                        end_col: line.chars().count(),
                    },
                    marker,
                )
            })
        })
        .collect::<Vec<_>>();
    editor.set_atomic_ranges(ranges.iter().map(|(range, _)| *range));
    editor.clear_custom_highlight();
    for (range, marker) in ranges {
        let style = match marker {
            DocumentMarker::Section(_) => Style::default().fg(Color::Yellow),
            DocumentMarker::Field(_) => Style::default(),
        }
        .add_modifier(Modifier::BOLD);
        editor.custom_highlight(
            ((range.row, range.start_col), (range.row, range.end_col)),
            style,
            1,
        );
    }
}

fn expected_marker_signature(definition: &CommitTypeDefinition) -> Vec<DocumentMarker> {
    std::iter::once(DocumentMarker::Section(MessageSection::Subject))
        .chain(std::iter::once(DocumentMarker::Field(FieldId::Scope)))
        .chain(std::iter::once(DocumentMarker::Field(FieldId::Description)))
        .chain(std::iter::once(DocumentMarker::Section(
            MessageSection::Body,
        )))
        .chain(
            (0..definition.properties().len())
                .map(FieldId::Property)
                .map(DocumentMarker::Field),
        )
        .chain(std::iter::once(DocumentMarker::Section(
            MessageSection::Footer,
        )))
        .chain(std::iter::once(DocumentMarker::Field(
            FieldId::BreakingChange,
        )))
        .collect()
}

fn headings_are_intact(lines: &[String], definition: &CommitTypeDefinition) -> bool {
    let markers = lines
        .iter()
        .enumerate()
        .filter_map(|(line_index, line)| {
            structural_marker(line, definition).map(|marker| (line_index, marker))
        })
        .collect::<Vec<_>>();
    let signature = markers
        .iter()
        .map(|(_, marker)| *marker)
        .collect::<Vec<_>>();
    let headings = markers
        .iter()
        .filter(|(_, marker)| matches!(marker, DocumentMarker::Field(_)))
        .collect::<Vec<_>>();
    let separated = headings
        .windows(2)
        .all(|pair| pair[1].0.saturating_sub(pair[0].0) >= 3);
    let groups_are_separated = markers.windows(2).all(|pair| {
        let distance = pair[1].0.saturating_sub(pair[0].0);
        match (pair[0].1, pair[1].1) {
            (DocumentMarker::Section(_), DocumentMarker::Field(_)) => distance >= 1,
            (DocumentMarker::Field(_), DocumentMarker::Section(_)) => distance >= 4,
            _ => true,
        }
    });
    let final_field_is_separated = headings
        .last()
        .is_some_and(|(line, _)| lines.len().saturating_sub(*line) >= 3);

    signature == expected_marker_signature(definition)
        && separated
        && groups_are_separated
        && final_field_is_separated
        && lines.last().is_some_and(String::is_empty)
}

fn terminal_line(line: usize) -> u16 {
    u16::try_from(line).unwrap_or(u16::MAX)
}

fn terminal_column(column: usize) -> u16 {
    u16::try_from(column).unwrap_or(u16::MAX)
}

fn skip_noneditable_cursor(
    editor: &mut TextArea<'static>,
    definition: &CommitTypeDefinition,
    previous_cursor: (usize, usize),
) {
    let moving_forward = editor.cursor() > previous_cursor;
    loop {
        let cursor = editor.cursor();
        let Some(line) = editor.lines().get(cursor.0) else {
            return;
        };
        let target_line = if structural_marker(line, definition).is_some()
            || reserved_separator(editor.lines(), cursor.0, definition)
        {
            let direction = if moving_forward { 1 } else { -1 };
            let mut target = cursor.0;
            loop {
                let Some(next) = target.checked_add_signed(direction) else {
                    editor.move_cursor(CursorMove::Jump(
                        terminal_line(previous_cursor.0),
                        terminal_column(previous_cursor.1),
                    ));
                    return;
                };
                let Some(candidate) = editor.lines().get(next) else {
                    editor.move_cursor(CursorMove::Jump(
                        terminal_line(previous_cursor.0),
                        terminal_column(previous_cursor.1),
                    ));
                    return;
                };
                target = next;
                if structural_marker(candidate, definition).is_none()
                    && !reserved_separator(editor.lines(), target, definition)
                {
                    break target;
                }
            }
        } else {
            return;
        };
        editor.move_cursor(CursorMove::Jump(
            terminal_line(target_line),
            terminal_column(previous_cursor.1),
        ));
    }
}

fn reserved_separator(lines: &[String], line: usize, definition: &CommitTypeDefinition) -> bool {
    if !lines.get(line).is_some_and(String::is_empty) {
        return false;
    }
    let Some((heading_line, marker)) =
        lines[..line]
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, candidate)| {
                structural_marker(candidate, definition).map(|marker| (index, marker))
            })
    else {
        return false;
    };
    let next_content = lines[line + 1..]
        .iter()
        .enumerate()
        .find(|(_, candidate)| !candidate.is_empty());
    let (boundary_line, next_marker) = match next_content {
        Some((offset, candidate)) => {
            let Some(marker) = structural_marker(candidate, definition) else {
                return false;
            };
            (line.saturating_add(offset).saturating_add(1), Some(marker))
        }
        None => (lines.len(), None),
    };
    match marker {
        DocumentMarker::Field(_) => {
            let reserved_rows = if matches!(next_marker, Some(DocumentMarker::Section(_))) {
                2
            } else {
                1
            };
            line > heading_line.saturating_add(1)
                && boundary_line.saturating_sub(line) <= reserved_rows
        }
        DocumentMarker::Section(MessageSection::Body | MessageSection::Footer) => true,
        DocumentMarker::Section(MessageSection::Subject) => false,
    }
}

fn scaffold_lines(definition: &CommitTypeDefinition) -> Vec<String> {
    let mut lines = Vec::with_capacity((definition.properties().len() + 2) * 3 + 7);
    lines.push(MessageSection::Subject.label().to_owned());
    lines.push("scope:".to_owned());
    lines.push(String::new());
    lines.push(String::new());
    lines.push("description:".to_owned());
    lines.push(String::new());
    lines.push(String::new());
    lines.push(String::new());
    lines.push(MessageSection::Body.label().to_owned());
    for (index, property) in definition.properties().iter().enumerate() {
        lines.push(format!("{}:", property.key()));
        lines.push(String::new());
        lines.push(String::new());
        if index + 1 == definition.properties().len() {
            lines.push(String::new());
        }
    }
    if definition.properties().is_empty() {
        lines.push(String::new());
    }
    lines.push(MessageSection::Footer.label().to_owned());
    lines.push("breaking-change:".to_owned());
    lines.push(String::new());
    lines.push(String::new());
    lines
}

fn parse_document(lines: &[String], definition: &CommitTypeDefinition) -> ParsedDocument {
    let mut parsed = ParsedDocument::default();
    let mut boundaries = Vec::new();
    for (line_index, line) in lines.iter().enumerate() {
        if let Some(marker) = structural_marker(line, definition) {
            let kind = match marker {
                DocumentMarker::Field(id) => BoundaryKind::Known(id),
                DocumentMarker::Section(_) => BoundaryKind::Group,
            };
            boundaries.push(Boundary {
                line: line_index,
                kind,
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
            FieldId::Description => FieldKind::Description,
            FieldId::Property(definition_index) => {
                let value_index = occurrences[definition_index];
                occurrences[definition_index] += 1;
                FieldKind::Property {
                    definition_index,
                    value_index,
                }
            }
            FieldId::BreakingChange => FieldKind::BreakingChange,
        };
        parsed.sections.push(DocumentSection {
            kind,
            heading_line: boundary.line,
            end_line,
            text,
        });
    }

    for id in std::iter::once(FieldId::Scope)
        .chain(std::iter::once(FieldId::Description))
        .chain((0..definition.properties().len()).map(FieldId::Property))
        .chain(std::iter::once(FieldId::BreakingChange))
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

fn structural_marker(line: &str, definition: &CommitTypeDefinition) -> Option<DocumentMarker> {
    let section = [
        MessageSection::Subject,
        MessageSection::Body,
        MessageSection::Footer,
    ]
    .into_iter()
    .find(|section| line == section.label())
    .map(DocumentMarker::Section);
    section.or_else(|| exact_heading(line, definition).map(DocumentMarker::Field))
}

fn exact_heading(line: &str, definition: &CommitTypeDefinition) -> Option<FieldId> {
    match line {
        "scope:" => Some(FieldId::Scope),
        "description:" => Some(FieldId::Description),
        "breaking-change:" => Some(FieldId::BreakingChange),
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
        || trimmed == "description"
        || trimmed == "breaking-change"
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
    lines[start..end]
        .iter()
        .map(|line| line.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
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

fn parse_description(
    parsed: &ParsedDocument,
    issues: &mut Vec<ValidationIssue>,
) -> Option<CommitSubject> {
    let Some(section) = parsed
        .sections
        .iter()
        .find(|section| section.kind == FieldKind::Description)
    else {
        issues.push(ValidationIssue {
            field: Some(FieldId::Description),
            line: 0,
            message: "restore the description field header".to_owned(),
        });
        return None;
    };
    match CommitSubject::new(&section.text) {
        Ok(subject) => Some(subject),
        Err(error) => {
            issues.push(ValidationIssue {
                field: Some(FieldId::Description),
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

fn parse_breaking_change(
    parsed: &ParsedDocument,
    issues: &mut Vec<ValidationIssue>,
) -> Option<PropertyValue> {
    let section = parsed
        .sections
        .iter()
        .find(|section| section.kind == FieldKind::BreakingChange)?;
    if section.text.is_empty() {
        return None;
    }
    match PropertyValue::new(&section.text) {
        Ok(value) => Some(value),
        Err(error) => {
            issues.push(ValidationIssue {
                field: Some(FieldId::BreakingChange),
                line: section.heading_line + 1,
                message: error.to_string(),
            });
            None
        }
    }
}

fn field_name(id: FieldId, definition: &CommitTypeDefinition) -> String {
    match id {
        FieldId::Scope => "scope".to_owned(),
        FieldId::Description => "description".to_owned(),
        FieldId::Property(index) => definition.properties()[index].key().to_string(),
        FieldId::BreakingChange => "breaking-change".to_owned(),
    }
}

pub(crate) struct ReviewState {
    pub(crate) draft: CommitDraft,
    pub(crate) message: CommitMessage,
    pub(crate) scroll: u16,
    pub(crate) scrollable: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ConfirmationButtons {
    pub(crate) discard: Rect,
    pub(crate) keep_editing: Rect,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum TypeCatalogKind {
    #[default]
    Conventional,
    Template(usize),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CatalogTab {
    pub(crate) kind: TypeCatalogKind,
    pub(crate) area: Rect,
}

pub(crate) struct AuthoringSession<'a> {
    context: Option<&'a CommitAuthoringContext>,
    pub(crate) template: Option<&'a CommitTemplate>,
    pub(crate) approved_message: Option<CommitMessage>,
    pub(crate) definitions: &'a [CommitTypeDefinition],
    pub(crate) selected_type: usize,
    pub(crate) type_catalog: TypeCatalogKind,
    pub(crate) preselected: bool,
    pub(crate) stage: Stage,
    pub(crate) composer: ComposerState,
    pub(crate) review: Option<ReviewState>,
    pub(crate) confirmation: ConfirmationAction,
    pub(crate) confirmation_buttons: Option<ConfirmationButtons>,
    pub(crate) catalog_tabs: Vec<CatalogTab>,
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
            context: None,
            template: None,
            approved_message: None,
            selected_type,
            type_catalog: TypeCatalogKind::Conventional,
            preselected: preselected_index.is_some(),
            stage: if preselected_index.is_some() {
                Stage::Compose
            } else {
                Stage::SelectType
            },
            composer: ComposerState::new(&definitions[selected_type]),
            review: None,
            confirmation: ConfirmationAction::Cancel,
            confirmation_buttons: None,
            catalog_tabs: Vec::new(),
            confirmation_resume: ResumeStage::Compose,
            too_small: false,
        }
    }

    pub(crate) fn definition(&self) -> &CommitTypeDefinition {
        &self.definitions[self.selected_type]
    }

    pub(crate) fn with_context(context: &'a CommitAuthoringContext) -> Self {
        let template = context.initial_template();
        let preselected = context.preselected_type().and_then(|selected| {
            template
                .definitions()
                .iter()
                .position(|definition| definition.id() == selected.id())
        });
        let mut session = Self::new(template.definitions(), preselected);
        session.template = Some(template);
        session.context = Some(context);
        session.type_catalog = TypeCatalogKind::Template(
            context
                .templates()
                .iter()
                .position(|item| item.id() == template.id())
                .unwrap_or(0),
        );
        session
    }

    pub(crate) fn available_type_catalogs(&self) -> Vec<TypeCatalogKind> {
        self.context.map_or_else(
            || vec![TypeCatalogKind::Conventional],
            |context| {
                (0..context.templates().len())
                    .map(TypeCatalogKind::Template)
                    .collect()
            },
        )
    }

    pub(crate) fn catalog_label(&self, kind: TypeCatalogKind) -> String {
        match kind {
            TypeCatalogKind::Conventional => "CONVENTIONAL".to_owned(),
            TypeCatalogKind::Template(index) => self
                .context
                .and_then(|context| context.templates().get(index))
                .map_or_else(String::new, |template| template.id().to_string()),
        }
    }

    fn cycle_type_catalog(&mut self) {
        let catalogs = self.available_type_catalogs();
        let current = catalogs
            .iter()
            .position(|kind| *kind == self.type_catalog)
            .unwrap_or(0);
        self.select_type_catalog(catalogs[(current + 1) % catalogs.len()]);
    }

    fn select_type_catalog(&mut self, kind: TypeCatalogKind) {
        if self.preselected || self.type_catalog == kind {
            return;
        }
        if let TypeCatalogKind::Template(index) = kind {
            let Some(template) = self
                .context
                .and_then(|context| context.templates().get(index))
            else {
                return;
            };
            self.template = Some(template);
            self.definitions = template.definitions();
            self.selected_type = 0;
            self.composer = ComposerState::new(self.definition());
            self.review = None;
            self.approved_message = None;
        }
        self.type_catalog = kind;
    }

    pub(crate) fn visible_stage(&self) -> Stage {
        if self.stage != Stage::Confirm {
            return self.stage;
        }
        match self.confirmation_resume {
            ResumeStage::Compose => Stage::Compose,
            ResumeStage::Review => Stage::Review,
        }
    }

    pub(crate) fn handle_event(&mut self, event: Event) -> Option<CommitDraftAuthorOutcome> {
        if self.too_small {
            return self.handle_too_small(&event);
        }
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => self.handle_key(key),
            Event::Mouse(mouse) => self.handle_mouse(mouse),
            Event::Paste(text) if self.stage == Stage::Compose => {
                let definition = self.definition().clone();
                if text.contains(['\n', '\r'])
                    && self
                        .composer
                        .current_field(&definition)
                        .is_some_and(|field| {
                            matches!(field, FieldKind::Scope | FieldKind::Description)
                        })
                {
                    return None;
                }
                self.composer
                    .edit_preserving_headings(&definition, |editor| {
                        editor.insert_str(text);
                    });
                None
            }
            Event::Resize(_, _)
            | Event::FocusGained
            | Event::FocusLost
            | Event::Paste(_)
            | Event::Key(_) => None,
        }
    }

    fn handle_too_small(&mut self, event: &Event) -> Option<CommitDraftAuthorOutcome> {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                if self.stage == Stage::Confirm {
                    self.handle_confirmation_key(*key)
                } else if matches!(key.code, KeyCode::Esc | KeyCode::Char('q'))
                    || control(*key, 'c')
                {
                    self.request_confirmation(ConfirmationAction::Cancel, ResumeStage::Compose)
                } else {
                    None
                }
            }
            Event::Mouse(mouse) => self.handle_mouse(*mouse),
            _ => None,
        }
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) -> Option<CommitDraftAuthorOutcome> {
        if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return None;
        }
        if self.stage == Stage::Confirm {
            let buttons = self.confirmation_buttons?;
            if contains(buttons.discard, mouse.column, mouse.row) {
                return self.confirm_discard();
            }
            if contains(buttons.keep_editing, mouse.column, mouse.row) {
                self.resume_after_confirmation();
            }
            return None;
        }
        if self.stage == Stage::SelectType
            && let Some(tab) = self
                .catalog_tabs
                .iter()
                .find(|tab| contains(tab.area, mouse.column, mouse.row))
        {
            self.select_type_catalog(tab.kind);
        }
        None
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
            KeyCode::Up => {
                self.selected_type = if self.selected_type == 0 {
                    self.definitions.len() - 1
                } else {
                    self.selected_type - 1
                };
            }
            KeyCode::Down => {
                self.selected_type = (self.selected_type + 1) % self.definitions.len();
            }
            KeyCode::Tab => self.cycle_type_catalog(),
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
        if key.modifiers.contains(KeyModifiers::ALT)
            && matches!(key.code, KeyCode::Char('a' | 'n'))
            && let Some(FieldKind::Property {
                definition_index, ..
            }) = self.composer.current_field(self.definition())
            && matches!(
                self.definition().properties()[definition_index].requirement(),
                PropertyRequirement::Conditional(_)
            )
        {
            self.composer.applicability[definition_index] =
                Some(if key.code == KeyCode::Char('a') {
                    ConditionalApplicability::Applies
                } else {
                    ConditionalApplicability::DoesNotApply
                });
            self.composer.issues.clear();
            return None;
        }
        if control(key, 's') {
            let definition = self.definition().clone();
            if let Some((draft, message)) = self.composer.validate(&definition) {
                let message = if let Some(template) = self.template {
                    match template.render(&draft) {
                        Ok(message) => message,
                        Err(error) => {
                            self.composer.issues = vec![ValidationIssue {
                                field: None,
                                line: self.composer.editor.cursor().0,
                                message: error.to_string(),
                            }];
                            return None;
                        }
                    }
                } else {
                    message
                };
                self.approved_message = None;
                self.review = Some(ReviewState {
                    draft,
                    message,
                    scroll: 0,
                    scrollable: false,
                });
                self.stage = Stage::Review;
            }
            return None;
        }
        if control(key, 'c') {
            return self.request_confirmation(ConfirmationAction::Cancel, ResumeStage::Compose);
        }
        if key.code == KeyCode::Esc {
            let action = if self.preselected {
                ConfirmationAction::Cancel
            } else {
                ConfirmationAction::ChangeType
            };
            return self.request_confirmation(action, ResumeStage::Compose);
        }
        self.input_key(key);
        None
    }

    fn input_key(&mut self, key: KeyEvent) {
        let definition = self.definition().clone();
        if key.code == KeyCode::Enter && self.composer.advance_on_enter(&definition) {
            return;
        }
        if matches!(key.code, KeyCode::Left | KeyCode::Right) {
            self.composer
                .move_horizontally_within_field(&definition, key);
            return;
        }
        if !key.modifiers.contains(KeyModifiers::SHIFT) {
            let movement = match key.code {
                KeyCode::Up => Some(CursorMove::Up),
                KeyCode::Down => Some(CursorMove::Down),
                _ => None,
            };
            if let Some(movement) = movement {
                self.composer.move_cursor(&definition, movement);
                return;
            }
        }
        if matches!(key.code, KeyCode::Backspace | KeyCode::Delete) {
            let (line, column) = self.composer.editor.cursor();
            let join_empty_previous = column == 0
                && line > 0
                && self
                    .composer
                    .editor
                    .lines()
                    .get(line - 1)
                    .is_some_and(String::is_empty)
                && self
                    .composer
                    .editor
                    .lines()
                    .get(line)
                    .is_some_and(|current| structural_marker(current, &definition).is_none());
            self.composer
                .edit_within_current_field(&definition, |editor| {
                    if join_empty_previous {
                        editor.delete_char();
                    } else {
                        editor.input(Input::from(key));
                    }
                });
            return;
        }
        self.composer
            .edit_preserving_headings(&definition, |editor| {
                editor.input(Input::from(key));
            });
    }

    fn handle_review_key(&mut self, key: KeyEvent) -> Option<CommitDraftAuthorOutcome> {
        match key.code {
            KeyCode::Enter => {
                let review = self.review.take()?;
                self.approved_message = Some(review.message);
                return Some(CommitDraftAuthorOutcome::Authored(review.draft));
            }
            KeyCode::Esc => {
                self.review = None;
                self.stage = Stage::Compose;
            }
            KeyCode::Char('q') => {
                return self.request_confirmation(ConfirmationAction::Cancel, ResumeStage::Review);
            }
            KeyCode::Up => {
                if let Some(review) = &mut self.review
                    && review.scrollable
                {
                    review.scroll = review.scroll.saturating_sub(1);
                }
            }
            KeyCode::Down => {
                if let Some(review) = &mut self.review
                    && review.scrollable
                {
                    review.scroll = review.scroll.saturating_add(1);
                }
            }
            KeyCode::PageUp => {
                if let Some(review) = &mut self.review
                    && review.scrollable
                {
                    review.scroll = review.scroll.saturating_sub(10);
                }
            }
            KeyCode::PageDown => {
                if let Some(review) = &mut self.review
                    && review.scrollable
                {
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
            KeyCode::Char('y') => self.confirm_discard(),
            KeyCode::Char('n') | KeyCode::Esc | KeyCode::Enter => {
                self.resume_after_confirmation();
                None
            }
            _ => None,
        }
    }

    fn confirm_discard(&mut self) -> Option<CommitDraftAuthorOutcome> {
        self.confirmation_buttons = None;
        match self.confirmation {
            ConfirmationAction::Cancel => Some(CommitDraftAuthorOutcome::Cancelled),
            ConfirmationAction::ChangeType => {
                self.composer = ComposerState::new(self.definition());
                self.review = None;
                self.stage = Stage::SelectType;
                None
            }
        }
    }

    fn resume_after_confirmation(&mut self) {
        self.confirmation_buttons = None;
        self.stage = match self.confirmation_resume {
            ResumeStage::Compose => Stage::Compose,
            ResumeStage::Review => Stage::Review,
        };
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
        self.confirmation_buttons = None;
        self.stage = Stage::Confirm;
        None
    }
}

fn contains(area: Rect, column: u16, row: u16) -> bool {
    column >= area.x && column < area.right() && row >= area.y && row < area.bottom()
}

fn control(key: KeyEvent, character: char) -> bool {
    key.code == KeyCode::Char(character) && key.modifiers.contains(KeyModifiers::CONTROL)
}
