use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{self, Display, Formatter, Write as _};

use crate::{
    AuthoredProperty, CommitDraft, CommitDraftError, CommitScope, CommitScopeError, CommitSubject,
    CommitSubjectError, CommitTypeDefinition, CommitTypeId, IdentifierError, PropertyKey,
    PropertyMultiplicity, PropertyRequirement, PropertyValue, PropertyValues,
};

pub(crate) const SUBJECT_PLACEHOLDER: &str = "<subject>";
const SCOPE_PLACEHOLDER: &str = "<optional-scope>";
const VALUE_PLACEHOLDER: &str = "<value>";

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

/// Renders the editor document for one selected commit type.
#[must_use]
pub fn render_commit_editor_document(definition: &CommitTypeDefinition) -> String {
    let mut document = format!(
        "{}({SCOPE_PLACEHOLDER}): {SUBJECT_PLACEHOLDER}\n\n\
         # Replace <subject>; remove the scope parentheses when no scope applies.\n\
         # Complete every required property. Delete unused placeholder lines.\n\
         # Continuation lines must remain indented by two spaces.\n",
        definition.id()
    );

    for property in definition.properties() {
        let requirement = match property.requirement() {
            PropertyRequirement::Required => "required".to_owned(),
            PropertyRequirement::Recommended => "recommended".to_owned(),
            PropertyRequirement::Optional => "optional".to_owned(),
            PropertyRequirement::Conditional(condition) => {
                format!("conditional: {}", condition.id())
            }
        };
        let _ = write!(
            document,
            "\n# [{requirement}] {}\n{}:\n  {VALUE_PLACEHOLDER}\n",
            property.description(),
            property.key()
        );
        if let PropertyRequirement::Conditional(condition) = property.requirement() {
            let _ = writeln!(document, "# {}", condition.rationale());
        }
    }

    document
}

/// Returns whether an editor document contains no authored, non-comment text.
#[must_use]
pub fn commit_editor_document_is_empty(document: &str) -> bool {
    document
        .lines()
        .all(|line| line.trim().is_empty() || line.starts_with('#'))
}

/// Adds validation failures as editor comments without changing authored text.
#[must_use]
pub fn annotate_commit_editor_document(document: &str, errors: &CommitDocumentErrors) -> String {
    let mut annotated = String::from("# gitserious could not use this draft:\n");
    for error in errors.as_slice() {
        let _ = writeln!(annotated, "# - {error}");
    }
    annotated.push_str("# Correct the fields below, save, and close the editor.\n\n");
    annotated.push_str(document);
    annotated
}

/// Parses and validates an edited document against the selected schema.
///
/// # Errors
///
/// Returns [`CommitDocumentErrors`] with every independently discoverable
/// syntax and schema failure.
pub fn parse_commit_editor_document(
    definition: &CommitTypeDefinition,
    document: &str,
) -> Result<CommitDraft, CommitDocumentErrors> {
    let mut errors = Vec::new();
    let content = document
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.starts_with('#'))
        .collect::<Vec<_>>();
    let Some((header_position, (_, header))) = content
        .iter()
        .enumerate()
        .find(|(_, (_, line))| !line.trim().is_empty())
    else {
        return Err(CommitDocumentErrors::new(vec![
            CommitDocumentError::MissingHeader,
        ]));
    };

    let header = parse_header(header, &mut errors);
    let blocks = parse_property_blocks(&content[header_position + 1..], &mut errors);
    let properties = build_properties(definition, blocks, &mut errors);

    let draft = header.and_then(|(commit_type, scope, subject)| {
        CommitDraft::new(commit_type, scope, subject, properties)
            .map_err(|error| errors.push(CommitDocumentError::Draft(error)))
            .ok()
    });

    if let Some(draft) = draft {
        errors.extend(
            validate_commit_draft(definition, &draft)
                .err()
                .into_iter()
                .flat_map(CommitValidationErrors::into_errors)
                .map(CommitDocumentError::Validation),
        );
        if errors.is_empty() {
            return Ok(draft);
        }
    }

    Err(CommitDocumentErrors::new(errors))
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
        let _ = write!(message, "({scope})");
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
                let _ = writeln!(message, "  {line}");
            }
        }
    }

    Ok(CommitMessage(message.into_boxed_str()))
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
    let mut errors = Vec::new();
    if definition.id() != draft.commit_type() {
        errors.push(CommitValidationError::TypeMismatch {
            expected: definition.id().clone(),
            actual: draft.commit_type().clone(),
        });
    }

    for property in draft.properties() {
        let Some(expected) = definition
            .properties()
            .iter()
            .find(|candidate| candidate.key() == property.key())
        else {
            errors.push(CommitValidationError::UnknownProperty(
                property.key().clone(),
            ));
            continue;
        };
        if expected.multiplicity() != property.values().multiplicity() {
            errors.push(CommitValidationError::Multiplicity {
                key: property.key().clone(),
                expected: expected.multiplicity(),
                actual: property.values().multiplicity(),
            });
        }
    }

    for property in definition.properties() {
        if matches!(property.requirement(), PropertyRequirement::Required)
            && !draft
                .properties()
                .iter()
                .any(|authored| authored.key() == property.key())
        {
            errors.push(CommitValidationError::MissingRequired(
                property.key().clone(),
            ));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(CommitValidationErrors::new(errors))
    }
}

fn parse_header(
    header: &str,
    errors: &mut Vec<CommitDocumentError>,
) -> Option<(CommitTypeId, Option<CommitScope>, CommitSubject)> {
    let Some((identity, subject)) = header.split_once(": ") else {
        errors.push(CommitDocumentError::MalformedHeader);
        return None;
    };

    let (commit_type, scope) = if let Some(open) = identity.find('(') {
        if !identity.ends_with(')') {
            errors.push(CommitDocumentError::MalformedHeader);
            return None;
        }
        (
            &identity[..open],
            Some(&identity[open + 1..identity.len() - 1]),
        )
    } else {
        (identity, None)
    };

    let commit_type = match CommitTypeId::new(commit_type) {
        Ok(commit_type) => Some(commit_type),
        Err(error) => {
            errors.push(CommitDocumentError::InvalidCommitType(error));
            None
        }
    };
    let scope = match scope {
        None | Some("" | SCOPE_PLACEHOLDER) => Some(None),
        Some(scope) => match CommitScope::new(scope) {
            Ok(scope) => Some(Some(scope)),
            Err(error) => {
                errors.push(CommitDocumentError::InvalidScope(error));
                None
            }
        },
    };
    let subject = match CommitSubject::new(subject) {
        Ok(subject) => Some(subject),
        Err(error) => {
            errors.push(CommitDocumentError::InvalidSubject(error));
            None
        }
    };

    Some((commit_type?, scope?, subject?))
}

#[derive(Debug)]
struct PropertyBlock {
    key: PropertyKey,
    values: Vec<String>,
}

fn parse_property_blocks(
    lines: &[(usize, &str)],
    errors: &mut Vec<CommitDocumentError>,
) -> Vec<PropertyBlock> {
    let mut blocks = Vec::new();
    let mut current: Option<PropertyBlock> = None;

    for (zero_based_line, line) in lines {
        let line_number = zero_based_line + 1;
        if let Some(value) = line.strip_prefix("  ") {
            if let Some(block) = current.as_mut() {
                block.values.push(value.to_owned());
            } else if !value.trim().is_empty() {
                errors.push(CommitDocumentError::OrphanContinuation(line_number));
            }
            continue;
        }
        if line.trim().is_empty() {
            if let Some(block) = current.take() {
                blocks.push(block);
            }
            continue;
        }
        if let Some(key) = line.strip_suffix(':') {
            if let Some(block) = current.take() {
                blocks.push(block);
            }
            match PropertyKey::new(key) {
                Ok(key) => {
                    current = Some(PropertyBlock {
                        key,
                        values: Vec::new(),
                    });
                }
                Err(error) => errors.push(CommitDocumentError::InvalidPropertyKey {
                    line: line_number,
                    source: error,
                }),
            }
            continue;
        }
        errors.push(CommitDocumentError::UnexpectedLine(line_number));
    }
    if let Some(block) = current {
        blocks.push(block);
    }
    blocks
}

fn build_properties(
    definition: &CommitTypeDefinition,
    blocks: Vec<PropertyBlock>,
    errors: &mut Vec<CommitDocumentError>,
) -> Vec<AuthoredProperty> {
    let mut grouped = BTreeMap::<PropertyKey, Vec<PropertyValue>>::new();
    for mut block in blocks {
        while block.values.first().is_some_and(String::is_empty) {
            block.values.remove(0);
        }
        while block.values.last().is_some_and(String::is_empty) {
            block.values.pop();
        }
        if block.values.len() == 1
            && block
                .values
                .first()
                .is_some_and(|value| value == VALUE_PLACEHOLDER)
        {
            block.values.clear();
        }
        if block.values.is_empty() {
            grouped.entry(block.key).or_default();
            continue;
        }
        let text = block.values.join("\n");
        match PropertyValue::new(text) {
            Ok(value) => grouped.entry(block.key).or_default().push(value),
            Err(_) => errors.push(CommitDocumentError::BlankProperty(block.key)),
        }
    }

    let mut properties = Vec::new();
    for (key, values) in grouped {
        let Some(property_definition) = definition
            .properties()
            .iter()
            .find(|property| property.key() == &key)
        else {
            errors.push(CommitDocumentError::Validation(
                CommitValidationError::UnknownProperty(key),
            ));
            continue;
        };
        if values.is_empty() {
            continue;
        }
        let multiplicity = property_definition.multiplicity();
        let property_values = match multiplicity {
            PropertyMultiplicity::Single if values.len() == 1 => {
                let mut values = values.into_iter();
                if let Some(value) = values.next() {
                    PropertyValues::single(value)
                } else {
                    continue;
                }
            }
            PropertyMultiplicity::Single | PropertyMultiplicity::Multiple => {
                match PropertyValues::multiple(values) {
                    Ok(values) => values,
                    Err(_) => continue,
                }
            }
        };
        properties.push(AuthoredProperty::new(key, property_values));
    }
    properties
}

/// One editor-document syntax or schema failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommitDocumentError {
    /// No non-comment header was supplied.
    MissingHeader,
    /// The header does not use `type(scope): subject` or `type: subject`.
    MalformedHeader,
    /// The header contains an invalid open commit-type identifier.
    InvalidCommitType(IdentifierError),
    /// The header contains an invalid scope.
    InvalidScope(CommitScopeError),
    /// The header contains an invalid subject.
    InvalidSubject(CommitSubjectError),
    /// A property key is not a valid open identifier.
    InvalidPropertyKey {
        /// One-based editor line number.
        line: usize,
        /// Identifier validation failure.
        source: IdentifierError,
    },
    /// An indented value has no preceding property key.
    OrphanContinuation(usize),
    /// A non-comment line is neither a property key nor an indented value.
    UnexpectedLine(usize),
    /// A property block contains no non-whitespace value.
    BlankProperty(PropertyKey),
    /// The parsed draft violated its own aggregate invariant.
    Draft(CommitDraftError),
    /// The parsed draft violated the selected schema.
    Validation(CommitValidationError),
}

impl Display for CommitDocumentError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingHeader => formatter.write_str("add a commit header"),
            Self::MalformedHeader => {
                formatter.write_str("header must use `type(scope): subject` or `type: subject`")
            }
            Self::InvalidCommitType(error) => Display::fmt(error, formatter),
            Self::InvalidScope(error) => Display::fmt(error, formatter),
            Self::InvalidSubject(error) => Display::fmt(error, formatter),
            Self::InvalidPropertyKey { line, source } => {
                write!(formatter, "line {line}: {source}")
            }
            Self::OrphanContinuation(line) => {
                write!(formatter, "line {line}: indented value has no property key")
            }
            Self::UnexpectedLine(line) => {
                write!(
                    formatter,
                    "line {line}: expected a property key or indented value"
                )
            }
            Self::BlankProperty(key) => write!(formatter, "property {key:?} is blank"),
            Self::Draft(error) => Display::fmt(error, formatter),
            Self::Validation(error) => Display::fmt(error, formatter),
        }
    }
}

impl Error for CommitDocumentError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidCommitType(error) | Self::InvalidPropertyKey { source: error, .. } => {
                Some(error)
            }
            Self::InvalidScope(error) => Some(error),
            Self::InvalidSubject(error) => Some(error),
            Self::Draft(error) => Some(error),
            Self::Validation(error) => Some(error),
            Self::MissingHeader
            | Self::MalformedHeader
            | Self::OrphanContinuation(_)
            | Self::UnexpectedLine(_)
            | Self::BlankProperty(_) => None,
        }
    }
}

/// Every independently discoverable editor-document failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitDocumentErrors(Vec<CommitDocumentError>);

impl CommitDocumentErrors {
    fn new(errors: Vec<CommitDocumentError>) -> Self {
        Self(errors)
    }

    /// Returns failures in discovery order.
    #[must_use]
    pub fn as_slice(&self) -> &[CommitDocumentError] {
        &self.0
    }
}

impl Display for CommitDocumentErrors {
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

impl Error for CommitDocumentErrors {}

/// One violation between an authored draft and its selected schema.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommitValidationError {
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

    fn into_errors(self) -> impl Iterator<Item = CommitValidationError> {
        self.0.into_iter()
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
