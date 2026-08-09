use std::error::Error;
use std::fmt::Write as _;

use gitserious_core::{
    AuthoredProperty, CommitDocumentError, CommitDraft, CommitDraftError, CommitScope,
    CommitScopeError, CommitSubject, CommitSubjectError, CommitTypeDefinition, CommitTypeId,
    CommitValidationError, PropertyDefinition, PropertyKey, PropertyMultiplicity,
    PropertyRequirement, PropertyValue, PropertyValues, SchemaVersion,
    annotate_commit_editor_document, built_in_commit_types, commit_editor_document_is_empty,
    parse_commit_editor_document, render_commit_editor_document, render_commit_message,
    validate_commit_draft,
};

fn definition(
    id: &str,
    multiplicity: PropertyMultiplicity,
) -> Result<CommitTypeDefinition, Box<dyn Error>> {
    Ok(CommitTypeDefinition::new(
        SchemaVersion::V1,
        CommitTypeId::new(id)?,
        "A custom commit type.",
        vec![PropertyDefinition::new(
            PropertyKey::new("claim")?,
            "The durable claim.",
            PropertyRequirement::Required,
            multiplicity,
        )?],
    )?)
}

fn complete_document(definition: &CommitTypeDefinition) -> String {
    let mut document = format!("{}(core): exercise the schema\n", definition.id());
    for property in definition.properties() {
        let _ = write!(
            document,
            "\n{}:\n  value for {}\n",
            property.key(),
            property.key()
        );
    }
    document
}

#[test]
fn scopes_and_subjects_preserve_valid_text_and_reject_ambiguous_headers()
-> Result<(), Box<dyn Error>> {
    let scope = CommitScope::new("workspace tools")?;
    let subject = CommitSubject::new("support Unicode 🦀")?;
    assert_eq!(scope.as_str(), "workspace tools");
    assert_eq!(scope.to_string(), "workspace tools");
    assert_eq!(subject.as_str(), "support Unicode 🦀");
    assert_eq!(subject.to_string(), "support Unicode 🦀");

    for (value, error) in [
        ("", CommitScopeError::Blank),
        (" ", CommitScopeError::Blank),
        (" core", CommitScopeError::SurroundingWhitespace),
        ("core ", CommitScopeError::SurroundingWhitespace),
        ("core\ncli", CommitScopeError::LineBreak),
        ("core(cli", CommitScopeError::Delimiter('(')),
        ("core:cli", CommitScopeError::Delimiter(':')),
    ] {
        assert_eq!(CommitScope::new(value), Err(error));
        assert!(!error.to_string().is_empty());
    }
    for (value, error) in [
        ("", CommitSubjectError::Blank),
        (" ", CommitSubjectError::Blank),
        (" subject", CommitSubjectError::SurroundingWhitespace),
        ("subject ", CommitSubjectError::SurroundingWhitespace),
        ("one\ntwo", CommitSubjectError::LineBreak),
        ("<subject>", CommitSubjectError::Placeholder),
    ] {
        assert_eq!(CommitSubject::new(value), Err(error));
        assert!(!error.to_string().is_empty());
    }
    Ok(())
}

#[test]
fn drafts_retain_keyed_values_and_reject_duplicate_properties() -> Result<(), Box<dyn Error>> {
    let key = PropertyKey::new("intent")?;
    let property = AuthoredProperty::new(
        key.clone(),
        PropertyValues::single(PropertyValue::new("explain the intent")?),
    );
    let draft = CommitDraft::new(
        CommitTypeId::new("feat")?,
        Some(CommitScope::new("core")?),
        CommitSubject::new("model commits")?,
        vec![property.clone()],
    )?;
    assert_eq!(draft.commit_type().as_str(), "feat");
    assert_eq!(draft.scope().map(CommitScope::as_str), Some("core"));
    assert_eq!(draft.subject().as_str(), "model commits");
    assert_eq!(draft.properties(), std::slice::from_ref(&property));

    assert_eq!(
        CommitDraft::new(
            CommitTypeId::new("feat")?,
            None,
            CommitSubject::new("duplicate")?,
            vec![property.clone(), property],
        ),
        Err(CommitDraftError::DuplicateProperty(key))
    );
    Ok(())
}

#[test]
fn every_built_in_schema_renders_parses_and_canonicalizes() -> Result<(), Box<dyn Error>> {
    for definition in built_in_commit_types() {
        let template = render_commit_editor_document(definition);
        assert!(template.starts_with(&format!(
            "{}(<optional-scope>): <subject>\n",
            definition.id()
        )));
        for property in definition.properties() {
            assert!(template.contains(&format!("\n{}:\n  <value>\n", property.key())));
            assert!(template.contains(property.description()));
        }

        let draft = parse_commit_editor_document(definition, &complete_document(definition))?;
        assert_eq!(draft.commit_type(), definition.id());
        assert_eq!(draft.properties().len(), definition.properties().len());
        let message = render_commit_message(definition, &draft)?;
        assert!(
            message
                .as_str()
                .starts_with(&format!("{}(core): exercise the schema\n", definition.id()))
        );
        for property in definition.properties() {
            assert!(message.as_str().contains(&format!(
                "\n{}:\n  value for {}\n",
                property.key(),
                property.key()
            )));
        }
    }
    Ok(())
}

#[test]
fn parsing_accepts_optional_scope_comments_multiline_values_and_schema_reordering()
-> Result<(), Box<dyn Error>> {
    let feat = &built_in_commit_types()[0];
    let document = "# instructions\nfeat(<optional-scope>): describe behavior\n\nbehavior:\n  first line\n  # durable hash line\n  third line\n\nintent:\n  explain why\n";
    let draft = parse_commit_editor_document(feat, document)?;
    assert!(draft.scope().is_none());
    let message = render_commit_message(feat, &draft)?;
    assert_eq!(
        message.as_str(),
        "feat: describe behavior\n\nintent:\n  explain why\n\nbehavior:\n  first line\n  # durable hash line\n  third line\n"
    );
    Ok(())
}

#[test]
fn only_required_properties_block_canonical_rendering() -> Result<(), Box<dyn Error>> {
    let feat = &built_in_commit_types()[0];
    let minimal = "feat: add capability\n\nintent:\n  establish it\n\nbehavior:\n  expose it\n";
    let draft = parse_commit_editor_document(feat, minimal)?;
    assert_eq!(draft.properties().len(), 2);
    assert!(render_commit_message(feat, &draft).is_ok());

    let missing = parse_commit_editor_document(
        feat,
        "feat: add capability\n\nconstraints:\n  dependency-free\n",
    )
    .err()
    .ok_or("missing required properties unexpectedly parsed")?;
    assert!(
        missing
            .as_slice()
            .contains(&CommitDocumentError::Validation(
                CommitValidationError::MissingRequired(PropertyKey::new("intent")?)
            ))
    );
    assert!(
        missing
            .as_slice()
            .contains(&CommitDocumentError::Validation(
                CommitValidationError::MissingRequired(PropertyKey::new("behavior")?)
            ))
    );
    Ok(())
}

#[test]
fn repeated_blocks_follow_declared_multiplicity() -> Result<(), Box<dyn Error>> {
    let repeatable = definition("custom", PropertyMultiplicity::Multiple)?;
    let document =
        "custom: collect claims\n\nclaim:\n  first\n\nclaim:\n  second\n  continuation\n";
    let draft = parse_commit_editor_document(&repeatable, document)?;
    assert_eq!(draft.properties()[0].values().len(), 2);
    assert_eq!(
        render_commit_message(&repeatable, &draft)?.as_str(),
        "custom: collect claims\n\nclaim:\n  first\n\nclaim:\n  second\n  continuation\n"
    );

    let single = definition("custom", PropertyMultiplicity::Single)?;
    let errors = parse_commit_editor_document(&single, document)
        .err()
        .ok_or("repeated single property unexpectedly parsed")?;
    assert!(errors.as_slice().contains(&CommitDocumentError::Validation(
        CommitValidationError::Multiplicity {
            key: PropertyKey::new("claim")?,
            expected: PropertyMultiplicity::Single,
            actual: PropertyMultiplicity::Multiple,
        }
    )));
    Ok(())
}

#[test]
fn parser_reports_header_property_and_layout_failures_together() -> Result<(), Box<dyn Error>> {
    let feat = &built_in_commit_types()[0];
    let errors = parse_commit_editor_document(
        feat,
        "fix(core): <subject>\n  orphan\nnot-a-block\nbad_key:\nunknown:\n  value\n",
    )
    .err()
    .ok_or("invalid document unexpectedly parsed")?;
    assert!(errors.as_slice().iter().any(|error| matches!(
        error,
        CommitDocumentError::InvalidSubject(CommitSubjectError::Placeholder)
    )));
    assert!(
        errors
            .as_slice()
            .iter()
            .any(|error| matches!(error, CommitDocumentError::OrphanContinuation(_)))
    );
    assert!(
        errors
            .as_slice()
            .iter()
            .any(|error| matches!(error, CommitDocumentError::UnexpectedLine(_)))
    );
    assert!(
        errors
            .as_slice()
            .iter()
            .any(|error| matches!(error, CommitDocumentError::InvalidPropertyKey { .. }))
    );
    assert!(errors.as_slice().contains(&CommitDocumentError::Validation(
        CommitValidationError::UnknownProperty(PropertyKey::new("unknown")?)
    )));
    assert!(!errors.to_string().is_empty());
    Ok(())
}

#[test]
fn parser_rejects_missing_malformed_and_mismatched_headers() -> Result<(), Box<dyn Error>> {
    let feat = &built_in_commit_types()[0];
    let missing = parse_commit_editor_document(feat, "# only comments\n\n")
        .err()
        .ok_or("missing header unexpectedly parsed")?;
    assert_eq!(missing.as_slice(), [CommitDocumentError::MissingHeader]);

    for header in [
        "feat subject",
        "feat(scope: subject",
        "Feat: subject",
        "feat!: subject",
    ] {
        assert!(
            parse_commit_editor_document(feat, header).is_err(),
            "{header}"
        );
    }

    let mismatch = parse_commit_editor_document(
        feat,
        "fix: correct behavior\n\nintent:\n  why\n\nbehavior:\n  what\n",
    )
    .err()
    .ok_or("mismatched type unexpectedly parsed")?;
    assert!(
        mismatch
            .as_slice()
            .contains(&CommitDocumentError::Validation(
                CommitValidationError::TypeMismatch {
                    expected: CommitTypeId::new("feat")?,
                    actual: CommitTypeId::new("fix")?,
                }
            ))
    );
    Ok(())
}

#[test]
fn placeholders_blank_blocks_and_unknown_keys_are_not_authored_values() -> Result<(), Box<dyn Error>>
{
    let feat = &built_in_commit_types()[0];
    let document = render_commit_editor_document(feat).replace("<subject>", "add behavior");
    let errors = parse_commit_editor_document(feat, &document)
        .err()
        .ok_or("placeholder-only properties unexpectedly parsed")?;
    assert!(errors.as_slice().contains(&CommitDocumentError::Validation(
        CommitValidationError::MissingRequired(PropertyKey::new("intent")?)
    )));

    let unknown =
        "feat: add behavior\n\nintent:\n  why\n\nbehavior:\n  what\n\nunknown:\n  <value>\n";
    let errors = parse_commit_editor_document(feat, unknown)
        .err()
        .ok_or("unknown blank key unexpectedly parsed")?;
    assert!(errors.as_slice().contains(&CommitDocumentError::Validation(
        CommitValidationError::UnknownProperty(PropertyKey::new("unknown")?)
    )));
    Ok(())
}

#[test]
fn direct_validation_rejects_type_unknown_property_missing_required_and_multiplicity()
-> Result<(), Box<dyn Error>> {
    let feat = &built_in_commit_types()[0];
    let draft = CommitDraft::new(
        CommitTypeId::new("fix")?,
        None,
        CommitSubject::new("wrong draft")?,
        vec![AuthoredProperty::new(
            PropertyKey::new("unknown")?,
            PropertyValues::multiple([PropertyValue::new("one")?, PropertyValue::new("two")?])?,
        )],
    )?;
    let errors = validate_commit_draft(feat, &draft)
        .err()
        .ok_or("invalid draft unexpectedly validated")?;
    assert_eq!(errors.as_slice().len(), 4);
    assert!(render_commit_message(feat, &draft).is_err());
    Ok(())
}

#[test]
fn empty_detection_and_error_annotations_preserve_authored_document() -> Result<(), Box<dyn Error>>
{
    assert!(commit_editor_document_is_empty("\n# comment\n  \n"));
    assert!(!commit_editor_document_is_empty("feat: subject\n"));

    let feat = &built_in_commit_types()[0];
    let document = "feat: subject\n";
    let errors = parse_commit_editor_document(feat, document)
        .err()
        .ok_or("missing properties unexpectedly parsed")?;
    let annotated = annotate_commit_editor_document(document, &errors);
    assert!(annotated.starts_with("# gitserious could not use this draft:\n# - "));
    assert!(annotated.ends_with(document));
    assert!(annotated.contains("complete required property"));
    Ok(())
}
