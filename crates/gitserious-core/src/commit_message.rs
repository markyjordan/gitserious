use std::borrow::Cow;
use std::error::Error;
use std::fmt::{self, Display, Formatter, Write as _};

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::{
    CommitDraft, CommitProvenance, CommitTypeDefinition, CommitTypeId, PropertyKey,
    PropertyMultiplicity, PropertyResponse, PropertyValidationIssue, PropertyValidationIssueKind,
    TemplateId, ValidationSeverity,
};

/// Maximum Unicode display width of canonical commit-message prose.
pub const COMMIT_MESSAGE_WIDTH: u16 = 80;

/// A canonical, schema-validated commit message.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CommitMessage(Box<str>);

impl CommitMessage {
    /// Returns the canonical message text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for CommitMessage {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Display for CommitMessage {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Validates and canonically renders an authored draft.
///
/// # Errors
///
/// Returns [`CommitValidationErrors`] when the draft does not satisfy the
/// selected commit-type schema.
pub fn render_commit_message(
    definition: &CommitTypeDefinition,
    draft: &CommitDraft,
) -> Result<CommitMessage, CommitValidationErrors> {
    validate_commit_draft(definition, draft)?;

    let mut message = draft.commit_type().to_string();
    if let Some(scope) = draft.scope() {
        let _ = write!(message, "({})", normalized_scope(scope.as_str()));
    }
    if draft.breaking_change().is_some() {
        message.push('!');
    }
    let _ = writeln!(message, ": {}", draft.subject());

    for definition_property in definition.properties() {
        let Some(authored) = draft
            .properties()
            .iter()
            .find(|property| property.key() == definition_property.key())
        else {
            continue;
        };
        for value in authored.values() {
            let _ = write!(message, "\n{}:\n", authored.key());
            for line in value.as_str().lines() {
                write_wrapped_line(&mut message, line);
            }
        }
    }

    if let Some(breaking_change) = draft.breaking_change() {
        let mut lines = breaking_change.as_str().lines();
        if let Some(first) = lines.next() {
            message.push('\n');
            write_wrapped_line(&mut message, &format!("BREAKING CHANGE: {first}"));
        }
        for line in lines {
            write_wrapped_line(&mut message, line);
        }
    }

    Ok(CommitMessage(message.into_boxed_str()))
}

/// Validates against the provenance schema and appends its canonical trailers.
///
/// The schema used for validation also supplies every identity and version in
/// the trailers. Fingerprint computation and verification belong to the caller
/// that resolves project policy; this function does not read mutable state.
/// Legacy drafts retain the validation behavior of [`render_commit_message`].
///
/// # Errors
///
/// Returns [`CommitValidationErrors`] if the draft type is absent from the
/// provenance schema or its properties do not satisfy the selected definition.
pub fn render_commit_message_with_provenance(
    provenance: &CommitProvenance,
    draft: &CommitDraft,
) -> Result<CommitMessage, CommitValidationErrors> {
    let schema = provenance.schema();
    let definition = schema
        .change_types()
        .iter()
        .find(|definition| definition.id() == draft.commit_type())
        .ok_or_else(|| {
            CommitValidationErrors::new(vec![CommitValidationError::UnknownCommitType {
                template: schema.template_id().clone(),
                actual: draft.commit_type().clone(),
            }])
        })?;
    let rendered = render_commit_message(&definition.commit_type_definition(), draft)?;
    let mut message = rendered.as_str().to_owned();
    message.push('\n');
    let _ = writeln!(
        message,
        "Gitserious-Template: {}@{}",
        schema.template_id(),
        schema.template_version()
    );
    let _ = writeln!(
        message,
        "Gitserious-Taxonomy: {}@{}",
        schema.taxonomy_id(),
        schema.taxonomy_version()
    );
    let _ = writeln!(
        message,
        "Gitserious-Typeset: {}/{}@{}",
        schema.taxonomy_id(),
        schema.typeset_id(),
        schema.typeset_version()
    );
    let _ = writeln!(message, "Gitserious-Schema: {}", provenance.fingerprint());
    Ok(CommitMessage(message.into_boxed_str()))
}

fn write_wrapped_line(message: &mut String, line: &str) {
    for wrapped in wrapped_line_segments(line) {
        let _ = writeln!(message, "{wrapped}");
    }
}

fn wrapped_line_segments(mut line: &str) -> Vec<&str> {
    let width = usize::from(COMMIT_MESSAGE_WIDTH);
    let mut wrapped = Vec::new();
    while UnicodeWidthStr::width(line) > width {
        let mut used = 0_usize;
        let mut fitting_end = 0_usize;
        let mut word_boundary = None;
        for (index, grapheme) in line.grapheme_indices(true) {
            let grapheme_width = UnicodeWidthStr::width(grapheme);
            if used.saturating_add(grapheme_width) > width {
                break;
            }
            used = used.saturating_add(grapheme_width);
            fitting_end = index.saturating_add(grapheme.len());
            if grapheme.chars().all(char::is_whitespace)
                && line[..index]
                    .chars()
                    .any(|character| !character.is_whitespace())
            {
                word_boundary = Some(index);
            }
        }

        if let Some(boundary) = word_boundary {
            wrapped.push(&line[..boundary]);
            line = line[boundary..].trim_start_matches(char::is_whitespace);
        } else if fitting_end > 0 {
            wrapped.push(&line[..fitting_end]);
            line = &line[fitting_end..];
        } else if let Some((_, grapheme)) = line.grapheme_indices(true).next() {
            wrapped.push(&line[..grapheme.len()]);
            line = &line[grapheme.len()..];
        } else {
            break;
        }
    }
    wrapped.push(line);
    wrapped
}

fn normalized_scope(scope: &str) -> String {
    scope.split_whitespace().collect::<Vec<_>>().join("-")
}

/// Validates a draft against a selected commit-type schema.
///
/// # Errors
///
/// Returns all type, property, requiredness, and multiplicity violations.
pub fn validate_commit_draft(
    definition: &CommitTypeDefinition,
    draft: &CommitDraft,
) -> Result<(), CommitValidationErrors> {
    let report = validate_commit_draft_report(definition, draft);
    if report.has_errors() {
        Err(CommitValidationErrors::new(report.errors))
    } else {
        Ok(())
    }
}

/// Errors and nonblocking recommendations for one authored draft.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CommitValidationReport {
    errors: Vec<CommitValidationError>,
    warnings: Vec<PropertyValidationIssue>,
}

impl CommitValidationReport {
    /// Returns blocking errors, with any header mismatch before property errors.
    #[must_use]
    pub fn errors(&self) -> &[CommitValidationError] {
        &self.errors
    }

    /// Returns nonblocking recommendations in schema order.
    #[must_use]
    pub fn warnings(&self) -> &[PropertyValidationIssue] {
        &self.warnings
    }

    /// Returns whether the draft must be repaired before rendering.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

/// Validates a draft and retains nonblocking recommended-property warnings.
///
/// Drafts created with [`CommitDraft::from_responses`] require explicit
/// applicability. Legacy drafts still permit omitted conditional properties;
/// adapters opt into the stricter contract when they can capture decisions.
#[must_use]
pub fn validate_commit_draft_report(
    definition: &CommitTypeDefinition,
    draft: &CommitDraft,
) -> CommitValidationReport {
    let mut report = CommitValidationReport::default();
    if definition.id() != draft.commit_type() {
        report.errors.push(CommitValidationError::TypeMismatch {
            expected: definition.id().clone(),
            actual: draft.commit_type().clone(),
        });
    }
    let responses: Cow<'_, [PropertyResponse]> = match draft.responses() {
        Some(responses) => Cow::Borrowed(responses),
        None => Cow::Owned(
            draft
                .properties()
                .iter()
                .map(|property| {
                    PropertyResponse::new(
                        property.key().clone(),
                        Some(property.values().clone()),
                        None,
                    )
                })
                .collect(),
        ),
    };
    let properties = crate::property_validation::validate_property_definitions(
        definition.properties(),
        &responses,
    );
    for issue in properties.issues() {
        if draft.responses().is_none()
            && matches!(
                issue.kind(),
                PropertyValidationIssueKind::MissingConditionalDecision(_)
            )
        {
            continue;
        }
        match issue.severity() {
            ValidationSeverity::Warning => report.warnings.push(issue.clone()),
            ValidationSeverity::Error => report.errors.push(match issue.kind() {
                PropertyValidationIssueKind::UnknownProperty(key) => {
                    CommitValidationError::UnknownProperty(key.clone())
                }
                PropertyValidationIssueKind::MissingRequired(key) => {
                    CommitValidationError::MissingRequired(key.clone())
                }
                PropertyValidationIssueKind::Multiplicity {
                    key,
                    expected,
                    actual,
                } => CommitValidationError::Multiplicity {
                    key: key.clone(),
                    expected: *expected,
                    actual: *actual,
                },
                kind => CommitValidationError::PropertyResponse(kind.clone()),
            }),
        }
    }
    report
}

/// One violation between an authored draft and its selected schema.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommitValidationError {
    /// The provenance schema does not contain the authored type.
    UnknownCommitType {
        /// Template that defines the available types.
        template: TemplateId,
        /// Rejected header type.
        actual: CommitTypeId,
    },
    /// An explicit response violates applicability or response structure.
    PropertyResponse(PropertyValidationIssueKind),
    /// The draft identifies a different type than the selected schema.
    TypeMismatch {
        /// Selected schema type.
        expected: CommitTypeId,
        /// Authored header type.
        actual: CommitTypeId,
    },
    /// The draft supplies a property absent from the selected schema.
    UnknownProperty(PropertyKey),
    /// A required property is absent.
    MissingRequired(PropertyKey),
    /// Authored values use a different multiplicity than the schema.
    Multiplicity {
        /// Property with the mismatch.
        key: PropertyKey,
        /// Schema multiplicity.
        expected: PropertyMultiplicity,
        /// Authored multiplicity.
        actual: PropertyMultiplicity,
    },
}

impl Display for CommitValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownCommitType { template, actual } => write!(
                formatter,
                "type {actual:?} is not available in template {template:?}"
            ),
            Self::PropertyResponse(kind) => Display::fmt(kind, formatter),
            Self::TypeMismatch { expected, actual } => write!(
                formatter,
                "selected type {expected:?} cannot use header type {actual:?}"
            ),
            Self::UnknownProperty(key) => {
                write!(
                    formatter,
                    "property {key:?} is not defined for the selected type"
                )
            }
            Self::MissingRequired(key) => {
                write!(formatter, "complete required property {key:?}")
            }
            Self::Multiplicity {
                key,
                expected,
                actual,
            } => write!(
                formatter,
                "property {key:?} requires {expected:?} values, not {actual:?} values"
            ),
        }
    }
}

impl Error for CommitValidationError {}

/// Every schema violation in an authored commit draft.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitValidationErrors(Vec<CommitValidationError>);

impl CommitValidationErrors {
    fn new(errors: Vec<CommitValidationError>) -> Self {
        Self(errors)
    }

    /// Returns failures in schema-validation order.
    #[must_use]
    pub fn as_slice(&self) -> &[CommitValidationError] {
        &self.0
    }
}

impl Display for CommitValidationErrors {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        for (index, error) in self.0.iter().enumerate() {
            if index > 0 {
                formatter.write_str("; ")?;
            }
            Display::fmt(error, formatter)?;
        }
        Ok(())
    }
}

impl Error for CommitValidationErrors {}
