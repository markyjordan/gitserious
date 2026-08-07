use std::sync::LazyLock;

use crate::{
    CommitTypeDefinition, CommitTypeId, ConditionId, PropertyCondition, PropertyDefinition,
    PropertyKey, PropertyRequirement,
};

static BUILT_IN_COMMIT_TYPES: LazyLock<Vec<CommitTypeDefinition>> = LazyLock::new(|| {
    vec![
        commit_type(
            "feat",
            "An intentional addition or expansion of capability.",
            vec![
                required("intent", "Why the capability is being introduced."),
                required("behavior", "What users, callers, or the system can now do."),
                recommended(
                    "constraints",
                    "Design, compatibility, product, or operational boundaries.",
                ),
                recommended(
                    "invariants",
                    "Properties that must remain true despite the addition.",
                ),
                recommended("validation", "Evidence that the intended behavior works."),
            ],
        ),
        commit_type(
            "fix",
            "A causal correction of an observed failure or incorrect behavior.",
            vec![
                required(
                    "symptom",
                    "The externally observed failure or incorrect behavior.",
                ),
                required(
                    "cause",
                    "The underlying mechanism that produced the failure.",
                ),
                required("decision", "Why this correction was selected."),
                recommended(
                    "effect",
                    "The behavior or state that the correction restores or changes.",
                ),
                recommended("validation", "How recurrence was checked or prevented."),
            ],
        ),
        commit_type(
            "refactor",
            "A structural transformation that preserves intended behavior.",
            vec![
                required("problem", "The structural deficiency being addressed."),
                required(
                    "transformation",
                    "The conceptual restructuring that was performed.",
                ),
                required("invariant", "The behavior that must not change."),
                recommended(
                    "benefit",
                    "The maintainability, extensibility, clarity, or architectural improvement.",
                ),
                recommended(
                    "validation",
                    "Evidence that preserved behavior remains intact.",
                ),
            ],
        ),
        commit_type(
            "perf",
            "An optimization supported by performance evidence.",
            vec![
                required(
                    "bottleneck",
                    "The measured or observed performance limitation.",
                ),
                required("change", "The optimization that was applied."),
                conditional(
                    "tradeoff",
                    "The complexity, memory, precision, latency, or maintainability cost.",
                    "optimization-has-known-cost",
                    "Required when the optimization knowingly increases another resource cost or complexity.",
                ),
                recommended("result", "The claimed performance improvement."),
                required(
                    "measurement",
                    "The benchmark method, environment, or evidence supporting the claim.",
                ),
            ],
        ),
        commit_type(
            "test",
            "Protection against a specific uncertainty or failure mode.",
            vec![
                required(
                    "risk",
                    "What could regress or was insufficiently specified.",
                ),
                required("coverage", "The scenarios or boundaries now exercised."),
                required(
                    "expected-behavior",
                    "The behavioral contract asserted by the tests.",
                ),
                recommended(
                    "production-impact",
                    "Whether fixtures, test hooks, or production seams changed.",
                ),
            ],
        ),
        commit_type(
            "docs",
            "A correction or expansion of the repository's knowledge surface.",
            vec![
                required(
                    "knowledge-gap",
                    "What was missing, ambiguous, or incorrect.",
                ),
                required("update", "The conceptual documentation change."),
                recommended(
                    "audience",
                    "The users, contributors, maintainers, operators, or integrators served.",
                ),
                recommended("code-impact", "Whether executable behavior changed."),
            ],
        ),
        commit_type(
            "chore",
            "Necessary maintenance that does not fit a more precise change type.",
            vec![
                required("rationale", "Why the maintenance is needed."),
                required("change", "The maintenance action that occurred."),
                recommended(
                    "operational-impact",
                    "The effect on contributors, tooling, environments, or processes.",
                ),
                required("behavioral-impact", "Whether runtime behavior changes."),
                recommended(
                    "validation",
                    "Evidence that the maintenance did not destabilize the project.",
                ),
            ],
        ),
        commit_type(
            "build",
            "A change to how project artifacts are constructed.",
            vec![
                required("problem", "The build-system limitation or requirement."),
                required("change", "The conceptual build-system modification."),
                recommended(
                    "environment-impact",
                    "The affected toolchains, platforms, or developer environments.",
                ),
                required(
                    "artifact-impact",
                    "Changes to produced binaries, packages, metadata, or reproducibility.",
                ),
                recommended(
                    "validation",
                    "The build matrices or artifact checks that were performed.",
                ),
            ],
        ),
        commit_type(
            "ci",
            "A change to continuous-integration pipeline behavior.",
            vec![
                required("objective", "The reliability or delivery outcome sought."),
                required(
                    "pipeline-change",
                    "The conceptual workflow or pipeline change.",
                ),
                recommended(
                    "trigger",
                    "The events or branches on which the pipeline operates.",
                ),
                required(
                    "failure-semantics",
                    "What a failure blocks, warns about, retries, or permits.",
                ),
                recommended(
                    "cost",
                    "The runtime, compute, maintenance, or contributor-latency implications.",
                ),
                conditional(
                    "permissions",
                    "The security-relevant workflow permission changes.",
                    "workflow-permissions-change",
                    "Required when the pipeline change modifies workflow or action permissions.",
                ),
            ],
        ),
        commit_type(
            "style",
            "A non-behavioral source-presentation change.",
            vec![
                required(
                    "change",
                    "The formatting or presentation convention applied.",
                ),
                required(
                    "behavioral-impact",
                    "The explicit claim that runtime behavior is unchanged.",
                ),
                optional(
                    "review-note",
                    "Anything that makes the diff harder to inspect, such as widespread formatting churn.",
                ),
            ],
        ),
        commit_type(
            "revert",
            "An intentional restoration of an earlier repository state.",
            vec![
                required("reverts", "The commit or logical change being reversed."),
                required(
                    "reason",
                    "Why rollback is preferable to continuing with the reverted change.",
                ),
                required(
                    "restored-state",
                    "The behavior or state to which the repository returns.",
                ),
                optional(
                    "follow-up",
                    "Whether the original work will be revised, replaced, or abandoned.",
                ),
            ],
        ),
    ]
});

/// Returns the versioned built-in commit-type schemas in deterministic order.
#[must_use]
pub fn built_in_commit_types() -> &'static [CommitTypeDefinition] {
    BUILT_IN_COMMIT_TYPES.as_slice()
}

fn commit_type(
    id: &'static str,
    description: &'static str,
    properties: Vec<PropertyDefinition>,
) -> CommitTypeDefinition {
    CommitTypeDefinition::from_trusted(CommitTypeId::from_trusted(id), description, properties)
}

fn property(
    key: &'static str,
    description: &'static str,
    requirement: PropertyRequirement,
) -> PropertyDefinition {
    PropertyDefinition::from_trusted(PropertyKey::from_trusted(key), description, requirement)
}

fn required(key: &'static str, description: &'static str) -> PropertyDefinition {
    property(key, description, PropertyRequirement::Required)
}

fn recommended(key: &'static str, description: &'static str) -> PropertyDefinition {
    property(key, description, PropertyRequirement::Recommended)
}

fn optional(key: &'static str, description: &'static str) -> PropertyDefinition {
    property(key, description, PropertyRequirement::Optional)
}

fn conditional(
    key: &'static str,
    description: &'static str,
    condition_id: &'static str,
    rationale: &'static str,
) -> PropertyDefinition {
    let condition =
        PropertyCondition::from_trusted(ConditionId::from_trusted(condition_id), rationale);
    property(
        key,
        description,
        PropertyRequirement::Conditional(condition),
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use crate::{PropertyMultiplicity, PropertyRequirement, SchemaVersion, built_in_commit_types};

    #[derive(Clone, Copy)]
    enum ExpectedRequirement {
        Required,
        Recommended,
        Optional,
        Conditional {
            id: &'static str,
            rationale: &'static str,
        },
    }

    struct ExpectedProperty {
        key: &'static str,
        description: &'static str,
        requirement: ExpectedRequirement,
    }

    struct ExpectedCommitType {
        id: &'static str,
        description: &'static str,
        properties: &'static [ExpectedProperty],
    }

    use ExpectedRequirement::{Conditional, Optional, Recommended, Required};

    const EXPECTED: &[ExpectedCommitType] = &[
        ExpectedCommitType {
            id: "feat",
            description: "An intentional addition or expansion of capability.",
            properties: &[
                ExpectedProperty {
                    key: "intent",
                    description: "Why the capability is being introduced.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "behavior",
                    description: "What users, callers, or the system can now do.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "constraints",
                    description: "Design, compatibility, product, or operational boundaries.",
                    requirement: Recommended,
                },
                ExpectedProperty {
                    key: "invariants",
                    description: "Properties that must remain true despite the addition.",
                    requirement: Recommended,
                },
                ExpectedProperty {
                    key: "validation",
                    description: "Evidence that the intended behavior works.",
                    requirement: Recommended,
                },
            ],
        },
        ExpectedCommitType {
            id: "fix",
            description: "A causal correction of an observed failure or incorrect behavior.",
            properties: &[
                ExpectedProperty {
                    key: "symptom",
                    description: "The externally observed failure or incorrect behavior.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "cause",
                    description: "The underlying mechanism that produced the failure.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "decision",
                    description: "Why this correction was selected.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "effect",
                    description: "The behavior or state that the correction restores or changes.",
                    requirement: Recommended,
                },
                ExpectedProperty {
                    key: "validation",
                    description: "How recurrence was checked or prevented.",
                    requirement: Recommended,
                },
            ],
        },
        ExpectedCommitType {
            id: "refactor",
            description: "A structural transformation that preserves intended behavior.",
            properties: &[
                ExpectedProperty {
                    key: "problem",
                    description: "The structural deficiency being addressed.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "transformation",
                    description: "The conceptual restructuring that was performed.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "invariant",
                    description: "The behavior that must not change.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "benefit",
                    description: "The maintainability, extensibility, clarity, or architectural improvement.",
                    requirement: Recommended,
                },
                ExpectedProperty {
                    key: "validation",
                    description: "Evidence that preserved behavior remains intact.",
                    requirement: Recommended,
                },
            ],
        },
        ExpectedCommitType {
            id: "perf",
            description: "An optimization supported by performance evidence.",
            properties: &[
                ExpectedProperty {
                    key: "bottleneck",
                    description: "The measured or observed performance limitation.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "change",
                    description: "The optimization that was applied.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "tradeoff",
                    description: "The complexity, memory, precision, latency, or maintainability cost.",
                    requirement: Conditional {
                        id: "optimization-has-known-cost",
                        rationale: "Required when the optimization knowingly increases another resource cost or complexity.",
                    },
                },
                ExpectedProperty {
                    key: "result",
                    description: "The claimed performance improvement.",
                    requirement: Recommended,
                },
                ExpectedProperty {
                    key: "measurement",
                    description: "The benchmark method, environment, or evidence supporting the claim.",
                    requirement: Required,
                },
            ],
        },
        ExpectedCommitType {
            id: "test",
            description: "Protection against a specific uncertainty or failure mode.",
            properties: &[
                ExpectedProperty {
                    key: "risk",
                    description: "What could regress or was insufficiently specified.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "coverage",
                    description: "The scenarios or boundaries now exercised.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "expected-behavior",
                    description: "The behavioral contract asserted by the tests.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "production-impact",
                    description: "Whether fixtures, test hooks, or production seams changed.",
                    requirement: Recommended,
                },
            ],
        },
        ExpectedCommitType {
            id: "docs",
            description: "A correction or expansion of the repository's knowledge surface.",
            properties: &[
                ExpectedProperty {
                    key: "knowledge-gap",
                    description: "What was missing, ambiguous, or incorrect.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "update",
                    description: "The conceptual documentation change.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "audience",
                    description: "The users, contributors, maintainers, operators, or integrators served.",
                    requirement: Recommended,
                },
                ExpectedProperty {
                    key: "code-impact",
                    description: "Whether executable behavior changed.",
                    requirement: Recommended,
                },
            ],
        },
        ExpectedCommitType {
            id: "chore",
            description: "Necessary maintenance that does not fit a more precise change type.",
            properties: &[
                ExpectedProperty {
                    key: "rationale",
                    description: "Why the maintenance is needed.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "change",
                    description: "The maintenance action that occurred.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "operational-impact",
                    description: "The effect on contributors, tooling, environments, or processes.",
                    requirement: Recommended,
                },
                ExpectedProperty {
                    key: "behavioral-impact",
                    description: "Whether runtime behavior changes.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "validation",
                    description: "Evidence that the maintenance did not destabilize the project.",
                    requirement: Recommended,
                },
            ],
        },
        ExpectedCommitType {
            id: "build",
            description: "A change to how project artifacts are constructed.",
            properties: &[
                ExpectedProperty {
                    key: "problem",
                    description: "The build-system limitation or requirement.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "change",
                    description: "The conceptual build-system modification.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "environment-impact",
                    description: "The affected toolchains, platforms, or developer environments.",
                    requirement: Recommended,
                },
                ExpectedProperty {
                    key: "artifact-impact",
                    description: "Changes to produced binaries, packages, metadata, or reproducibility.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "validation",
                    description: "The build matrices or artifact checks that were performed.",
                    requirement: Recommended,
                },
            ],
        },
        ExpectedCommitType {
            id: "ci",
            description: "A change to continuous-integration pipeline behavior.",
            properties: &[
                ExpectedProperty {
                    key: "objective",
                    description: "The reliability or delivery outcome sought.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "pipeline-change",
                    description: "The conceptual workflow or pipeline change.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "trigger",
                    description: "The events or branches on which the pipeline operates.",
                    requirement: Recommended,
                },
                ExpectedProperty {
                    key: "failure-semantics",
                    description: "What a failure blocks, warns about, retries, or permits.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "cost",
                    description: "The runtime, compute, maintenance, or contributor-latency implications.",
                    requirement: Recommended,
                },
                ExpectedProperty {
                    key: "permissions",
                    description: "The security-relevant workflow permission changes.",
                    requirement: Conditional {
                        id: "workflow-permissions-change",
                        rationale: "Required when the pipeline change modifies workflow or action permissions.",
                    },
                },
            ],
        },
        ExpectedCommitType {
            id: "style",
            description: "A non-behavioral source-presentation change.",
            properties: &[
                ExpectedProperty {
                    key: "change",
                    description: "The formatting or presentation convention applied.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "behavioral-impact",
                    description: "The explicit claim that runtime behavior is unchanged.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "review-note",
                    description: "Anything that makes the diff harder to inspect, such as widespread formatting churn.",
                    requirement: Optional,
                },
            ],
        },
        ExpectedCommitType {
            id: "revert",
            description: "An intentional restoration of an earlier repository state.",
            properties: &[
                ExpectedProperty {
                    key: "reverts",
                    description: "The commit or logical change being reversed.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "reason",
                    description: "Why rollback is preferable to continuing with the reverted change.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "restored-state",
                    description: "The behavior or state to which the repository returns.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "follow-up",
                    description: "Whether the original work will be revised, replaced, or abandoned.",
                    requirement: Optional,
                },
            ],
        },
    ];

    #[test]
    fn built_in_catalog_matches_the_complete_version_one_contract() {
        let actual = built_in_commit_types();

        assert_eq!(actual.len(), EXPECTED.len());
        for (definition, expected) in actual.iter().zip(EXPECTED) {
            assert_eq!(definition.schema_version(), SchemaVersion::V1);
            assert_eq!(definition.id().as_str(), expected.id);
            assert_eq!(definition.description(), expected.description);
            assert!(!definition.description().trim().is_empty());
            assert_eq!(definition.properties().len(), expected.properties.len());

            for (property, expected_property) in
                definition.properties().iter().zip(expected.properties)
            {
                assert_eq!(property.key().as_str(), expected_property.key);
                assert_eq!(property.description(), expected_property.description);
                assert!(!property.description().trim().is_empty());
                assert_eq!(property.multiplicity(), PropertyMultiplicity::Single);
                assert_requirement(property.requirement(), expected_property.requirement);
            }
        }
    }

    #[test]
    fn built_in_catalog_is_immutable_ordered_unique_and_excludes_security() {
        let first = built_in_commit_types();
        let second = built_in_commit_types();
        assert!(std::ptr::eq(first, second));

        let ids = first
            .iter()
            .map(|definition| definition.id().as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            [
                "feat", "fix", "refactor", "perf", "test", "docs", "chore", "build", "ci", "style",
                "revert"
            ]
        );
        assert!(!ids.contains(&"security"));
        assert_eq!(
            ids.iter().copied().collect::<BTreeSet<_>>().len(),
            ids.len()
        );

        for definition in first {
            let keys = definition
                .properties()
                .iter()
                .map(|property| property.key().as_str())
                .collect::<Vec<_>>();
            assert_eq!(
                keys.iter().copied().collect::<BTreeSet<_>>().len(),
                keys.len()
            );
        }
    }

    fn assert_requirement(actual: &PropertyRequirement, expected: ExpectedRequirement) {
        match expected {
            Required => assert_eq!(actual, &PropertyRequirement::Required),
            Recommended => assert_eq!(actual, &PropertyRequirement::Recommended),
            Optional => assert_eq!(actual, &PropertyRequirement::Optional),
            Conditional { id, rationale } => {
                let PropertyRequirement::Conditional(condition) = actual else {
                    assert!(matches!(actual, PropertyRequirement::Conditional(_)));
                    return;
                };
                assert_eq!(condition.id().as_str(), id);
                assert_eq!(condition.rationale(), rationale);
            }
        }
    }
}
