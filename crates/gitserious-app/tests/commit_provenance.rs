use std::error::Error;

use gitserious_app::{Fingerprint, built_in_effective_catalog, fingerprint_resolved_taxonomy};
use gitserious_core::{
    CommitDraft, CommitProvenance, CommitSubject, CommitTypeId, ConditionalApplicability,
    PropertyKey, PropertyResponse, TemplateId, render_commit_message_with_provenance,
};

#[test]
fn application_fingerprints_bind_to_core_provenance_without_reencoding()
-> Result<(), Box<dyn Error>> {
    let catalog = built_in_effective_catalog()?;
    let schema = catalog.resolve(&TemplateId::new("default")?)?;
    let fingerprint: Fingerprint = fingerprint_resolved_taxonomy(&schema);
    let core_fingerprint: gitserious_core::Fingerprint = fingerprint;
    let provenance = CommitProvenance::new(schema, core_fingerprint);
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
    let message = render_commit_message_with_provenance(&provenance, &draft)?;
    assert_eq!(
        provenance.fingerprint(),
        fingerprint_resolved_taxonomy(provenance.schema())
    );
    assert_eq!(
        message.as_str(),
        format!(
            "docs: correct a typo\n\nGitserious-Template: default@1\nGitserious-Taxonomy: conventional@1\nGitserious-Typeset: conventional/default@1\nGitserious-Schema: {fingerprint}\n"
        )
    );
    assert_eq!(
        fingerprint.to_string().parse::<Fingerprint>()?,
        core_fingerprint
    );
    Ok(())
}
