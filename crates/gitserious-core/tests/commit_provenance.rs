use std::error::Error;
use std::fmt::Write;

use gitserious_core::{
    CommitDraft, CommitProvenance, CommitScope, CommitSubject, CommitTypeDefinition, CommitTypeId,
    CommitValidationError, ConditionalApplicability, Description, Fingerprint, PropertyDefinition,
    PropertyKey, PropertyRequirement, PropertyResponse, PropertyValue, SchemaVersion, Taxonomy,
    TaxonomyId, TaxonomyVersion, built_in_taxonomies, validate_commit_draft,
    validate_commit_draft_with_provenance,
};
use unicode_width::UnicodeWidthStr;

type TestResult = Result<(), Box<dyn Error>>;

fn taxonomy(domain: &str, required: Option<&str>) -> Result<Taxonomy, Box<dyn Error>> {
    let properties = required
        .map(|key| -> Result<_, Box<dyn Error>> {
            Ok(PropertyDefinition::new(
                PropertyKey::new(key)?,
                "Durable context.",
                PropertyRequirement::Required,
            )?)
        })
        .transpose()?
        .into_iter()
        .collect();
    Ok(Taxonomy::new(
        TaxonomyId::new(domain)?,
        TaxonomyVersion::new(2)?,
        Description::new("Domain changes.")?,
        None,
        vec![CommitTypeDefinition::new(
            SchemaVersion::V1,
            CommitTypeId::new("fix")?,
            "Correct a failure.",
            properties,
        )?],
    )?)
}

fn draft(required: Option<(&str, &str)>) -> Result<CommitDraft, Box<dyn Error>> {
    let responses = required
        .map(|(key, text)| -> Result<_, Box<dyn Error>> {
            Ok(PropertyResponse::new(
                PropertyKey::new(key)?,
                Some(PropertyValue::new(text)?),
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
    let research = taxonomy("research", Some("cause"))?;
    let provenance = CommitProvenance::new(research.clone(), digest);
    assert_eq!(provenance.taxonomy(), &research);
    assert_eq!(provenance.fingerprint(), digest);
    let draft = draft(Some(("cause", "The sample split leaked speaker identity.")))?;
    let before = draft.clone();

    let message = validate_commit_draft_with_provenance(&provenance, &draft).into_message()?;

    assert_eq!(
        message.as_str(),
        concat!(
            "fix(data-source): preserve the evidence\n\ncause:\n",
            "The sample split leaked speaker identity.\n\n",
            "Gitserious-Taxonomy: research@2\n",
            "Gitserious-Schema: sha256:abababababababababababababababababababababababababababababababab\n"
        )
    );
    assert_eq!(
        validate_commit_draft_with_provenance(&provenance, &draft).into_message()?,
        message
    );
    assert_eq!(draft, before);
    Ok(())
}

#[test]
fn overlapping_types_are_validated_against_the_provenance_taxonomy() -> TestResult {
    let research = CommitProvenance::new(
        taxonomy("research", Some("cause"))?,
        Fingerprint::from_bytes([1; 32]),
    );
    let ops = CommitProvenance::new(
        taxonomy("ops", Some("impact"))?,
        Fingerprint::from_bytes([2; 32]),
    );
    let draft = draft(Some(("cause", "The sample split leaked speaker identity.")))?;
    assert!(
        validate_commit_draft_with_provenance(&research, &draft)
            .into_message()
            .is_ok()
    );

    let errors = validate_commit_draft_with_provenance(&ops, &draft)
        .into_message()
        .err()
        .ok_or("wrong taxonomy accepted")?;
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
    let errors = validate_commit_draft_with_provenance(&research, &unknown)
        .into_message()
        .err()
        .ok_or("unknown type accepted")?;
    assert_eq!(
        errors.as_slice(),
        &[CommitValidationError::UnknownCommitType {
            taxonomy: TaxonomyId::new("research")?,
            actual: CommitTypeId::new("absent")?,
        }]
    );
    assert!(errors.to_string().contains("research"));
    Ok(())
}

#[test]
fn provenance_does_not_change_wrapped_body_or_breaking_change_bytes() -> TestResult {
    let research = taxonomy("research", Some("cause"))?;
    let definition = research.commit_types()[0].clone();
    let provenance = CommitProvenance::new(research, Fingerprint::from_bytes([0; 32]));
    let draft = draft(Some(("cause", &"Context 界 🦀 e\u{301} ".repeat(30))))?
        .with_breaking_change(PropertyValue::new("Clients must migrate. ".repeat(20))?);

    let canonical = validate_commit_draft(&definition, &draft).into_message()?;
    let with_provenance =
        validate_commit_draft_with_provenance(&provenance, &draft).into_message()?;

    assert!(
        canonical
            .as_str()
            .lines()
            .all(|line| UnicodeWidthStr::width(line) <= 80)
    );
    assert!(
        with_provenance
            .as_str()
            .starts_with(&format!("{canonical}\nGitserious-Taxonomy:"))
    );
    let tail = with_provenance
        .as_str()
        .strip_prefix(canonical.as_str())
        .ok_or("message prefix changed")?;
    let trailers: Vec<_> = tail.trim_start_matches('\n').lines().collect();
    assert_eq!(trailers.len(), 2);
    assert!(trailers[1].starts_with("Gitserious-Schema: sha256:"));
    assert!(UnicodeWidthStr::width(trailers[1]) > 80);
    Ok(())
}

#[test]
fn empty_schemas_render_header_and_provenance_without_empty_properties() -> TestResult {
    let provenance =
        CommitProvenance::new(taxonomy("empty", None)?, Fingerprint::from_bytes([0; 32]));

    let message =
        validate_commit_draft_with_provenance(&provenance, &draft(None)?).into_message()?;

    assert!(
        message.as_str().starts_with(
            "fix(data-source): preserve the evidence\n\nGitserious-Taxonomy: empty@2\n"
        )
    );
    assert_eq!(message.as_str().lines().count(), 4);
    Ok(())
}

#[test]
fn provenance_rendering_keeps_explicit_applicability_validation() -> TestResult {
    let provenance = CommitProvenance::new(
        built_in_taxonomies()[0].clone(),
        Fingerprint::from_bytes([0; 32]),
    );
    let mut responses = ["intent", "decision"]
        .into_iter()
        .map(|key| -> Result<_, Box<dyn Error>> {
            Ok(PropertyResponse::new(
                PropertyKey::new(key)?,
                Some(PropertyValue::new("context")?),
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
    assert!(
        validate_commit_draft_with_provenance(&provenance, &unanswered)
            .into_message()
            .is_err()
    );

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
    let message = validate_commit_draft_with_provenance(&provenance, &answered).into_message()?;
    assert!(
        message
            .as_str()
            .contains("Gitserious-Taxonomy: conventional@2\n")
    );
    assert!(!message.as_str().contains("constraints:"));

    let without_responses = CommitDraft::new(
        answered.commit_type().clone(),
        None,
        answered.subject().clone(),
        answered.properties().to_vec(),
    )?;
    assert_eq!(
        validate_commit_draft_with_provenance(&provenance, &without_responses).into_message()?,
        message
    );
    Ok(())
}

#[test]
fn fingerprint_representation_rejects_malformed_or_injectable_values() -> TestResult {
    for bytes in [[0; 32], [0xab; 32], [0xff; 32]] {
        let fingerprint = Fingerprint::from_bytes(bytes);
        let hex = bytes.iter().fold(String::new(), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        });
        assert_eq!(fingerprint.to_string(), format!("sha256:{hex}"));
        assert_eq!(fingerprint.to_string().parse::<Fingerprint>()?, fingerprint);
    }
    for (text, expected) in [
        (
            format!("SHA256:{}", "0".repeat(64)),
            "fingerprint must begin with sha256:".to_owned(),
        ),
        (
            format!("sha256:{}", "0".repeat(63)),
            "fingerprint must contain exactly 64 hexadecimal characters, found 63".to_owned(),
        ),
        (
            format!("sha256:{}", "0".repeat(65)),
            "fingerprint must contain exactly 64 hexadecimal characters, found 65".to_owned(),
        ),
        (
            format!("sha256:A{}", "0".repeat(63)),
            "fingerprint contains invalid lowercase hexadecimal at byte index 0".to_owned(),
        ),
        (
            format!("sha256:é{}", "0".repeat(62)),
            "fingerprint contains invalid lowercase hexadecimal at byte index 0".to_owned(),
        ),
        (
            format!("sha256:{}\n", "0".repeat(63)),
            "fingerprint contains invalid lowercase hexadecimal at byte index 63".to_owned(),
        ),
    ] {
        assert_eq!(
            text.parse::<Fingerprint>()
                .map_err(|error| error.to_string()),
            Err(expected)
        );
    }
    Ok(())
}
