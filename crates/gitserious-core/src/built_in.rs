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
