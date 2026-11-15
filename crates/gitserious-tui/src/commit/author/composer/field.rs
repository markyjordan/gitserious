//! The composer's fields: which one, its text and requirement, its marker,
//! and the review's feedback on it.

use gitserious_core::{ConditionalApplicability, PropertyRequirement, ValidationSeverity};
use tui_textarea::TextArea;

/// A field, in the order the page shows them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MessageFieldId {
    Subject,
    /// The property at this index of the type's definition.
    Property(usize),
    BreakingChange,
}

/// How the type asks for a property, with the answer to a conditional one.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FieldRequirementState {
    Required,
    Recommended,
    Optional,
    /// Asked for only when it applies, which stays `None` until the user says.
    Conditional(Option<ConditionalApplicability>),
}

impl FieldRequirementState {
    pub(super) const fn new(requirement: &PropertyRequirement) -> Self {
        match requirement {
            PropertyRequirement::Required => Self::Required,
            PropertyRequirement::Recommended => Self::Recommended,
            PropertyRequirement::Optional => Self::Optional,
            PropertyRequirement::Conditional(_) => Self::Conditional(None),
        }
    }

    /// The word on the right of the field.
    pub(crate) const fn word(self) -> &'static str {
        match self {
            Self::Required => "required",
            Self::Recommended => "recommended",
            Self::Optional => "optional",
            Self::Conditional(None) => "conditional",
            Self::Conditional(Some(ConditionalApplicability::Applies)) => "applies",
            Self::Conditional(Some(ConditionalApplicability::DoesNotApply)) => "doesn't apply",
        }
    }

    pub(super) const fn decision(self) -> Option<ConditionalApplicability> {
        match self {
            Self::Conditional(decision) => decision,
            Self::Required | Self::Recommended | Self::Optional => None,
        }
    }
}

/// A property's field: its text, with its own undo history, and how the type
/// asks for it.
pub(crate) struct BodyFieldState {
    pub(crate) body: TextArea<'static>,
    pub(crate) requirement: FieldRequirementState,
}

impl BodyFieldState {
    pub(super) fn new(requirement: FieldRequirementState) -> Self {
        Self {
            body: TextArea::default(),
            requirement,
        }
    }
}

/// The glyph left of a field's key.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FieldMarker {
    /// ✓ Nothing is left to write.
    Done,
    /// ● The field being written.
    Focused,
    /// ◐ A conditional property nobody has said applies or not.
    Undecided,
    /// ○ Empty, or a subject that does not read `(scope): description` yet.
    Open,
}

/// What the last review attempt said about a field, shown under it until the
/// field changes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FieldFeedback {
    pub(crate) field: MessageFieldId,
    pub(crate) severity: ValidationSeverity,
    pub(crate) message: String,
}
