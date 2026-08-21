use std::collections::BTreeSet;

use crate::{
    PropertyKey, PropertyMultiplicity, PropertyRequirement, PropertyValues, ResolvedChangeType,
};

/// An author's applicability decision for a conditional property.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConditionalApplicability {
    /// The property condition holds for this change.
    Applies,
    /// The property condition does not hold for this change.
    DoesNotApply,
}

/// One durable-property response supplied to taxonomy-driven validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PropertyResponse {
    key: PropertyKey,
    values: Option<PropertyValues>,
    applicability: Option<ConditionalApplicability>,
}

impl PropertyResponse {
    /// Creates a response. Cross-field consistency is reported by validation.
    #[must_use]
    pub const fn new(
        key: PropertyKey,
        values: Option<PropertyValues>,
        applicability: Option<ConditionalApplicability>,
    ) -> Self {
        Self {
            key,
            values,
            applicability,
        }
    }

    /// Returns the durable-property key.
    #[must_use]
    pub const fn key(&self) -> &PropertyKey {
        &self.key
    }

    /// Returns authored values, when present.
    #[must_use]
    pub const fn values(&self) -> Option<&PropertyValues> {
        self.values.as_ref()
    }

    /// Returns the explicit conditional decision, when supplied.
    #[must_use]
    pub const fn applicability(&self) -> Option<ConditionalApplicability> {
        self.applicability
    }
}

/// Whether a property-validation issue blocks acceptance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValidationSeverity {
    /// The response cannot be accepted.
    Error,
    /// The response is valid but omits recommended durable context.
    Warning,
}

/// One taxonomy-derived property validation outcome.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PropertyValidationIssue {
    severity: ValidationSeverity,
    kind: PropertyValidationIssueKind,
}

impl PropertyValidationIssue {
    /// Returns whether this issue blocks acceptance.
    #[must_use]
    pub const fn severity(&self) -> ValidationSeverity {
        self.severity
    }

    /// Returns the precise validation rule that produced the issue.
    #[must_use]
    pub const fn kind(&self) -> &PropertyValidationIssueKind {
        &self.kind
    }
}

/// A precise property response violation or recommendation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PropertyValidationIssueKind {
    /// A response names a property outside the resolved typeset.
    UnknownProperty(PropertyKey),
    /// More than one response uses the same key.
    DuplicateProperty(PropertyKey),
    /// A required value is absent.
    MissingRequired(PropertyKey),
    /// A recommended value is absent.
    MissingRecommended(PropertyKey),
    /// A conditional property has no explicit applicability decision.
    MissingConditionalDecision(PropertyKey),
    /// An applicable conditional property has no value.
    MissingApplicableValue(PropertyKey),
    /// A non-applicable conditional property carries a value.
    ValueForNonApplicableProperty(PropertyKey),
    /// A nonconditional property carries a conditional decision.
    UnexpectedConditionalDecision(PropertyKey),
    /// Authored multiplicity differs from the resolved typeset.
    Multiplicity {
        /// Property with the mismatch.
        key: PropertyKey,
        /// Required multiplicity.
        expected: PropertyMultiplicity,
        /// Supplied multiplicity.
        actual: PropertyMultiplicity,
    },
}

/// All ordered issues derived from validating property responses.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PropertyValidationReport {
    issues: Vec<PropertyValidationIssue>,
}

impl PropertyValidationReport {
    /// Returns issues in deterministic validation order.
    #[must_use]
    pub fn issues(&self) -> &[PropertyValidationIssue] {
        &self.issues
    }

    /// Returns whether at least one blocking issue exists.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.issues
            .iter()
            .any(|issue| issue.severity == ValidationSeverity::Error)
    }
}

/// Validates authored durable-property responses from one resolved change type.
#[must_use]
pub fn validate_property_responses(
    definition: &ResolvedChangeType,
    responses: &[PropertyResponse],
) -> PropertyValidationReport {
    let mut issues = Vec::new();
    let mut seen = BTreeSet::new();
    for response in responses {
        if !seen.insert(response.key()) {
            error(
                &mut issues,
                PropertyValidationIssueKind::DuplicateProperty(response.key().clone()),
            );
            continue;
        }
        let Some(property) = definition
            .properties()
            .iter()
            .find(|property| property.key() == response.key())
        else {
            error(
                &mut issues,
                PropertyValidationIssueKind::UnknownProperty(response.key().clone()),
            );
            continue;
        };
        if let Some(values) = response.values()
            && values.multiplicity() != property.multiplicity()
        {
            error(
                &mut issues,
                PropertyValidationIssueKind::Multiplicity {
                    key: response.key().clone(),
                    expected: property.multiplicity(),
                    actual: values.multiplicity(),
                },
            );
        }
        if !matches!(property.requirement(), PropertyRequirement::Conditional(_))
            && response.applicability().is_some()
        {
            error(
                &mut issues,
                PropertyValidationIssueKind::UnexpectedConditionalDecision(response.key().clone()),
            );
        }
    }

    for property in definition.properties() {
        let response = responses
            .iter()
            .find(|response| response.key() == property.key());
        let has_value = response.is_some_and(|response| response.values().is_some());
        match property.requirement() {
            PropertyRequirement::Required if !has_value => error(
                &mut issues,
                PropertyValidationIssueKind::MissingRequired(property.key().clone()),
            ),
            PropertyRequirement::Recommended if !has_value => {
                issues.push(PropertyValidationIssue {
                    severity: ValidationSeverity::Warning,
                    kind: PropertyValidationIssueKind::MissingRecommended(property.key().clone()),
                });
            }
            PropertyRequirement::Conditional(_) => match response.and_then(|value| {
                value
                    .applicability()
                    .map(|applicability| (applicability, value.values().is_some()))
            }) {
                None => error(
                    &mut issues,
                    PropertyValidationIssueKind::MissingConditionalDecision(property.key().clone()),
                ),
                Some((ConditionalApplicability::Applies, false)) => error(
                    &mut issues,
                    PropertyValidationIssueKind::MissingApplicableValue(property.key().clone()),
                ),
                Some((ConditionalApplicability::DoesNotApply, true)) => error(
                    &mut issues,
                    PropertyValidationIssueKind::ValueForNonApplicableProperty(
                        property.key().clone(),
                    ),
                ),
                Some(
                    (ConditionalApplicability::Applies, true)
                    | (ConditionalApplicability::DoesNotApply, false),
                ) => {}
            },
            PropertyRequirement::Required
            | PropertyRequirement::Recommended
            | PropertyRequirement::Optional => {}
        }
    }

    PropertyValidationReport { issues }
}

fn error(issues: &mut Vec<PropertyValidationIssue>, kind: PropertyValidationIssueKind) {
    issues.push(PropertyValidationIssue {
        severity: ValidationSeverity::Error,
        kind,
    });
}
