use std::error::Error;

use gitserious_core::{
    ChangeTypeDefinition, ChangeTypeSchema, CommitDraft, CommitProvenance, CommitScope,
    CommitSubject, CommitTypeId, CommitValidationError, ConditionalApplicability, Description,
    Fingerprint, FingerprintError, PropertyDefinition, PropertyKey, PropertyMultiplicity,
    PropertyRequirement, PropertyResponse, PropertyValue, PropertyValues, ResolvedTaxonomy,
    TaxonomyDefinition, TaxonomyId, TaxonomyVersion, TemplateDefinition, TemplateId,
    TemplateVersion, TypesetDefinition, TypesetId, TypesetVersion, built_in_configuration,
    render_commit_message, render_commit_message_with_provenance,
};
use unicode_width::UnicodeWidthStr;

type TestResult = Result<(), Box<dyn Error>>;

fn schema(domain: &str, required: Option<&str>) -> Result<ResolvedTaxonomy, Box<dyn Error>> {
    let taxonomy_id = TaxonomyId::new(domain)?;
    let properties = required
        .map(|key| -> Result<_, Box<dyn Error>> {
            Ok(PropertyDefinition::new(
                PropertyKey::new(key)?,
                "Durable context.",
                PropertyRequirement::Required,
                PropertyMultiplicity::Single,
            )?)
        })
        .transpose()?
        .into_iter()
        .collect();
    let taxonomy = TaxonomyDefinition::new(
        taxonomy_id.clone(),
        TaxonomyVersion::new(2)?,
        Description::new("Domain changes.")?,
        vec![ChangeTypeDefinition::new(
            CommitTypeId::new("fix")?,
            Description::new("Correct a failure.")?,
        )],
    )?;
    let typeset = TypesetDefinition::new(
        taxonomy_id.clone(),
        TypesetId::new("default")?,
        TypesetVersion::new(3)?,
        Description::new("Domain context.")?,
        vec![ChangeTypeSchema::new(
            CommitTypeId::new("fix")?,
            properties,
        )?],
    )?;
    let template = TemplateDefinition::new(
        TemplateId::new(format!("{domain}-template"))?,
        TemplateVersion::new(4)?,
        Description::new("Domain template.")?,
        taxonomy_id,
        typeset.id().clone(),
    );
    Ok(ResolvedTaxonomy::resolve(&template, &taxonomy, &typeset)?)
}

fn draft(required: Option<(&str, &str)>) -> Result<CommitDraft, Box<dyn Error>> {
    let responses = required
        .map(|(key, text)| -> Result<_, Box<dyn Error>> {
            Ok(PropertyResponse::new(
                PropertyKey::new(key)?,
                Some(PropertyValues::single(PropertyValue::new(text)?)),
                None,
            ))
        })
        .transpose()?
        .into_iter()
        .collect();
    Ok(CommitDraft::from_responses(
        CommitTypeId::new("fix")?,
        Some(CommitScope::new("data source")?),
        CommitSubject::new("preserve the evidence")?,
        responses,
    )?)
}

#[test]
fn trailers_preserve_qualified_identities_versions_and_exact_order() -> TestResult {
    let digest = Fingerprint::from_bytes([0xab; 32]);
    let resolved = schema("research", Some("cause"))?;
    let provenance = CommitProvenance::new(resolved.clone(), digest);
    assert_eq!(provenance.schema(), &resolved);
    assert_eq!(provenance.fingerprint(), digest);
    let draft = draft(Some(("cause", "The sample split leaked speaker identity.")))?;
    let before = draft.clone();
    let message = render_commit_message_with_provenance(&provenance, &draft)?;
    assert_eq!(
        message.as_str(),
        concat!(
            "fix(data-source): preserve the evidence\n\ncause:\n",
            "The sample split leaked speaker identity.\n\n",
            "Gitserious-Template: research-template@4\n",
            "Gitserious-Taxonomy: research@2\n",
            "Gitserious-Typeset: research/default@3\n",
            "Gitserious-Schema: sha256:abababababababababababababababababababababababababababababababab\n"
        )
    );
    assert_eq!(
        render_commit_message_with_provenance(&provenance, &draft)?,
        message
    );
    assert_eq!(draft, before);
    Ok(())
}

#[test]
fn overlapping_types_are_validated_against_the_provenance_schema() -> TestResult {
    let research = CommitProvenance::new(
        schema("research", Some("cause"))?,
        Fingerprint::from_bytes([1; 32]),
    );
    let ops = CommitProvenance::new(
        schema("ops", Some("impact"))?,
        Fingerprint::from_bytes([2; 32]),
    );
    let draft = draft(Some(("cause", "The sample split leaked speaker identity.")))?;
    assert!(render_commit_message_with_provenance(&research, &draft).is_ok());
    let errors = render_commit_message_with_provenance(&ops, &draft)
        .err()
        .ok_or("wrong schema accepted")?;
    assert_eq!(
        errors.as_slice(),
        &[
            CommitValidationError::UnknownProperty(PropertyKey::new("cause")?),
            CommitValidationError::MissingRequired(PropertyKey::new("impact")?),
        ]
    );
    let unknown = CommitDraft::from_responses(
        CommitTypeId::new("absent")?,
        None,
        CommitSubject::new("reject unknown type")?,
        vec![],
    )?;
    let errors = render_commit_message_with_provenance(&research, &unknown)
        .err()
        .ok_or("unknown type accepted")?;
    assert_eq!(
        errors.as_slice(),
        &[CommitValidationError::UnknownCommitType {
            template: TemplateId::new("research-template")?,
            actual: CommitTypeId::new("absent")?,
        }]
    );
    assert!(errors.to_string().contains("research-template"));
    Ok(())
}

#[test]
fn provenance_does_not_change_wrapped_body_or_breaking_change_bytes() -> TestResult {
    let resolved = schema("research", Some("cause"))?;
    let definition = resolved.change_types()[0].commit_type_definition();
    let provenance = CommitProvenance::new(resolved, Fingerprint::from_bytes([0; 32]));
    let draft = draft(Some(("cause", &"Context 界 🦀 e\u{301} ".repeat(30))))?
        .with_breaking_change(PropertyValue::new("Clients must migrate. ".repeat(20))?);
    let canonical = render_commit_message(&definition, &draft)?;
    let with_provenance = render_commit_message_with_provenance(&provenance, &draft)?;
    assert!(
        canonical
            .as_str()
            .lines()
            .all(|line| UnicodeWidthStr::width(line) <= 80)
    );
    assert!(
        with_provenance
            .as_str()
            .starts_with(&format!("{canonical}\nGitserious-Template:"))
    );
    let tail = with_provenance
        .as_str()
        .strip_prefix(canonical.as_str())
        .ok_or("message prefix changed")?;
    let trailers: Vec<_> = tail.trim_start_matches('\n').lines().collect();
    assert_eq!(trailers.len(), 4);
    assert!(trailers[3].starts_with("Gitserious-Schema: sha256:"));
    assert!(UnicodeWidthStr::width(trailers[3]) > 80);
    Ok(())
}

#[test]
fn empty_schemas_render_header_and_provenance_without_empty_properties() -> TestResult {
    let provenance =
        CommitProvenance::new(schema("empty", None)?, Fingerprint::from_bytes([0; 32]));
    let message = render_commit_message_with_provenance(&provenance, &draft(None)?)?;
    assert!(message.as_str().starts_with(
        "fix(data-source): preserve the evidence\n\nGitserious-Template: empty-template@4\n"
    ));
    assert_eq!(message.as_str().lines().count(), 6);
    Ok(())
}

#[test]
fn provenance_rendering_keeps_explicit_applicability_validation() -> TestResult {
    let built_in = built_in_configuration();
    let resolved =
        ResolvedTaxonomy::resolve(built_in.template(), built_in.taxonomy(), built_in.typeset())?;
    let provenance = CommitProvenance::new(resolved, Fingerprint::from_bytes([0; 32]));
    let mut responses = ["intent", "decision"]
        .into_iter()
        .map(|key| -> Result<_, Box<dyn Error>> {
            Ok(PropertyResponse::new(
                PropertyKey::new(key)?,
                Some(PropertyValues::single(PropertyValue::new("context")?)),
                None,
            ))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let unanswered = CommitDraft::from_responses(
        CommitTypeId::new("feat")?,
        None,
        CommitSubject::new("enforce applicability")?,
        responses.clone(),
    )?;
    assert!(render_commit_message_with_provenance(&provenance, &unanswered).is_err());
    responses.push(PropertyResponse::new(
        PropertyKey::new("constraints")?,
        None,
        Some(ConditionalApplicability::DoesNotApply),
    ));
    let answered = CommitDraft::from_responses(
        unanswered.commit_type().clone(),
        None,
        unanswered.subject().clone(),
        responses,
    )?;
    let message = render_commit_message_with_provenance(&provenance, &answered)?;
    assert!(
        message
            .as_str()
            .contains("Gitserious-Template: default@1\n")
    );
    assert!(!message.as_str().contains("constraints:"));
    // Existing adapters can continue rendering legacy drafts during migration.
    let legacy = CommitDraft::new(
        answered.commit_type().clone(),
        None,
        answered.subject().clone(),
        answered.properties().to_vec(),
    )?;
    assert_eq!(
        render_commit_message_with_provenance(&provenance, &legacy)?,
        message
    );
    Ok(())
}

#[test]
fn fingerprint_representation_rejects_malformed_or_injectable_values() -> TestResult {
    for bytes in [[0; 32], [0xab; 32], [0xff; 32]] {
        let fingerprint = Fingerprint::from_bytes(bytes);
        assert_eq!(fingerprint.as_bytes(), bytes);
        assert_eq!(fingerprint.to_string().parse::<Fingerprint>()?, fingerprint);
    }
    for (text, expected) in [
        (
            format!("SHA256:{}", "0".repeat(64)),
            FingerprintError::InvalidPrefix,
        ),
        (
            format!("sha256:{}", "0".repeat(63)),
            FingerprintError::InvalidLength(63),
        ),
        (
            format!("sha256:{}", "0".repeat(65)),
            FingerprintError::InvalidLength(65),
        ),
        (
            format!("sha256:A{}", "0".repeat(63)),
            FingerprintError::InvalidCharacter(0),
        ),
        (
            format!("sha256:é{}", "0".repeat(62)),
            FingerprintError::InvalidCharacter(0),
        ),
        (
            format!("sha256:{}\n", "0".repeat(63)),
            FingerprintError::InvalidCharacter(63),
        ),
    ] {
        assert_eq!(text.parse::<Fingerprint>(), Err(expected));
    }
    Ok(())
}
