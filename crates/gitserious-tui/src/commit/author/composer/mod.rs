//! The composer a commit is written in: the subject line, one field for each
//! property of the chosen type, then breaking-change.

mod field;
mod input;
mod layout;
mod render;
#[cfg(test)]
mod tests;

pub(crate) use field::{
    BodyFieldState, FieldFeedback, FieldMarker, FieldRequirementState, MessageFieldId,
};
pub(crate) use render::render;

use gitserious_app::CommitTaxonomy;
use gitserious_core::{
    CommitDraft, CommitMessage, CommitScope, CommitSubject, CommitSubjectError,
    CommitTypeDefinition, CommitValidationError, ConditionalApplicability, PropertyKey,
    PropertyRequirement, PropertyResponse, PropertyValidationIssueKind, PropertyValue,
    ValidationSeverity, validate_commit_draft,
};
use tui_textarea::TextArea;

use layout::ComposerLayout;

pub(crate) struct MessageComposer {
    definition: CommitTypeDefinition,
    /// What follows the type key on the subject line: `(scope): description`
    /// or `: description`. The type itself is not text, so it cannot be edited
    /// into another type.
    pub(crate) subject: TextArea<'static>,
    pub(crate) properties: Vec<BodyFieldState>,
    pub(crate) breaking_change: BodyFieldState,
    pub(crate) focus: MessageFieldId,
    pub(crate) feedback: Vec<FieldFeedback>,
    layout: ComposerLayout,
}

impl MessageComposer {
    pub(crate) fn new(definition: CommitTypeDefinition) -> Self {
        let properties = definition
            .properties()
            .iter()
            .map(|property| BodyFieldState::new(FieldRequirementState::new(property.requirement())))
            .collect();
        Self {
            definition,
            subject: TextArea::default(),
            properties,
            breaking_change: BodyFieldState::new(FieldRequirementState::Optional),
            focus: MessageFieldId::Subject,
            feedback: Vec::new(),
            layout: ComposerLayout::default(),
        }
    }

    /// Keeps the subject, breaking change, and every property field whose key
    /// `definition` also asks for. A carried body makes a conditional property
    /// apply. A decision carries only when the new property is conditional too.
    pub(crate) fn retarget(&mut self, definition: CommitTypeDefinition) {
        let focus_key = match self.focus {
            MessageFieldId::Subject => None,
            MessageFieldId::BreakingChange => Some(String::from("breaking-change")),
            MessageFieldId::Property(index) => self
                .definition
                .properties()
                .get(index)
                .map(|property| property.key().as_str().to_owned()),
        };
        let mut previous: Vec<(String, BodyFieldState)> = self
            .definition
            .properties()
            .iter()
            .map(|property| property.key().as_str().to_owned())
            .zip(std::mem::take(&mut self.properties))
            .collect();
        self.properties = definition
            .properties()
            .iter()
            .map(|property| {
                previous
                    .iter()
                    .position(|(key, _)| key == property.key().as_str())
                    .map_or_else(
                        || BodyFieldState::new(FieldRequirementState::new(property.requirement())),
                        |index| adopt(previous.swap_remove(index).1, property.requirement()),
                    )
            })
            .collect();
        self.definition = definition;
        self.feedback.clear();
        self.layout.scroll_offset = 0;
        self.focus = match focus_key.as_deref() {
            Some("breaking-change") => MessageFieldId::BreakingChange,
            Some(key) => self
                .definition
                .properties()
                .iter()
                .position(|property| property.key().as_str() == key)
                .map_or(MessageFieldId::Subject, MessageFieldId::Property),
            None => MessageFieldId::Subject,
        };
    }

    /// What the last frame measured.
    pub(crate) const fn layout(&self) -> ComposerLayout {
        self.layout
    }

    pub(crate) const fn definition(&self) -> &CommitTypeDefinition {
        &self.definition
    }

    /// Every field, top to bottom.
    pub(crate) fn ids(&self) -> impl Iterator<Item = MessageFieldId> + use<> {
        std::iter::once(MessageFieldId::Subject)
            .chain((0..self.properties.len()).map(MessageFieldId::Property))
            .chain(std::iter::once(MessageFieldId::BreakingChange))
    }

    /// The property field `id` names, which the subject is not.
    pub(crate) fn field(&self, id: MessageFieldId) -> Option<&BodyFieldState> {
        match id {
            MessageFieldId::Subject => None,
            MessageFieldId::Property(index) => self.properties.get(index),
            MessageFieldId::BreakingChange => Some(&self.breaking_change),
        }
    }

    fn field_mut(&mut self, id: MessageFieldId) -> Option<&mut BodyFieldState> {
        match id {
            MessageFieldId::Subject => None,
            MessageFieldId::Property(index) => self.properties.get_mut(index),
            MessageFieldId::BreakingChange => Some(&mut self.breaking_change),
        }
    }

    pub(crate) fn text(&self, id: MessageFieldId) -> &TextArea<'static> {
        self.field(id).map_or(&self.subject, |field| &field.body)
    }

    fn text_mut(&mut self, id: MessageFieldId) -> &mut TextArea<'static> {
        match id {
            MessageFieldId::Subject => &mut self.subject,
            MessageFieldId::Property(index) => &mut self.properties[index].body,
            MessageFieldId::BreakingChange => &mut self.breaking_change.body,
        }
    }

    /// The key a field shows, or `subject`.
    pub(crate) fn name(&self, id: MessageFieldId) -> &str {
        match id {
            MessageFieldId::Subject => "subject",
            MessageFieldId::Property(index) => self.definition.properties()[index].key().as_str(),
            MessageFieldId::BreakingChange => "breaking-change",
        }
    }

    /// What an empty field says it is for while it is focused.
    pub(crate) fn guidance(&self, id: MessageFieldId) -> &str {
        match id {
            MessageFieldId::Subject => "(scope): description",
            MessageFieldId::Property(index) => self.definition.properties()[index].description(),
            MessageFieldId::BreakingChange => {
                "An incompatible change. Review adds ! before the subject's colon and a BREAKING CHANGE footer."
            }
        }
    }

    /// Whether leaving now would drop anything the user wrote or decided.
    pub(crate) fn dirty(&self) -> bool {
        !self.subject.is_empty()
            || self
                .properties
                .iter()
                .chain(std::iter::once(&self.breaking_change))
                .any(|field| !field.body.is_empty() || field.requirement.decision().is_some())
    }

    pub(crate) fn marker(&self, id: MessageFieldId) -> FieldMarker {
        if id == self.focus {
            return FieldMarker::Focused;
        }
        let Some(field) = self.field(id) else {
            return if self.subject_parts().is_ok() {
                FieldMarker::Done
            } else {
                FieldMarker::Open
            };
        };
        match (field.requirement, value(&field.body).is_some()) {
            (FieldRequirementState::Conditional(None), _) => FieldMarker::Undecided,
            (
                FieldRequirementState::Conditional(Some(ConditionalApplicability::DoesNotApply)),
                _,
            )
            | (_, true) => FieldMarker::Done,
            (_, false) => FieldMarker::Open,
        }
    }

    /// The fields review waits on, top to bottom.
    pub(crate) fn to_go(&self) -> Vec<MessageFieldId> {
        self.ids().filter(|id| self.blocks(*id)).collect()
    }

    fn blocks(&self, id: MessageFieldId) -> bool {
        let Some(field) = self.field(id) else {
            return self.subject_parts().is_err();
        };
        let empty = value(&field.body).is_none();
        match field.requirement {
            FieldRequirementState::Required
            | FieldRequirementState::Conditional(Some(ConditionalApplicability::Applies)) => empty,
            FieldRequirementState::Conditional(None) => true,
            FieldRequirementState::Recommended
            | FieldRequirementState::Optional
            | FieldRequirementState::Conditional(Some(ConditionalApplicability::DoesNotApply)) => {
                false
            }
        }
    }

    /// Whether alt+a and alt+n answer the focused field.
    pub(crate) fn focused_conditional(&self) -> bool {
        self.field(self.focus)
            .is_some_and(|field| matches!(field.requirement, FieldRequirementState::Conditional(_)))
    }

    /// The subject line's scope and description.
    pub(crate) fn subject_parts(&self) -> Result<(Option<CommitScope>, CommitSubject), String> {
        parse_subject(&self.subject.lines()[0])
    }

    /// The recommendations the last review attempt made, top to bottom.
    pub(crate) fn recommendations(&self) -> impl Iterator<Item = &str> {
        self.feedback
            .iter()
            .filter(|feedback| feedback.severity == ValidationSeverity::Warning)
            .map(|feedback| feedback.message.as_str())
    }

    /// Validates the fields as a draft and renders its message, or notes what
    /// blocks it on the fields and focuses the first.
    pub(crate) fn review(
        &mut self,
        taxonomy: Option<&CommitTaxonomy>,
    ) -> Option<(CommitDraft, CommitMessage)> {
        self.feedback.clear();
        let draft = match self.draft() {
            Ok(draft) => draft,
            Err(message) => {
                self.add_feedback(MessageFieldId::Subject, ValidationSeverity::Error, message);
                return self.reject();
            }
        };
        let report = taxonomy.map_or_else(
            || validate_commit_draft(&self.definition, &draft),
            |taxonomy| taxonomy.validate(&draft),
        );
        for error in report.errors() {
            let field = self.error_field(error);
            self.add_feedback(field, ValidationSeverity::Error, error.to_string());
        }
        for warning in report.warnings() {
            let field = self.property_field(issue_key(warning.kind()));
            self.add_feedback(
                field,
                ValidationSeverity::Warning,
                warning.kind().to_string(),
            );
        }
        match report.into_message() {
            Ok(message) => Some((draft, message)),
            Err(_) => self.reject(),
        }
    }

    fn draft(&self) -> Result<CommitDraft, String> {
        let (scope, subject) = self.subject_parts()?;
        let responses = self
            .definition
            .properties()
            .iter()
            .zip(&self.properties)
            .map(|(property, field)| {
                PropertyResponse::new(
                    property.key().clone(),
                    value(&field.body),
                    field.requirement.decision(),
                )
            })
            .collect();
        let draft =
            CommitDraft::from_responses(self.definition.id().clone(), scope, subject, responses)
                .map_err(|error| error.to_string())?;
        Ok(match value(&self.breaking_change.body) {
            Some(breaking_change) => draft.with_breaking_change(breaking_change),
            None => draft,
        })
    }

    fn add_feedback(
        &mut self,
        field: MessageFieldId,
        severity: ValidationSeverity,
        message: String,
    ) {
        self.feedback.push(FieldFeedback {
            field,
            severity,
            message,
        });
    }

    fn reject(&mut self) -> Option<(CommitDraft, CommitMessage)> {
        if let Some(first) = self.ids().find(|id| {
            self.feedback.iter().any(|feedback| {
                feedback.field == *id && feedback.severity == ValidationSeverity::Error
            })
        }) {
            self.focus = first;
        }
        None
    }

    fn error_field(&self, error: &CommitValidationError) -> MessageFieldId {
        match error {
            CommitValidationError::UnknownProperty(key)
            | CommitValidationError::MissingRequired(key) => self.property_field(key),
            CommitValidationError::PropertyResponse(kind) => self.property_field(issue_key(kind)),
            CommitValidationError::UnknownCommitType { .. }
            | CommitValidationError::TypeMismatch { .. } => MessageFieldId::Subject,
        }
    }

    fn property_field(&self, key: &PropertyKey) -> MessageFieldId {
        self.definition
            .properties()
            .iter()
            .position(|property| property.key() == key)
            .map_or(MessageFieldId::Subject, MessageFieldId::Property)
    }
}

fn adopt(mut field: BodyFieldState, requirement: &PropertyRequirement) -> BodyFieldState {
    field.requirement = match (field.requirement, FieldRequirementState::new(requirement)) {
        (
            FieldRequirementState::Conditional(decision @ Some(_)),
            FieldRequirementState::Conditional(_),
        ) => FieldRequirementState::Conditional(decision),
        (_, FieldRequirementState::Conditional(_)) if value(&field.body).is_some() => {
            FieldRequirementState::Conditional(Some(ConditionalApplicability::Applies))
        }
        (_, requirement) => requirement,
    };
    field
}

/// Reads `(scope): description` or `: description`, the subject after its type.
fn parse_subject(tail: &str) -> Result<(Option<CommitScope>, CommitSubject), String> {
    if tail.trim().is_empty() {
        return Err(CommitSubjectError::Blank.to_string());
    }
    let (scope, rest) = match tail.strip_prefix('(') {
        Some(rest) => {
            let (scope, rest) = rest
                .split_once(')')
                .ok_or_else(|| "close the scope with \")\"".to_owned())?;
            let scope = CommitScope::new(scope.trim()).map_err(|error| error.to_string())?;
            (Some(scope), rest)
        }
        None => (None, tail),
    };
    let description = rest
        .strip_prefix(':')
        .ok_or_else(|| "put \": \" before the description".to_owned())?;
    let subject = CommitSubject::new(description.trim()).map_err(|error| error.to_string())?;
    Ok((scope, subject))
}

/// A field's text as the message carries it: no trailing whitespace on any
/// line and no blank lines around it.
fn value(text: &TextArea<'_>) -> Option<PropertyValue> {
    let lines = text.lines();
    let start = lines.iter().position(|line| !line.trim().is_empty())?;
    let end = lines.iter().rposition(|line| !line.trim().is_empty())?;
    PropertyValue::new(
        lines[start..=end]
            .iter()
            .map(|line| line.trim_end())
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .ok()
}

fn issue_key(kind: &PropertyValidationIssueKind) -> &PropertyKey {
    use PropertyValidationIssueKind as Kind;
    match kind {
        Kind::UnknownProperty(key)
        | Kind::DuplicateProperty(key)
        | Kind::MissingRequired(key)
        | Kind::MissingRecommended(key)
        | Kind::MissingConditionalDecision(key)
        | Kind::MissingApplicableValue(key)
        | Kind::ValueForNonApplicableProperty(key)
        | Kind::UnexpectedConditionalDecision(key) => key,
    }
}
