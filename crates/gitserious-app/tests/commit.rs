use std::error::Error;

use gitserious_app::{CommitAuthoringContext, CommitTaxonomy, fingerprint_taxonomy};
use gitserious_core::{
    CommitDraft, CommitMessage, CommitSubject, CommitTypeId, ConditionalApplicability, Fingerprint,
    PropertyKey, PropertyResponse, TaxonomyId, built_in_taxonomies,
};

#[test]
fn authoring_context_rejects_ambiguous_or_missing_initial_selections() -> Result<(), Box<dyn Error>>
{
    let conventional = built_in_taxonomies()[0].clone();
    let id = conventional.id().clone();

    assert!(CommitAuthoringContext::new(vec![], &id, None).is_err());
    assert!(
        CommitAuthoringContext::new(vec![conventional.clone(), conventional.clone()], &id, None)
            .is_err()
    );
    assert!(
        CommitAuthoringContext::new(
            vec![conventional.clone()],
            &TaxonomyId::new("absent")?,
            None
        )
        .is_err()
    );
    assert!(
        CommitAuthoringContext::new(vec![conventional], &id, Some(&CommitTypeId::new("absent")?))
            .is_err()
    );
    Ok(())
}

#[test]
fn taxonomy_render_binds_the_application_fingerprint_into_provenance() -> Result<(), Box<dyn Error>>
{
    let conventional = built_in_taxonomies()[0].clone();
    let fingerprint = fingerprint_taxonomy(&conventional);
    let draft = CommitDraft::from_responses(
        CommitTypeId::new("docs")?,
        None,
        CommitSubject::new("correct a typo")?,
        vec![PropertyResponse::new(
            PropertyKey::new("reason")?,
            None,
            Some(ConditionalApplicability::DoesNotApply),
        )],
    )?;

    let message: CommitMessage = CommitTaxonomy::new(conventional)
        .validate(&draft)
        .into_message()?;

    assert_eq!(
        message.as_str(),
        format!(
            "docs: correct a typo\n\nGitserious-Taxonomy: conventional-v1\nGitserious-Schema: {fingerprint}\n"
        )
    );
    assert_eq!(fingerprint.to_string().parse::<Fingerprint>()?, fingerprint);
    Ok(())
}
