use std::error::Error;

use gitserious_core::{
    CommitDraft, CommitDraftError, CommitSubject, CommitTypeDefinition, CommitTypeId,
    CommitValidationError, ConditionId, ConditionalApplicability, PropertyCondition,
    PropertyDefinition, PropertyKey, PropertyRequirement, PropertyResponse,
    PropertyValidationIssueKind, PropertyValue, SchemaVersion, ValidationSeverity,
    built_in_commit_types, validate_commit_draft,
};

type TestResult = Result<(), Box<dyn Error>>;

fn response(
    key: &str,
    text: Option<&str>,
    applicability: Option<ConditionalApplicability>,
) -> Result<PropertyResponse, Box<dyn Error>> {
    Ok(PropertyResponse::new(
        PropertyKey::new(key)?,
        text.map(PropertyValue::new).transpose()?,
        applicability,
    ))
}

fn definition() -> Result<CommitTypeDefinition, Box<dyn Error>> {
    let condition = PropertyCondition::new(ConditionId::new("bounded")?, "A limit applies.")?;
    let properties = [
        ("intent", PropertyRequirement::Required),
        ("evidence", PropertyRequirement::Recommended),
        ("notes", PropertyRequirement::Optional),
        ("constraints", PropertyRequirement::Conditional(condition)),
        ("references", PropertyRequirement::Optional),
    ]
    .into_iter()
    .map(|(key, requirement)| {
        Ok(PropertyDefinition::new(
            PropertyKey::new(key)?,
            format!("Meaning of {key}."),
            requirement,
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
    let report = validate_commit_draft(&definition()?, &draft);
    assert!(report.errors().is_empty());
    assert_eq!(report.warnings().len(), 1);
    assert_eq!(report.warnings()[0].severity(), ValidationSeverity::Warning);
    assert_eq!(
        report.warnings()[0].kind(),
        &PropertyValidationIssueKind::MissingRecommended(PropertyKey::new("evidence")?)
    );
    assert_eq!(
        report.into_message()?.as_str(),
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
        let report = validate_commit_draft(&definition()?, &draft);
        assert_eq!(
            report.errors(),
            &[CommitValidationError::PropertyResponse(expected)]
        );
        assert!(report.into_message().is_err());
    }
    // Omitting a conditional response entirely is also an unanswered decision.
    let omitted = draft(vec![response("intent", Some("reason"), None)?])?;
    assert!(
        !validate_commit_draft(&definition()?, &omitted)
            .errors()
            .is_empty()
    );
    let applicable = draft(vec![
        response("intent", Some("reason"), None)?,
        response(
            "constraints",
            Some("limit"),
            Some(ConditionalApplicability::Applies),
        )?,
    ])?;
    assert!(
        validate_commit_draft(&definition()?, &applicable)
            .into_message()?
            .as_str()
            .ends_with("constraints:\nlimit\n")
    );
    Ok(())
}

#[test]
fn explicit_empty_responses_and_legacy_drafts_keep_distinct_contracts() -> TestResult {
    let explicit = draft(vec![])?;
    assert_eq!(explicit.responses(), Some([].as_slice()));
    let report = validate_commit_draft(&definition()?, &explicit);
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
    assert_eq!(
        validate_commit_draft(&definition()?, &legacy).into_message()?,
        validate_commit_draft(&definition()?, &explicit).into_message()?
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
    let report = validate_commit_draft(&definition()?, &draft);
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
    assert!(report.into_message().is_err());
    Ok(())
}

#[test]
fn draft_validation_reports_the_header_mismatch_before_response_diagnostics() -> TestResult {
    let feat = built_in_commit_types()
        .iter()
        .find(|definition| definition.id().as_str() == "feat")
        .ok_or("conventional has no feat")?;
    let responses = vec![
        response("intent", Some("reason"), None)?,
        response(
            "constraints",
            Some("limit"),
            Some(ConditionalApplicability::DoesNotApply),
        )?,
    ];
    let matching = CommitDraft::from_responses(
        feat.id().clone(),
        None,
        CommitSubject::new("check schema")?,
        responses.clone(),
    )?;
    let mismatched = CommitDraft::from_responses(
        CommitTypeId::new("wrong")?,
        None,
        CommitSubject::new("check schema")?,
        responses,
    )?;

    let expected: Vec<_> = validate_commit_draft(feat, &matching)
        .errors()
        .iter()
        .map(ToString::to_string)
        .collect();
    let report = validate_commit_draft(feat, &mismatched);

    assert!(!expected.is_empty());
    assert!(matches!(
        report.errors()[0],
        CommitValidationError::TypeMismatch { .. }
    ));
    let rest: Vec<_> = report.errors()[1..]
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(rest, expected);
    assert!(report.into_message().is_err());
    Ok(())
}
