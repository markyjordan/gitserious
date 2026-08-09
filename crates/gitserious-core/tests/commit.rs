use std::error::Error;

use gitserious_core::{
    AuthoredProperty, CommitDraft, CommitDraftError, CommitScope, CommitScopeError, CommitSubject,
    CommitSubjectError, CommitTypeDefinition, CommitTypeId, CommitValidationError,
    PropertyDefinition, PropertyKey, PropertyMultiplicity, PropertyRequirement, PropertyValue,
    PropertyValues, SchemaVersion, built_in_commit_types, render_commit_message,
    validate_commit_draft,
};

fn authored(key: &str, values: PropertyValues) -> Result<AuthoredProperty, Box<dyn Error>> {
    Ok(AuthoredProperty::new(PropertyKey::new(key)?, values))
}

fn value(text: &str) -> Result<PropertyValue, Box<dyn Error>> {
    Ok(PropertyValue::new(text)?)
}

fn feat_draft(properties: Vec<AuthoredProperty>) -> Result<CommitDraft, Box<dyn Error>> {
    Ok(CommitDraft::new(
        CommitTypeId::new("feat")?,
        Some(CommitScope::new("core")?),
        CommitSubject::new("render typed drafts")?,
        properties,
    )?)
}

#[test]
fn scope_and_subject_enforce_header_boundaries_and_preserve_unicode() -> Result<(), Box<dyn Error>>
{
    let scope = CommitScope::new("résumé-🦀")?;
    let subject = CommitSubject::new("preserve typed input 🦀")?;
    assert_eq!(scope.as_str(), "résumé-🦀");
    assert_eq!(subject.as_str(), "preserve typed input 🦀");
    assert_eq!(CommitScope::new(" "), Err(CommitScopeError::Blank));
    assert_eq!(
        CommitScope::new(" core"),
        Err(CommitScopeError::SurroundingWhitespace)
    );
    assert_eq!(
        CommitScope::new("co:re"),
        Err(CommitScopeError::Delimiter(':'))
    );
    assert_eq!(CommitSubject::new(""), Err(CommitSubjectError::Blank));
    assert_eq!(
        CommitSubject::new("line\nbreak"),
        Err(CommitSubjectError::LineBreak)
    );
    assert_eq!(
        CommitSubject::new("trailing "),
        Err(CommitSubjectError::SurroundingWhitespace)
    );
    assert_eq!(CommitSubject::new("<subject>")?.as_str(), "<subject>");
    Ok(())
}

#[test]
fn drafts_reject_duplicate_keys_and_preserve_authored_order() -> Result<(), Box<dyn Error>> {
    let first = authored("intent", PropertyValues::single(value("one")?))?;
    let second = authored("intent", PropertyValues::single(value("two")?))?;
    let duplicate = CommitDraft::new(
        CommitTypeId::new("feat")?,
        Some(CommitScope::new("core")?),
        CommitSubject::new("render typed drafts")?,
        vec![first.clone(), second],
    );
    assert_eq!(
        duplicate,
        Err(CommitDraftError::DuplicateProperty(PropertyKey::new(
            "intent",
        )?))
    );

    let behavior = authored("behavior", PropertyValues::single(value("behavior")?))?;
    let draft = feat_draft(vec![behavior.clone(), first])?;
    assert_eq!(draft.properties()[0], behavior);
    Ok(())
}

#[test]
fn canonical_render_uses_schema_order_and_preserves_multiline_values() -> Result<(), Box<dyn Error>>
{
    let draft = feat_draft(vec![
        authored(
            "behavior",
            PropertyValues::single(value("render one message")?),
        )?,
        authored(
            "intent",
            PropertyValues::single(value("centralize validation\nwithout syntax coupling 🦀")?),
        )?,
    ])?;

    let message = render_commit_message(&built_in_commit_types()[0], &draft)?;

    assert_eq!(
        message.as_str(),
        "feat(core): render typed drafts\n\nintent:\n  centralize validation\n  without syntax coupling 🦀\n\nbehavior:\n  render one message\n"
    );
    Ok(())
}

#[test]
fn validation_reports_type_unknown_required_and_multiplicity_failures() -> Result<(), Box<dyn Error>>
{
    let feat = &built_in_commit_types()[0];
    let unknown = authored("unknown", PropertyValues::single(value("value")?))?;
    let missing = feat_draft(vec![unknown])?;
    let Err(errors) = validate_commit_draft(feat, &missing) else {
        return Err("invalid draft accepted".into());
    };
    assert!(
        errors
            .as_slice()
            .contains(&CommitValidationError::UnknownProperty(PropertyKey::new(
                "unknown"
            )?))
    );
    for key in ["intent", "behavior"] {
        assert!(
            errors
                .as_slice()
                .contains(&CommitValidationError::MissingRequired(PropertyKey::new(
                    key
                )?))
        );
    }

    let fix_draft = CommitDraft::new(
        CommitTypeId::new("fix")?,
        None,
        CommitSubject::new("wrong type")?,
        Vec::new(),
    )?;
    assert!(matches!(
        validate_commit_draft(feat, &fix_draft),
        Err(errors) if matches!(
            errors.as_slice().first(),
            Some(CommitValidationError::TypeMismatch { .. })
        )
    ));

    let multiple = authored(
        "intent",
        PropertyValues::multiple([value("one")?, value("two")?])?,
    )?;
    let mismatch = feat_draft(vec![
        multiple,
        authored("behavior", PropertyValues::single(value("behavior")?))?,
    ])?;
    assert!(matches!(
        validate_commit_draft(feat, &mismatch),
        Err(errors) if errors.as_slice().contains(&CommitValidationError::Multiplicity {
            key: PropertyKey::new("intent")?,
            expected: PropertyMultiplicity::Single,
            actual: PropertyMultiplicity::Multiple,
        })
    ));
    Ok(())
}

#[test]
fn repeatable_values_render_as_ordered_canonical_property_blocks() -> Result<(), Box<dyn Error>> {
    let repeatable = CommitTypeDefinition::new(
        SchemaVersion::V1,
        CommitTypeId::new("custom")?,
        "Repeatable test type.",
        vec![PropertyDefinition::new(
            PropertyKey::new("evidence")?,
            "Independent evidence.",
            PropertyRequirement::Required,
            PropertyMultiplicity::Multiple,
        )?],
    )?;
    let draft = CommitDraft::new(
        CommitTypeId::new("custom")?,
        None,
        CommitSubject::new("retain evidence")?,
        vec![authored(
            "evidence",
            PropertyValues::multiple([value("first")?, value("second\nline")?])?,
        )?],
    )?;

    assert_eq!(
        render_commit_message(&repeatable, &draft)?.as_str(),
        "custom: retain evidence\n\nevidence:\n  first\n\nevidence:\n  second\n  line\n"
    );
    Ok(())
}

#[test]
fn recommended_optional_and_conditional_properties_may_be_absent() -> Result<(), Box<dyn Error>> {
    for definition in built_in_commit_types() {
        let properties = definition
            .properties()
            .iter()
            .filter(|property| property.requirement() == &PropertyRequirement::Required)
            .map(|property| {
                Ok(AuthoredProperty::new(
                    property.key().clone(),
                    PropertyValues::single(PropertyValue::new("complete")?),
                ))
            })
            .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
        let draft = CommitDraft::new(
            definition.id().clone(),
            None,
            CommitSubject::new("minimal valid draft")?,
            properties,
        )?;
        validate_commit_draft(definition, &draft)?;
    }
    Ok(())
}
