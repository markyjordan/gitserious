use std::error::Error;

use gitserious_core::{
    CommitMessageTemplateDefinition, CommitMessageTemplateDefinitionError, CommitTypeDefinition,
    CommitTypeId, PropertyDefinition, PropertyKey, PropertyMultiplicity, PropertyRequirement,
    SchemaVersion, TemplateId, TemplateVersion, TemplateVersionError, built_in_commit_types,
    default_commit_message_template,
};

fn commit_type(id: &str) -> Result<CommitTypeDefinition, Box<dyn Error>> {
    Ok(CommitTypeDefinition::new(
        SchemaVersion::V1,
        CommitTypeId::new(id)?,
        format!("Description for {id}."),
        vec![PropertyDefinition::new(
            PropertyKey::new("intent")?,
            "Why this change exists.",
            PropertyRequirement::Required,
            PropertyMultiplicity::Single,
        )?],
    )?)
}

#[test]
fn template_identifiers_follow_the_open_identifier_contract() -> Result<(), Box<dyn Error>> {
    let id = TemplateId::new("custom-template-2")?;

    assert_eq!(id.as_str(), "custom-template-2");
    assert_eq!(id.to_string(), "custom-template-2");
    assert!(TemplateId::new("Custom").is_err());
    assert!(TemplateId::new("custom_template").is_err());
    assert!(TemplateId::new("custom--template").is_err());

    Ok(())
}

#[test]
fn template_versions_are_positive_and_ordered() -> Result<(), Box<dyn Error>> {
    let version = TemplateVersion::new(2)?;

    assert_eq!(TemplateVersion::V1.get(), 1);
    assert_eq!(version.get(), 2);
    assert!(TemplateVersion::V1 < version);
    assert_eq!(TemplateVersion::new(0), Err(TemplateVersionError));
    assert_eq!(
        TemplateVersionError.to_string(),
        "template version must be greater than zero"
    );

    Ok(())
}

#[test]
fn template_definitions_preserve_identity_description_and_order() -> Result<(), Box<dyn Error>> {
    let template = CommitMessageTemplateDefinition::new(
        TemplateVersion::new(3)?,
        TemplateId::new("custom")?,
        "A custom policy.",
        vec![commit_type("first")?, commit_type("second")?],
    )?;

    assert_eq!(template.id().as_str(), "custom");
    assert_eq!(template.version().get(), 3);
    assert_eq!(template.description(), "A custom policy.");
    assert_eq!(
        template
            .commit_types()
            .iter()
            .map(|definition| definition.id().as_str())
            .collect::<Vec<_>>(),
        ["first", "second"]
    );

    Ok(())
}

#[test]
fn template_definitions_reject_blank_empty_and_duplicate_content() -> Result<(), Box<dyn Error>> {
    let id = TemplateId::new("custom")?;
    let first = commit_type("first")?;

    assert_eq!(
        CommitMessageTemplateDefinition::new(
            TemplateVersion::V1,
            id.clone(),
            " \n",
            vec![first.clone()],
        ),
        Err(CommitMessageTemplateDefinitionError::EmptyDescription)
    );
    assert_eq!(
        CommitMessageTemplateDefinition::new(
            TemplateVersion::V1,
            id.clone(),
            "A policy.",
            Vec::new(),
        ),
        Err(CommitMessageTemplateDefinitionError::EmptyCommitTypes)
    );
    assert_eq!(
        CommitMessageTemplateDefinition::new(
            TemplateVersion::V1,
            id,
            "A policy.",
            vec![first.clone(), first],
        ),
        Err(CommitMessageTemplateDefinitionError::DuplicateCommitType(
            CommitTypeId::new("first")?
        ))
    );

    Ok(())
}

#[test]
fn default_channel_resolves_the_exact_conventional_v1_catalog() {
    let template = default_commit_message_template();

    assert_eq!(template.id().as_str(), "conventional");
    assert_eq!(template.version(), TemplateVersion::V1);
    assert!(!template.description().trim().is_empty());
    assert_eq!(template.commit_types(), built_in_commit_types());
    assert_eq!(template.commit_types().len(), 11);
    assert_eq!(
        template
            .commit_types()
            .iter()
            .map(|definition| definition.id().as_str())
            .collect::<Vec<_>>(),
        [
            "feat", "fix", "refactor", "perf", "test", "docs", "chore", "build", "ci", "style",
            "revert",
        ]
    );
}
