use std::error::Error;

use gitserious_core::{
    CommitDraft, CommitDraftError, CommitSubject, CommitTypeDefinition, CommitTypeId,
    CommitValidationError, ConditionId, ConditionalApplicability, PropertyCondition,
    PropertyDefinition, PropertyKey, PropertyMultiplicity, PropertyRequirement, PropertyResponse,
    PropertyValidationIssueKind, PropertyValue, PropertyValues, ResolvedTaxonomy, SchemaVersion,
    ValidationSeverity, built_in_configuration, render_commit_message, validate_commit_draft,
    validate_commit_draft_report, validate_property_responses,
};

type TestResult = Result<(), Box<dyn Error>>;

fn response(
    key: &str,
    text: Option<&str>,
    applicability: Option<ConditionalApplicability>,
) -> Result<PropertyResponse, Box<dyn Error>> {
    Ok(PropertyResponse::new(
        PropertyKey::new(key)?,
        text.map(PropertyValue::new)
            .transpose()?
            .map(PropertyValues::single),
        applicability,
    ))
}

fn definition() -> Result<CommitTypeDefinition, Box<dyn Error>> {
    let condition = PropertyCondition::new(ConditionId::new("bounded")?, "A limit applies.")?;
    let properties = [
        (
            "intent",
            PropertyRequirement::Required,
            PropertyMultiplicity::Single,
        ),
        (
            "evidence",
            PropertyRequirement::Recommended,
            PropertyMultiplicity::Single,
        ),
        (
            "notes",
            PropertyRequirement::Optional,
            PropertyMultiplicity::Single,
        ),
        (
            "constraints",
            PropertyRequirement::Conditional(condition),
            PropertyMultiplicity::Single,
        ),
        (
            "references",
            PropertyRequirement::Optional,
            PropertyMultiplicity::Multiple,
        ),
    ]
    .into_iter()
    .map(|(key, requirement, multiplicity)| {
        Ok(PropertyDefinition::new(
            PropertyKey::new(key)?,
            format!("Meaning of {key}."),
            requirement,
            multiplicity,
        )?)
    })
    .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    Ok(CommitTypeDefinition::new(
        SchemaVersion::V1,
        CommitTypeId::new("change")?,
        "A change.",
        properties,
    )?)
}

fn valid_responses() -> Result<Vec<PropertyResponse>, Box<dyn Error>> {
    Ok(vec![
        response("intent", Some("preserve context"), None)?,
        response(
            "constraints",
            None,
            Some(ConditionalApplicability::DoesNotApply),
        )?,
    ])
}

fn draft(responses: Vec<PropertyResponse>) -> Result<CommitDraft, Box<dyn Error>> {
    Ok(CommitDraft::from_responses(
        CommitTypeId::new("change")?,
        None,
        CommitSubject::new("retain decisions")?,
        responses,
    )?)
}

#[test]
fn explicit_decisions_survive_cloning_and_nonblocking_warnings() -> TestResult {
    let responses = valid_responses()?;
    let draft =
        draft(responses.clone())?.with_breaking_change(PropertyValue::new("Use the new API.")?);
    assert_eq!(draft.clone().responses(), Some(responses.as_slice()));
    assert_eq!(draft.properties().len(), 1);
    let report = validate_commit_draft_report(&definition()?, &draft);
    assert!(!report.has_errors());
    assert_eq!(report.warnings().len(), 1);
    assert_eq!(report.warnings()[0].severity(), ValidationSeverity::Warning);
    assert_eq!(
        report.warnings()[0].kind(),
        &PropertyValidationIssueKind::MissingRecommended(PropertyKey::new("evidence")?)
    );
    assert_eq!(
        render_commit_message(&definition()?, &draft)?.as_str(),
        "change!: retain decisions\n\nintent:\npreserve context\n\nBREAKING CHANGE: Use the new API.\n"
    );
    Ok(())
}

#[test]
fn response_drafts_block_each_invalid_applicability_state() -> TestResult {
    let key = PropertyKey::new("constraints")?;
    let cases = [
        (
            None,
            None,
            PropertyValidationIssueKind::MissingConditionalDecision(key.clone()),
        ),
        (
            Some("limit"),
            None,
            PropertyValidationIssueKind::MissingConditionalDecision(key.clone()),
        ),
        (
            None,
            Some(ConditionalApplicability::Applies),
            PropertyValidationIssueKind::MissingApplicableValue(key.clone()),
        ),
        (
            Some("limit"),
            Some(ConditionalApplicability::DoesNotApply),
            PropertyValidationIssueKind::ValueForNonApplicableProperty(key),
        ),
    ];
    for (value, applicability, expected) in cases {
        let draft = draft(vec![
            response("intent", Some("reason"), None)?,
            response("constraints", value, applicability)?,
        ])?;
        let report = validate_commit_draft_report(&definition()?, &draft);
        assert_eq!(
            report.errors(),
            &[CommitValidationError::PropertyResponse(expected)]
        );
        assert_eq!(
            validate_commit_draft(&definition()?, &draft)
                .err()
                .as_ref()
                .map(gitserious_core::CommitValidationErrors::as_slice),
            Some(report.errors())
        );
        assert!(render_commit_message(&definition()?, &draft).is_err());
    }
    // Omitting a conditional response entirely is also an unanswered decision.
    let omitted = draft(vec![response("intent", Some("reason"), None)?])?;
    assert!(validate_commit_draft_report(&definition()?, &omitted).has_errors());
    let applicable = draft(vec![
        response("intent", Some("reason"), None)?,
        response(
            "constraints",
            Some("limit"),
            Some(ConditionalApplicability::Applies),
        )?,
    ])?;
    assert!(
        render_commit_message(&definition()?, &applicable)?
            .as_str()
            .ends_with("constraints:\nlimit\n")
    );
    Ok(())
}

#[test]
fn explicit_empty_responses_and_legacy_drafts_keep_distinct_contracts() -> TestResult {
    let explicit = draft(vec![])?;
    assert_eq!(explicit.responses(), Some([].as_slice()));
    let report = validate_commit_draft_report(&definition()?, &explicit);
    assert_eq!(report.errors().len(), 2);
    assert_eq!(
        report.errors()[0],
        CommitValidationError::MissingRequired(PropertyKey::new("intent")?)
    );
    let explicit = draft(valid_responses()?)?;
    let legacy = CommitDraft::new(
        explicit.commit_type().clone(),
        None,
        explicit.subject().clone(),
        explicit.properties().to_vec(),
    )?;
    assert!(legacy.responses().is_none());
    assert!(validate_commit_draft(&definition()?, &legacy).is_ok());
    assert_eq!(
        render_commit_message(&definition()?, &legacy)?,
        render_commit_message(&definition()?, &explicit)?
    );
    Ok(())
}

#[test]
fn duplicate_valueless_responses_cannot_disappear_during_draft_construction() -> TestResult {
    let first = response("notes", None, None)?;
    let duplicate = CommitDraft::from_responses(
        CommitTypeId::new("change")?,
        None,
        CommitSubject::new("reject duplicates")?,
        vec![first.clone(), first],
    );
    assert_eq!(
        duplicate,
        Err(CommitDraftError::DuplicateProperty(PropertyKey::new(
            "notes"
        )?))
    );
    Ok(())
}

#[test]
fn unknown_valueless_responses_and_unexpected_decisions_are_rejected() -> TestResult {
    let mut responses = valid_responses()?;
    responses.push(response("unknown", None, None)?);
    responses.push(response(
        "notes",
        None,
        Some(ConditionalApplicability::DoesNotApply),
    )?);
    let draft = draft(responses)?;
    let report = validate_commit_draft_report(&definition()?, &draft);
    assert_eq!(
        report.errors(),
        &[
            CommitValidationError::UnknownProperty(PropertyKey::new("unknown")?),
            CommitValidationError::PropertyResponse(
                PropertyValidationIssueKind::UnexpectedConditionalDecision(PropertyKey::new(
                    "notes"
                )?)
            ),
        ]
    );
    assert!(render_commit_message(&definition()?, &draft).is_err());
    Ok(())
}

#[test]
fn repeatable_values_preserve_authored_order_and_enforce_multiplicity() -> TestResult {
    let mut responses = valid_responses()?;
    responses.insert(
        0,
        PropertyResponse::new(
            PropertyKey::new("references")?,
            Some(PropertyValues::multiple([
                PropertyValue::new("second\ncontinued")?,
                PropertyValue::new("first")?,
            ])?),
            None,
        ),
    );
    let draft = draft(responses)?;
    assert_eq!(
        render_commit_message(&definition()?, &draft)?.as_str(),
        "change: retain decisions\n\nintent:\npreserve context\n\nreferences:\nsecond\ncontinued\n\nreferences:\nfirst\n"
    );
    for (key, values, expected, actual) in [
        (
            "references",
            PropertyValues::single(PropertyValue::new("one")?),
            PropertyMultiplicity::Multiple,
            PropertyMultiplicity::Single,
        ),
        (
            "notes",
            PropertyValues::multiple([PropertyValue::new("one")?])?,
            PropertyMultiplicity::Single,
            PropertyMultiplicity::Multiple,
        ),
    ] {
        let mut responses = valid_responses()?;
        responses.push(PropertyResponse::new(
            PropertyKey::new(key)?,
            Some(values),
            None,
        ));
        let invalid = CommitDraft::from_responses(
            draft.commit_type().clone(),
            None,
            draft.subject().clone(),
            responses,
        )?;
        assert_eq!(
            validate_commit_draft_report(&definition()?, &invalid).errors(),
            &[CommitValidationError::Multiplicity {
                key: PropertyKey::new(key)?,
                expected,
                actual
            },]
        );
    }
    Ok(())
}

#[test]
fn draft_validation_preserves_shared_response_diagnostics_and_header_checks() -> TestResult {
    let built_in = built_in_configuration();
    let resolved =
        ResolvedTaxonomy::resolve(built_in.template(), built_in.taxonomy(), built_in.typeset())?;
    let selected = &resolved.change_types()[0];
    let responses = vec![
        response("intent", Some("reason"), None)?,
        response(
            "constraints",
            Some("limit"),
            Some(ConditionalApplicability::DoesNotApply),
        )?,
    ];
    let shared = validate_property_responses(selected, &responses);
    let draft = CommitDraft::from_responses(
        CommitTypeId::new("wrong")?,
        None,
        CommitSubject::new("check schema")?,
        responses,
    )?;
    let report = validate_commit_draft_report(&selected.commit_type_definition(), &draft);
    assert!(matches!(
        report.errors()[0],
        CommitValidationError::TypeMismatch { .. }
    ));
    let shared_errors: Vec<_> = shared
        .issues()
        .iter()
        .filter(|issue| issue.severity() == ValidationSeverity::Error)
        .map(|issue| issue.kind().to_string())
        .collect();
    let draft_errors: Vec<_> = report.errors()[1..]
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(shared_errors, draft_errors);
    assert!(render_commit_message(&selected.commit_type_definition(), &draft).is_err());
    Ok(())
}
