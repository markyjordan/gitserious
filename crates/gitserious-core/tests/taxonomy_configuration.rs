use std::error::Error;

use gitserious_core::{
    CommitDraft, CommitSubject, CommitTypeDefinition, CommitTypeId, CommitValidationError,
    ConditionId, ConditionalApplicability, Description, DescriptionError, PropertyCondition,
    PropertyDefinition, PropertyKey, PropertyMultiplicity, PropertyRequirement, PropertyResponse,
    PropertyValidationIssueKind, PropertyValue, PropertyValues, SchemaVersion, Taxonomy,
    TaxonomyError, TaxonomyId, TaxonomyVersion, built_in_commit_types,
    validate_commit_draft_report,
};

fn commit_type(
    id: &str,
    properties: Vec<PropertyDefinition>,
) -> Result<CommitTypeDefinition, Box<dyn Error>> {
    Ok(CommitTypeDefinition::new(
        SchemaVersion::V1,
        CommitTypeId::new(id)?,
        format!("The {id} change."),
        properties,
    )?)
}

fn property(
    key: &str,
    requirement: PropertyRequirement,
    multiplicity: PropertyMultiplicity,
) -> Result<PropertyDefinition, Box<dyn Error>> {
    Ok(PropertyDefinition::new(
        PropertyKey::new(key)?,
        format!("Meaning of {key}."),
        requirement,
        multiplicity,
    )?)
}

fn draft(id: &str, responses: Vec<PropertyResponse>) -> Result<CommitDraft, Box<dyn Error>> {
    Ok(CommitDraft::from_responses(
        CommitTypeId::new(id)?,
        None,
        CommitSubject::new("validate responses")?,
        responses,
    )?)
}

#[test]
fn descriptions_preserve_text_and_reject_blank_values() -> Result<(), Box<dyn Error>> {
    let value = Description::new("  Durable meaning 🦀  ")?;
    assert_eq!(value.as_str(), "  Durable meaning 🦀  ");
    assert_eq!(value.to_string(), "  Durable meaning 🦀  ");
    assert_eq!(Description::new(" \n"), Err(DescriptionError));
    assert!(!DescriptionError.to_string().is_empty());
    Ok(())
}

#[test]
fn taxonomy_preserves_identity_version_description_and_order() -> Result<(), Box<dyn Error>> {
    let taxonomy = Taxonomy::new(
        TaxonomyId::new("custom")?,
        TaxonomyVersion::new(2)?,
        Description::new("A custom taxonomy.")?,
        None,
        vec![
            commit_type("beta", Vec::new())?,
            commit_type("alpha", Vec::new())?,
        ],
    )?;

    assert_eq!(taxonomy.id().as_str(), "custom");
    assert_eq!(taxonomy.version().get(), 2);
    assert_eq!(taxonomy.description().as_str(), "A custom taxonomy.");
    assert_eq!(
        taxonomy
            .commit_types()
            .iter()
            .map(|definition| definition.id().as_str())
            .collect::<Vec<_>>(),
        ["beta", "alpha"]
    );
    assert_eq!(taxonomy.clone(), taxonomy);
    Ok(())
}

#[test]
fn taxonomy_rejects_empty_and_duplicate_commit_types() -> Result<(), Box<dyn Error>> {
    let id = TaxonomyId::new("custom")?;
    let description = Description::new("Custom.")?;

    assert_eq!(
        Taxonomy::new(
            id.clone(),
            TaxonomyVersion::V1,
            description.clone(),
            None,
            Vec::new()
        ),
        Err(TaxonomyError::EmptyCommitTypes)
    );
    assert_eq!(
        Taxonomy::new(
            id,
            TaxonomyVersion::V1,
            description,
            None,
            vec![
                commit_type("alpha", Vec::new())?,
                commit_type("alpha", Vec::new())?
            ],
        ),
        Err(TaxonomyError::DuplicateCommitType(CommitTypeId::new(
            "alpha"
        )?))
    );
    Ok(())
}

#[test]
fn requirement_validation_reports_errors_and_recommendations_from_the_schema()
-> Result<(), Box<dyn Error>> {
    let condition = PropertyCondition::new(
        ConditionId::new("known-cost")?,
        "Required when a known cost exists.",
    )?;
    let definition = commit_type(
        "change",
        vec![
            property(
                "required",
                PropertyRequirement::Required,
                PropertyMultiplicity::Single,
            )?,
            property(
                "recommended",
                PropertyRequirement::Recommended,
                PropertyMultiplicity::Single,
            )?,
            property(
                "optional",
                PropertyRequirement::Optional,
                PropertyMultiplicity::Multiple,
            )?,
            property(
                "conditional",
                PropertyRequirement::Conditional(condition),
                PropertyMultiplicity::Single,
            )?,
        ],
    )?;

    let report = validate_commit_draft_report(&definition, &draft("change", Vec::new())?);
    assert!(report.has_errors());
    assert_eq!(
        report.errors(),
        [
            CommitValidationError::MissingRequired(PropertyKey::new("required")?),
            CommitValidationError::PropertyResponse(
                PropertyValidationIssueKind::MissingConditionalDecision(PropertyKey::new(
                    "conditional"
                )?)
            ),
        ]
    );
    assert_eq!(report.warnings().len(), 1);
    assert_eq!(
        report.warnings()[0].kind(),
        &PropertyValidationIssueKind::MissingRecommended(PropertyKey::new("recommended")?)
    );

    let answered = draft(
        "change",
        vec![
            PropertyResponse::new(
                PropertyKey::new("required")?,
                Some(PropertyValues::single(PropertyValue::new("value")?)),
                None,
            ),
            PropertyResponse::new(
                PropertyKey::new("conditional")?,
                None,
                Some(ConditionalApplicability::DoesNotApply),
            ),
        ],
    )?;
    let report = validate_commit_draft_report(&definition, &answered);
    assert!(!report.has_errors());
    assert_eq!(report.warnings().len(), 1);
    Ok(())
}

#[test]
fn conditional_and_multiplicity_contradictions_are_blocking() -> Result<(), Box<dyn Error>> {
    let feat = built_in_commit_types()
        .iter()
        .find(|definition| definition.id().as_str() == "feat")
        .ok_or("conventional has no feat")?;
    let responses = vec![
        PropertyResponse::new(
            PropertyKey::new("intent")?,
            Some(PropertyValues::multiple([PropertyValue::new("one")?])?),
            Some(ConditionalApplicability::Applies),
        ),
        PropertyResponse::new(
            PropertyKey::new("constraints")?,
            Some(PropertyValues::single(PropertyValue::new("constraint")?)),
            Some(ConditionalApplicability::DoesNotApply),
        ),
        PropertyResponse::new(PropertyKey::new("unknown")?, None, None),
    ];

    let report = validate_commit_draft_report(feat, &draft("feat", responses)?);

    assert!(report.has_errors());
    let errors = report.errors();
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, CommitValidationError::Multiplicity { .. }))
    );
    assert!(errors.iter().any(|error| matches!(
        error,
        CommitValidationError::PropertyResponse(
            PropertyValidationIssueKind::UnexpectedConditionalDecision(_)
        )
    )));
    assert!(errors.iter().any(|error| matches!(
        error,
        CommitValidationError::PropertyResponse(
            PropertyValidationIssueKind::ValueForNonApplicableProperty(_)
        )
    )));
    assert!(errors.iter().any(|error| matches!(
        error,
        CommitValidationError::UnknownProperty(_)
            | CommitValidationError::PropertyResponse(
                PropertyValidationIssueKind::UnknownProperty(_)
            )
    )));
    Ok(())
}
