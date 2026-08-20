use std::sync::LazyLock;

use crate::{
    CommitTypeDefinition, CommitTypeId, ConditionId, PropertyCondition, PropertyDefinition,
    PropertyKey, PropertyRequirement,
};

static BUILT_IN_COMMIT_TYPES: LazyLock<Vec<CommitTypeDefinition>> = LazyLock::new(|| {
    vec![
        commit_type(
            "feat",
            "An addition or expansion of capability.",
            vec![
                required(
                    "intent",
                    "Why the product or system needs this capability. The implementation shows how it works, not why it should exist.",
                ),
                required(
                    "decision",
                    "Why this implementation was selected among reasonable alternatives. The diff records only the winner.",
                ),
                conditional(
                    "constraints",
                    "Non-obvious requirements that bounded the change and cannot be recovered from the implementation with confidence.",
                    "non-obvious-requirements",
                    "Required when non-obvious requirements bounded the change.",
                ),
            ],
        ),
        commit_type(
            "fix",
            "A causal correction of an observed failure or incorrect behavior.",
            vec![
                required(
                    "cause",
                    "The causal model established while debugging. The diff shows the condition that changed, not why it was the cause.",
                ),
                required(
                    "decision",
                    "Why this layer was repaired rather than a superficially equivalent intervention.",
                ),
                conditional(
                    "constraints",
                    "Why the obvious fix was unacceptable, and what behavior the repair must preserve.",
                    "preserved-behavior",
                    "Required when the obvious fix was unacceptable or specific behavior must be preserved.",
                ),
            ],
        ),
        commit_type(
            "refactor",
            "A structural transformation that preserves intended behavior.",
            vec![
                required("motivation", "The structural problem that justified churn."),
                required(
                    "decision",
                    "The restructuring that was chosen instead of another equally plausible structure.",
                ),
                required(
                    "invariant",
                    "What intentionally did not change despite the movement of code.",
                ),
            ],
        ),
        commit_type(
            "perf",
            "An optimization supported by performance evidence.",
            vec![
                required("bottleneck", "What was actually expensive."),
                required("decision", "Why this optimization was selected."),
                required("result", "The measured change."),
                conditional(
                    "tradeoff",
                    "Complexity, memory, or other cost knowingly accepted.",
                    "optimization-has-known-cost",
                    "Required when the optimization knowingly increases another resource cost or complexity.",
                ),
            ],
        ),
        commit_type(
            "test",
            "Protection against a specific uncertainty or failure mode.",
            vec![
                required(
                    "risk",
                    "The future regression important enough to permanently encode this test. Often not obvious from the assertion.",
                ),
                conditional(
                    "rationale",
                    "Why this fixture, boundary, fuzz case, or regression input matters.",
                    "scenario-justification",
                    "Required when the scenario is a strange fixture, boundary, fuzz case, or regression input.",
                ),
            ],
        ),
        commit_type(
            "docs",
            "A correction or expansion of the repository's knowledge surface.",
            vec![conditional(
                "reason",
                "Why existing documentation was insufficient, misleading, or stale. A typo correction needs no body.",
                "insufficient-documentation",
                "Required when existing documentation was insufficient, misleading, or stale.",
            )],
        ),
        commit_type(
            "chore",
            "Necessary maintenance that does not fit a more precise change type.",
            vec![
                required("reason", "Why this maintenance is necessary."),
                conditional(
                    "impact",
                    "Non-obvious operational or developer consequence.",
                    "non-obvious-operational-consequence",
                    "Required when there is a non-obvious operational or developer consequence.",
                ),
            ],
        ),
        commit_type(
            "build",
            "A change to how project artifacts are constructed.",
            vec![
                required(
                    "intent",
                    "Why the build needed to change. Often a response to the environment rather than a product design.",
                ),
                conditional(
                    "constraint",
                    "External or toolchain constraint that forced the change.",
                    "external-toolchain-constraint",
                    "Required when an external or toolchain constraint caused the change.",
                ),
                conditional(
                    "impact",
                    "Non-obvious artifact or release consequence.",
                    "non-obvious-artifact-consequence",
                    "Required when the change has a non-obvious artifact or release consequence.",
                ),
            ],
        ),
        commit_type(
            "ci",
            "A change to continuous-integration pipeline behavior.",
            vec![
                required(
                    "intent",
                    "The operational property the pipeline should obtain.",
                ),
                required(
                    "decision",
                    "Why this pipeline design was selected. Workflow YAML shows the mechanism, not the policy.",
                ),
                conditional(
                    "failure-semantics",
                    "What failure now means (block, warn, visible-but-non-blocking).",
                    "failure-meaning-changed",
                    "Required when what a pipeline failure means has changed.",
                ),
            ],
        ),
        commit_type(
            "style",
            "A non-behavioral source-presentation change.",
            vec![conditional(
                "reason",
                "Why a purely non-behavioral rewrite was worth recording. Most style commits should have no body.",
                "non-behavioral-rewrite-justification",
                "Required when a purely non-behavioral rewrite is worth recording.",
            )],
        ),
        commit_type(
            "revert",
            "An intentional restoration of an earlier repository state.",
            vec![
                required(
                    "reason",
                    "Why the prior change must be undone. The disappearing diff does not record that judgment.",
                ),
                conditional(
                    "impact",
                    "Resulting state or consequence of the rollback.",
                    "rollback-consequence",
                    "Required when the undo has a non-obvious resulting state or consequence.",
                ),
                conditional(
                    "follow-up",
                    "Whether the rollback is tactical and what comes next.",
                    "temporary-rollback",
                    "Required when the rollback is temporary or a follow-up is planned.",
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

    use ExpectedRequirement::{Conditional, Required};

    const EXPECTED: &[ExpectedCommitType] = &[
        ExpectedCommitType {
            id: "feat",
            description: "An addition or expansion of capability.",
            properties: &[
                ExpectedProperty {
                    key: "intent",
                    description: "Why the product or system needs this capability. The implementation shows how it works, not why it should exist.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "decision",
                    description: "Why this implementation was selected among reasonable alternatives. The diff records only the winner.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "constraints",
                    description: "Non-obvious requirements that bounded the change and cannot be recovered from the implementation with confidence.",
                    requirement: Conditional {
                        id: "non-obvious-requirements",
                        rationale: "Required when non-obvious requirements bounded the change.",
                    },
                },
            ],
        },
        ExpectedCommitType {
            id: "fix",
            description: "A causal correction of an observed failure or incorrect behavior.",
            properties: &[
                ExpectedProperty {
                    key: "cause",
                    description: "The causal model established while debugging. The diff shows the condition that changed, not why it was the cause.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "decision",
                    description: "Why this layer was repaired rather than a superficially equivalent intervention.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "constraints",
                    description: "Why the obvious fix was unacceptable, and what behavior the repair must preserve.",
                    requirement: Conditional {
                        id: "preserved-behavior",
                        rationale: "Required when the obvious fix was unacceptable or specific behavior must be preserved.",
                    },
                },
            ],
        },
        ExpectedCommitType {
            id: "refactor",
            description: "A structural transformation that preserves intended behavior.",
            properties: &[
                ExpectedProperty {
                    key: "motivation",
                    description: "The structural problem that justified churn.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "decision",
                    description: "The restructuring that was chosen instead of another equally plausible structure.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "invariant",
                    description: "What intentionally did not change despite the movement of code.",
                    requirement: Required,
                },
            ],
        },
        ExpectedCommitType {
            id: "perf",
            description: "An optimization supported by performance evidence.",
            properties: &[
                ExpectedProperty {
                    key: "bottleneck",
                    description: "What was actually expensive.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "decision",
                    description: "Why this optimization was selected.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "result",
                    description: "The measured change.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "tradeoff",
                    description: "Complexity, memory, or other cost knowingly accepted.",
                    requirement: Conditional {
                        id: "optimization-has-known-cost",
                        rationale: "Required when the optimization knowingly increases another resource cost or complexity.",
                    },
                },
            ],
        },
        ExpectedCommitType {
            id: "test",
            description: "Protection against a specific uncertainty or failure mode.",
            properties: &[
                ExpectedProperty {
                    key: "risk",
                    description: "The future regression important enough to permanently encode this test. Often not obvious from the assertion.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "rationale",
                    description: "Why this fixture, boundary, fuzz case, or regression input matters.",
                    requirement: Conditional {
                        id: "scenario-justification",
                        rationale: "Required when the scenario is a strange fixture, boundary, fuzz case, or regression input.",
                    },
                },
            ],
        },
        ExpectedCommitType {
            id: "docs",
            description: "A correction or expansion of the repository's knowledge surface.",
            properties: &[ExpectedProperty {
                key: "reason",
                description: "Why existing documentation was insufficient, misleading, or stale. A typo correction needs no body.",
                requirement: Conditional {
                    id: "insufficient-documentation",
                    rationale: "Required when existing documentation was insufficient, misleading, or stale.",
                },
            }],
        },
        ExpectedCommitType {
            id: "chore",
            description: "Necessary maintenance that does not fit a more precise change type.",
            properties: &[
                ExpectedProperty {
                    key: "reason",
                    description: "Why this maintenance is necessary.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "impact",
                    description: "Non-obvious operational or developer consequence.",
                    requirement: Conditional {
                        id: "non-obvious-operational-consequence",
                        rationale: "Required when there is a non-obvious operational or developer consequence.",
                    },
                },
            ],
        },
        ExpectedCommitType {
            id: "build",
            description: "A change to how project artifacts are constructed.",
            properties: &[
                ExpectedProperty {
                    key: "intent",
                    description: "Why the build needed to change. Often a response to the environment rather than a product design.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "constraint",
                    description: "External or toolchain constraint that forced the change.",
                    requirement: Conditional {
                        id: "external-toolchain-constraint",
                        rationale: "Required when an external or toolchain constraint caused the change.",
                    },
                },
                ExpectedProperty {
                    key: "impact",
                    description: "Non-obvious artifact or release consequence.",
                    requirement: Conditional {
                        id: "non-obvious-artifact-consequence",
                        rationale: "Required when the change has a non-obvious artifact or release consequence.",
                    },
                },
            ],
        },
        ExpectedCommitType {
            id: "ci",
            description: "A change to continuous-integration pipeline behavior.",
            properties: &[
                ExpectedProperty {
                    key: "intent",
                    description: "The operational property the pipeline should obtain.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "decision",
                    description: "Why this pipeline design was selected. Workflow YAML shows the mechanism, not the policy.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "failure-semantics",
                    description: "What failure now means (block, warn, visible-but-non-blocking).",
                    requirement: Conditional {
                        id: "failure-meaning-changed",
                        rationale: "Required when what a pipeline failure means has changed.",
                    },
                },
            ],
        },
        ExpectedCommitType {
            id: "style",
            description: "A non-behavioral source-presentation change.",
            properties: &[ExpectedProperty {
                key: "reason",
                description: "Why a purely non-behavioral rewrite was worth recording. Most style commits should have no body.",
                requirement: Conditional {
                    id: "non-behavioral-rewrite-justification",
                    rationale: "Required when a purely non-behavioral rewrite is worth recording.",
                },
            }],
        },
        ExpectedCommitType {
            id: "revert",
            description: "An intentional restoration of an earlier repository state.",
            properties: &[
                ExpectedProperty {
                    key: "reason",
                    description: "Why the prior change must be undone. The disappearing diff does not record that judgment.",
                    requirement: Required,
                },
                ExpectedProperty {
                    key: "impact",
                    description: "Resulting state or consequence of the rollback.",
                    requirement: Conditional {
                        id: "rollback-consequence",
                        rationale: "Required when the undo has a non-obvious resulting state or consequence.",
                    },
                },
                ExpectedProperty {
                    key: "follow-up",
                    description: "Whether the rollback is tactical and what comes next.",
                    requirement: Conditional {
                        id: "temporary-rollback",
                        rationale: "Required when the rollback is temporary or a follow-up is planned.",
                    },
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
