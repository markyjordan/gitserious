use std::error::Error;
use std::fmt::Write as _;

use gitserious_core::{
    CommitDraft, CommitSubject, CommitValidationError, PropertyMultiplicity, PropertyRequirement,
    PropertyResponse, PropertyValue, PropertyValues, ResolvedTaxonomy, TaxonomyVersion, TemplateId,
    TemplateVersion, TypesetVersion, built_in_configuration, render_commit_message,
    validate_commit_draft_report,
};

type TestResult = Result<(), Box<dyn Error>>;

fn resolve(id: &str) -> Result<ResolvedTaxonomy, Box<dyn Error>> {
    let catalog = built_in_configuration();
    let template = catalog
        .find_template(&TemplateId::new(id)?)
        .ok_or("missing template")?;
    let taxonomy = catalog
        .find_taxonomy(template.taxonomy())
        .ok_or("missing taxonomy")?;
    let typeset = catalog
        .find_typeset(template.taxonomy(), template.typeset())
        .ok_or("missing typeset")?;
    Ok(ResolvedTaxonomy::resolve(template, taxonomy, typeset)?)
}

fn verify_bundle(id: &str, expected: &[(&str, &str)]) -> TestResult {
    let schema = resolve(id)?;
    assert_eq!(schema.template_id().as_str(), id);
    assert_eq!(schema.taxonomy_id().as_str(), id);
    assert_eq!(schema.typeset_id().as_str(), "default");
    assert_eq!(schema.template_version(), TemplateVersion::V1);
    assert_eq!(schema.taxonomy_version(), TaxonomyVersion::V1);
    assert_eq!(schema.typeset_version(), TypesetVersion::V1);
    assert!(!schema.template_description().as_str().trim().is_empty());
    assert!(!schema.taxonomy_description().as_str().trim().is_empty());
    assert!(!schema.typeset_description().as_str().trim().is_empty());
    assert_eq!(schema.change_types().len(), expected.len());
    for (change, (expected_type, expected_properties)) in schema.change_types().iter().zip(expected)
    {
        assert_eq!(change.id().as_str(), *expected_type);
        assert!(!change.description().as_str().trim().is_empty());
        let actual = change
            .properties()
            .iter()
            .map(|property| {
                assert_eq!(property.multiplicity(), PropertyMultiplicity::Single);
                assert!(!property.description().trim().is_empty());
                let suffix = match property.requirement() {
                    PropertyRequirement::Required => "!",
                    PropertyRequirement::Recommended => "?",
                    other => return Err(format!("unexpected requirement: {other:?}").into()),
                };
                Ok(format!("{}{suffix}", property.key()))
            })
            .collect::<Result<Vec<_>, Box<dyn Error>>>()?
            .join(" ");
        assert_eq!(actual, *expected_properties, "{id}/{expected_type}");
        let responses = change
            .properties()
            .iter()
            .map(|property| {
                Ok(PropertyResponse::new(
                    property.key().clone(),
                    Some(PropertyValues::single(PropertyValue::new(format!(
                        "context for {}",
                        property.key()
                    ))?)),
                    None,
                ))
            })
            .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
        let definition = change.commit_type_definition();
        let complete = CommitDraft::from_responses(
            change.id().clone(),
            None,
            CommitSubject::new("record context")?,
            responses.clone(),
        )?;
        let report = validate_commit_draft_report(&definition, &complete);
        assert!(!report.has_errors());
        assert!(report.warnings().is_empty());
        let rendered = render_commit_message(&definition, &complete)?;
        let mut expected_message = format!("{expected_type}: record context\n");
        for property in change.properties() {
            write!(
                expected_message,
                "\n{}:\ncontext for {}\n",
                property.key(),
                property.key()
            )?;
        }
        assert_eq!(rendered.as_str(), expected_message);
        // An omitted recommendation warns; each omitted required value blocks.
        for property in change.properties() {
            let missing = CommitDraft::from_responses(
                change.id().clone(),
                None,
                complete.subject().clone(),
                responses
                    .iter()
                    .filter(|response| response.key() != property.key())
                    .cloned()
                    .collect(),
            )?;
            let report = validate_commit_draft_report(&definition, &missing);
            match property.requirement() {
                PropertyRequirement::Required => {
                    assert_eq!(
                        report.errors(),
                        &[CommitValidationError::MissingRequired(
                            property.key().clone()
                        )]
                    );
                    assert!(render_commit_message(&definition, &missing).is_err());
                }
                PropertyRequirement::Recommended => {
                    assert!(!report.has_errors());
                    assert_eq!(report.warnings().len(), 1);
                    assert!(render_commit_message(&definition, &missing).is_ok());
                }
                other => return Err(format!("unexpected requirement: {other:?}").into()),
            }
        }
    }
    Ok(())
}

#[test]
fn ml_research_preserves_the_agreed_schema_and_requirement_contract() -> TestResult {
    verify_bundle(
        "ml-research",
        &[
            (
                "hypothesis",
                "claim! motivation! prediction! falsifier? assumptions?",
            ),
            (
                "data",
                "objective! population? transformation! assumptions? leakage-risk? validation?",
            ),
            (
                "model",
                "objective! change! rationale! assumptions? tradeoffs?",
            ),
            (
                "experiment",
                "question! intervention! control! prediction? confounders? result?",
            ),
            ("eval", "target! protocol! metrics! rationale? limitations?"),
            (
                "analysis",
                "evidence! finding! interpretation! confidence? next-question?",
            ),
            (
                "reproduce",
                "source! target-result! deviations? result! discrepancy?",
            ),
            (
                "fix",
                "symptom! cause! affected-results? decision! validation?",
            ),
            (
                "infra",
                "objective! change! experimental-impact? reproducibility-impact? validation?",
            ),
            ("docs", "intent! decision! audience? validation?"),
        ],
    )
}

#[test]
fn domain_defaults_do_not_change_conventional_compatibility_or_order() -> TestResult {
    let catalog = built_in_configuration();
    assert_eq!(
        catalog
            .taxonomies()
            .iter()
            .map(|t| t.id().as_str())
            .collect::<Vec<_>>(),
        ["conventional", "ml-research"]
    );
    assert_eq!(
        catalog
            .templates()
            .iter()
            .map(|t| t.id().as_str())
            .collect::<Vec<_>>(),
        ["default", "ml-research"]
    );
    let conventional = resolve("default")?;
    assert_eq!(catalog.taxonomy().id(), conventional.taxonomy_id());
    assert_eq!(catalog.typeset().id(), conventional.typeset_id());
    assert_eq!(catalog.template().id(), conventional.template_id());
    assert_eq!(
        conventional
            .change_types()
            .iter()
            .map(|t| t.id().as_str())
            .collect::<Vec<_>>(),
        [
            "feat", "fix", "refactor", "perf", "test", "docs", "chore", "build", "ci", "style",
            "revert"
        ]
    );
    let ml = resolve("ml-research")?;
    let ml_fix = ml
        .change_types()
        .iter()
        .find(|t| t.id().as_str() == "fix")
        .ok_or("ML fix missing")?;
    let conventional_fix = conventional
        .change_types()
        .iter()
        .find(|t| t.id().as_str() == "fix")
        .ok_or("Conventional fix missing")?;
    assert_ne!(ml_fix.properties(), conventional_fix.properties());
    Ok(())
}
