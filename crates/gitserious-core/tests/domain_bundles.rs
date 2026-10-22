use std::error::Error;
use std::fmt::Write as _;

use gitserious_core::{
    CommitDraft, CommitSubject, CommitValidationError, PropertyMultiplicity, PropertyRequirement,
    PropertyResponse, PropertyValue, PropertyValues, Taxonomy, TaxonomyVersion,
    built_in_taxonomies, render_commit_message, validate_commit_draft_report,
};

type TestResult = Result<(), Box<dyn Error>>;

fn built_in(id: &str) -> Result<&'static Taxonomy, Box<dyn Error>> {
    Ok(built_in_taxonomies()
        .iter()
        .find(|taxonomy| taxonomy.id().as_str() == id)
        .ok_or_else(|| format!("missing built-in taxonomy {id}"))?)
}

fn verify_taxonomy(id: &str, expected: &[(&str, &str)]) -> TestResult {
    let taxonomy = built_in(id)?;
    assert_eq!(taxonomy.id().as_str(), id);
    assert_eq!(taxonomy.version(), TaxonomyVersion::V1);
    assert!(!taxonomy.description().as_str().trim().is_empty());
    assert_eq!(taxonomy.derived_from(), None);
    assert_eq!(taxonomy.commit_types().len(), expected.len());
    for (change, (expected_type, expected_properties)) in
        taxonomy.commit_types().iter().zip(expected)
    {
        assert_eq!(change.id().as_str(), *expected_type);
        assert!(!change.description().trim().is_empty());
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
        let definition = change;
        let complete = CommitDraft::from_responses(
            change.id().clone(),
            None,
            CommitSubject::new("record context")?,
            responses.clone(),
        )?;
        let report = validate_commit_draft_report(definition, &complete);
        assert!(!report.has_errors());
        assert!(report.warnings().is_empty());
        let rendered = render_commit_message(definition, &complete)?;
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
            let report = validate_commit_draft_report(definition, &missing);
            match property.requirement() {
                PropertyRequirement::Required => {
                    assert_eq!(
                        report.errors(),
                        &[CommitValidationError::MissingRequired(
                            property.key().clone()
                        )]
                    );
                    assert!(render_commit_message(definition, &missing).is_err());
                }
                PropertyRequirement::Recommended => {
                    assert!(!report.has_errors());
                    assert_eq!(report.warnings().len(), 1);
                    assert!(render_commit_message(definition, &missing).is_ok());
                }
                other => return Err(format!("unexpected requirement: {other:?}").into()),
            }
        }
    }
    Ok(())
}

#[test]
fn research_preserves_the_agreed_schema_and_requirement_contract() -> TestResult {
    verify_taxonomy(
        "research",
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
fn domain_taxonomies_do_not_change_conventional_compatibility_or_order() -> TestResult {
    assert_eq!(
        built_in_taxonomies()
            .iter()
            .map(|taxonomy| taxonomy.id().as_str())
            .collect::<Vec<_>>(),
        ["conventional", "research", "infra-ops"]
    );
    let conventional = built_in("conventional")?;
    assert_eq!(
        conventional
            .commit_types()
            .iter()
            .map(|definition| definition.id().as_str())
            .collect::<Vec<_>>(),
        [
            "feat", "fix", "refactor", "perf", "test", "docs", "chore", "build", "ci", "style",
            "revert"
        ]
    );
    let fix = |taxonomy: &Taxonomy| {
        taxonomy
            .commit_types()
            .iter()
            .find(|definition| definition.id().as_str() == "fix")
            .map(|definition| definition.properties().to_vec())
    };
    assert!(fix(built_in("research")?).is_some());
    assert_ne!(fix(built_in("research")?), fix(conventional));
    Ok(())
}

#[test]
fn infra_ops_preserves_the_agreed_schema_and_requirement_contract() -> TestResult {
    verify_taxonomy(
        "infra-ops",
        &[
            (
                "provision",
                "purpose! topology! capacity-assumption? failure-domain? dependencies? rollback?",
            ),
            (
                "configure",
                "objective! change! rationale! assumptions? rollback? validation?",
            ),
            (
                "deploy",
                "objective! artifact! environment! strategy! risk? rollback? validation?",
            ),
            (
                "migrate",
                "from-state! to-state! reason! compatibility? invariants? rollback? validation?",
            ),
            (
                "scale",
                "constraint! signal! change! capacity-assumption? tradeoff? validation?",
            ),
            (
                "observe",
                "blind-spot! signal! interpretation! threshold? response? cost?",
            ),
            (
                "incident",
                "symptom! impact! trigger? cause? mitigation! follow-up?",
            ),
            (
                "recover",
                "failure-state! target-state! action! data-loss? residual-risk? validation?",
            ),
            (
                "secure",
                "threat! exposure! control! assumptions? residual-risk? validation?",
            ),
            (
                "decommission",
                "target! reason! dependencies? migration? residual-state? validation?",
            ),
        ],
    )
}
