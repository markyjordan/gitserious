use std::error::Error;

use gitserious_core::{
    AuthoredProperty, COMMIT_MESSAGE_WIDTH, CommitDraft, CommitDraftError, CommitScope,
    CommitScopeError, CommitSubject, CommitSubjectError, CommitTypeDefinition,
    CommitTypeDefinitionError, CommitTypeId, CommitValidationError, PropertyDefinition,
    PropertyKey, PropertyRequirement, PropertyValidationIssueKind, PropertyValue, SchemaVersion,
    TaxonomyError, TaxonomyId, built_in_commit_types, validate_commit_draft,
};
use unicode_width::UnicodeWidthStr;

fn authored(key: &str, value: PropertyValue) -> Result<AuthoredProperty, Box<dyn Error>> {
    Ok(AuthoredProperty::new(PropertyKey::new(key)?, value))
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
    let first = authored("intent", value("one")?)?;
    let second = authored("intent", value("two")?)?;
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

    let decision = authored("decision", value("decision")?)?;
    let draft = feat_draft(vec![decision.clone(), first])?;
    assert_eq!(draft.properties()[0], decision);
    Ok(())
}

#[test]
fn canonical_render_uses_schema_order_and_preserves_multiline_values() -> Result<(), Box<dyn Error>>
{
    let draft = feat_draft(vec![
        authored("decision", value("render one message")?)?,
        authored(
            "intent",
            value("centralize validation\n  preserve authored indentation 🦀")?,
        )?,
    ])?;

    let message = validate_commit_draft(&built_in_commit_types()[0], &draft).into_message()?;

    assert_eq!(
        message.as_str(),
        "feat(core): render typed drafts\n\nintent:\ncentralize validation\n  preserve authored indentation 🦀\n\ndecision:\nrender one message\n"
    );
    Ok(())
}

#[test]
fn canonical_prose_wraps_at_80_columns_without_mutating_authored_values()
-> Result<(), Box<dyn Error>> {
    assert_eq!(COMMIT_MESSAGE_WIDTH, 80);
    let intent = format!("{} durable\n  authored indentation", "x".repeat(75));
    let decision = "y".repeat(80);
    let draft = feat_draft(vec![
        authored("intent", value(&intent)?)?,
        authored("decision", value(&decision)?)?,
    ])?;

    let message = validate_commit_draft(&built_in_commit_types()[0], &draft).into_message()?;
    assert_eq!(
        message.as_str(),
        format!(
            "feat(core): render typed drafts\n\nintent:\n{}\ndurable\n  authored indentation\n\ndecision:\n{decision}\n",
            "x".repeat(75),
        )
    );
    assert_eq!(draft.properties()[0].value().as_str(), intent);
    assert!(
        message
            .as_str()
            .lines()
            .all(|line| { UnicodeWidthStr::width(line) <= usize::from(COMMIT_MESSAGE_WIDTH) })
    );
    Ok(())
}

#[test]
fn canonical_wrapping_splits_long_unicode_tokens_only_at_grapheme_boundaries()
-> Result<(), Box<dyn Error>> {
    let crabs = "🦀".repeat(41);
    let combined = "e\u{301}".repeat(81);
    let draft = feat_draft(vec![
        authored("intent", value(&crabs)?)?,
        authored("decision", value(&combined)?)?,
    ])?;

    let message = validate_commit_draft(&built_in_commit_types()[0], &draft).into_message()?;
    assert_eq!(
        message.as_str(),
        format!(
            "feat(core): render typed drafts\n\nintent:\n{}\n🦀\n\ndecision:\n{}\ne\u{301}\n",
            "🦀".repeat(40),
            "e\u{301}".repeat(80),
        )
    );
    assert!(
        message
            .as_str()
            .lines()
            .all(|line| { UnicodeWidthStr::width(line) <= usize::from(COMMIT_MESSAGE_WIDTH) })
    );
    Ok(())
}

#[test]
fn breaking_change_prefix_and_continuations_share_the_80_column_contract()
-> Result<(), Box<dyn Error>> {
    let breaking = std::iter::repeat_n("migration", 10)
        .collect::<Vec<_>>()
        .join(" ");
    let draft = feat_draft(vec![
        authored("intent", value("protect callers")?)?,
        authored("decision", value("require upgrade")?)?,
    ])?
    .with_breaking_change(value(&format!("{breaking}\n{}", "z".repeat(81)))?);

    let message = validate_commit_draft(&built_in_commit_types()[0], &draft).into_message()?;
    let footer = message
        .as_str()
        .split_once("BREAKING CHANGE: ")
        .map(|(_, footer)| footer)
        .ok_or("missing breaking-change footer")?;
    assert_eq!(
        footer.lines().take(2).collect::<Vec<_>>().join(" "),
        breaking
    );
    let eighty_zs = "z".repeat(80);
    assert_eq!(footer.lines().nth(2), Some(eighty_zs.as_str()));
    assert_eq!(footer.lines().nth(3), Some("z"));
    assert!(
        message
            .as_str()
            .lines()
            .all(|line| { UnicodeWidthStr::width(line) <= usize::from(COMMIT_MESSAGE_WIDTH) })
    );
    Ok(())
}

#[test]
fn canonical_render_hyphenates_scope_whitespace() -> Result<(), Box<dyn Error>> {
    let definition = &built_in_commit_types()[0];
    for (scope, expected) in [
        ("tui editor", "tui-editor"),
        ("TUI\t editor", "TUI-editor"),
        ("résumé  parser", "résumé-parser"),
        ("already-hyphenated", "already-hyphenated"),
    ] {
        let draft = CommitDraft::new(
            definition.id().clone(),
            Some(CommitScope::new(scope)?),
            CommitSubject::new("normalize scope")?,
            vec![
                authored("intent", value("canonical output")?)?,
                authored("decision", value("replace whitespace")?)?,
            ],
        )?;
        assert!(
            validate_commit_draft(definition, &draft)
                .into_message()?
                .as_str()
                .starts_with(&format!("feat({expected}): normalize scope\n"))
        );
    }
    Ok(())
}

#[test]
fn breaking_change_adds_header_bang_and_uppercase_multiline_footer() -> Result<(), Box<dyn Error>> {
    let definition = &built_in_commit_types()[0];
    let draft = feat_draft(vec![
        authored("intent", value("replace API")?)?,
        authored("decision", value("require migration")?)?,
    ])?
    .with_breaking_change(value("remove the old API\nuse the replacement")?);

    assert_eq!(
        validate_commit_draft(definition, &draft)
            .into_message()?
            .as_str(),
        "feat(core)!: render typed drafts\n\nintent:\nreplace API\n\ndecision:\nrequire migration\n\nBREAKING CHANGE: remove the old API\nuse the replacement\n"
    );
    assert_eq!(
        draft.breaking_change().map(PropertyValue::as_str),
        Some("remove the old API\nuse the replacement")
    );
    Ok(())
}

#[test]
fn breaking_change_without_scope_or_body_preserves_canonical_separation()
-> Result<(), Box<dyn Error>> {
    let definition = CommitTypeDefinition::new(
        SchemaVersion::V1,
        CommitTypeId::new("custom")?,
        "No body properties.",
        vec![PropertyDefinition::new(
            PropertyKey::new("notes")?,
            "Optional body notes.",
            PropertyRequirement::Optional,
        )?],
    )?;
    let draft = CommitDraft::new(
        CommitTypeId::new("custom")?,
        None,
        CommitSubject::new("change contract")?,
        Vec::new(),
    )?
    .with_breaking_change(value("new contract")?);

    assert_eq!(
        validate_commit_draft(&definition, &draft)
            .into_message()?
            .as_str(),
        "custom!: change contract\n\nBREAKING CHANGE: new contract\n"
    );
    Ok(())
}

#[test]
fn validation_reports_type_unknown_and_required_failures() -> Result<(), Box<dyn Error>> {
    let feat = &built_in_commit_types()[0];
    let unknown = authored("unknown", value("value")?)?;
    let missing = feat_draft(vec![unknown])?;
    let Err(errors) = validate_commit_draft(feat, &missing).into_message() else {
        return Err("invalid draft accepted".into());
    };
    assert!(
        errors
            .as_slice()
            .contains(&CommitValidationError::UnknownProperty(PropertyKey::new(
                "unknown"
            )?))
    );
    for key in ["intent", "decision"] {
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
        validate_commit_draft(feat, &fix_draft).into_message(),
        Err(errors) if matches!(
            errors.as_slice().first(),
            Some(CommitValidationError::TypeMismatch { .. })
        )
    ));
    Ok(())
}

#[test]
fn validation_messages_quote_names() -> Result<(), Box<dyn Error>> {
    let key = PropertyKey::new("intent")?;
    let feat = CommitTypeId::new("feat")?;
    let fix = CommitTypeId::new("fix")?;
    let cases = [
        (
            CommitValidationError::UnknownCommitType {
                taxonomy: TaxonomyId::new("conventional")?,
                actual: CommitTypeId::new("deploy")?,
            }
            .to_string(),
            "type \"deploy\" is not available in taxonomy \"conventional\"",
        ),
        (
            CommitValidationError::TypeMismatch {
                expected: feat.clone(),
                actual: fix,
            }
            .to_string(),
            "selected type \"feat\" cannot use header type \"fix\"",
        ),
        (
            CommitValidationError::UnknownProperty(key.clone()).to_string(),
            "property \"intent\" is not defined for the selected type",
        ),
        (
            CommitValidationError::MissingRequired(key.clone()).to_string(),
            "complete required property \"intent\"",
        ),
        (
            PropertyValidationIssueKind::UnknownProperty(key.clone()).to_string(),
            "property \"intent\" is not defined for the selected type",
        ),
        (
            PropertyValidationIssueKind::DuplicateProperty(key.clone()).to_string(),
            "property \"intent\" has more than one response",
        ),
        (
            PropertyValidationIssueKind::MissingRequired(key.clone()).to_string(),
            "complete required property \"intent\"",
        ),
        (
            PropertyValidationIssueKind::MissingRecommended(key.clone()).to_string(),
            "consider completing recommended property \"intent\"",
        ),
        (
            PropertyValidationIssueKind::MissingConditionalDecision(key.clone()).to_string(),
            "choose whether conditional property \"intent\" applies",
        ),
        (
            PropertyValidationIssueKind::MissingApplicableValue(key.clone()).to_string(),
            "complete applicable property \"intent\"",
        ),
        (
            PropertyValidationIssueKind::ValueForNonApplicableProperty(key.clone()).to_string(),
            "remove the value for non-applicable property \"intent\"",
        ),
        (
            PropertyValidationIssueKind::UnexpectedConditionalDecision(key.clone()).to_string(),
            "property \"intent\" is not conditional",
        ),
        (
            CommitDraftError::DuplicateProperty(key.clone()).to_string(),
            "commit draft repeats property \"intent\"",
        ),
        (
            CommitTypeDefinitionError::DuplicateProperty(key).to_string(),
            "commit type definition repeats property \"intent\"",
        ),
        (
            TaxonomyError::DuplicateCommitType(feat).to_string(),
            "taxonomy repeats commit type \"feat\"",
        ),
    ];
    for (actual, expected) in cases {
        assert_eq!(actual, expected);
    }
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
                    PropertyValue::new("complete")?,
                ))
            })
            .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
        let draft = CommitDraft::new(
            definition.id().clone(),
            None,
            CommitSubject::new("minimal valid draft")?,
            properties,
        )?;
        validate_commit_draft(definition, &draft).into_message()?;
    }
    Ok(())
}
