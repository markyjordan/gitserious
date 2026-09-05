use std::sync::LazyLock;

use crate::CommitTypeDefinition;
use crate::built_in_domain::{BuiltInBundle, bundle, change_type, recommended, required};

static DEFINITIONS: LazyLock<Vec<CommitTypeDefinition>> = LazyLock::new(|| {
    vec![
        change_type(
            "hypothesis",
            "Introduce or revise a falsifiable research hypothesis.",
            vec![
                required(
                    "claim",
                    "The falsifiable relationship being proposed, with its intended scope.",
                ),
                required(
                    "motivation",
                    "The evidence or unresolved problem that makes this claim worth testing.",
                ),
                required(
                    "prediction",
                    "The observable outcome expected if the claim holds, including the comparison that gives it meaning.",
                ),
                recommended(
                    "falsifier",
                    "The observation or comparison that would count against the claim.",
                ),
                recommended(
                    "assumptions",
                    "Conditions the prediction depends on that the experiment does not itself establish.",
                ),
            ],
        ),
        change_type(
            "data",
            "Change data, sampling, labels, splits, filtering, preprocessing, or augmentation.",
            vec![
                required(
                    "objective",
                    "The research need motivating this data change.",
                ),
                recommended(
                    "population",
                    "The population represented after the change and the groups now excluded or underrepresented.",
                ),
                required(
                    "transformation",
                    "The data intervention and why these selection or processing choices were made.",
                ),
                recommended(
                    "assumptions",
                    "Assumptions about labels, sampling, or data quality that affect interpretation.",
                ),
                recommended(
                    "leakage-risk",
                    "How the change affects independence between training and evaluation, including known unresolved leakage risks.",
                ),
                recommended(
                    "validation",
                    "Checks performed on the changed data, their observed outcomes, and what remains unverified.",
                ),
            ],
        ),
        change_type(
            "model",
            "Change representation, architecture, objective, or inference formulation.",
            vec![
                required(
                    "objective",
                    "The capability or research question motivating the model change.",
                ),
                required(
                    "change",
                    "The modeling intervention and the behavior it is intended to alter.",
                ),
                required(
                    "rationale",
                    "Why this formulation was selected over plausible alternatives.",
                ),
                recommended(
                    "assumptions",
                    "Assumptions about the task or data on which the modeling choice depends.",
                ),
                recommended(
                    "tradeoffs",
                    "Costs or limitations knowingly accepted in exchange for the intended benefit.",
                ),
            ],
        ),
        change_type(
            "experiment",
            "Introduce or modify an intervention or controlled comparison.",
            vec![
                required(
                    "question",
                    "The specific uncertainty this experiment is intended to resolve.",
                ),
                required(
                    "intervention",
                    "What is deliberately varied and why that variation addresses the question.",
                ),
                required(
                    "control",
                    "The baseline or comparison used to interpret the intervention, including what is held constant.",
                ),
                recommended(
                    "prediction",
                    "The expected result before observing the experiment's outcome.",
                ),
                recommended(
                    "confounders",
                    "Other factors that could explain a difference and how they are controlled or left unresolved.",
                ),
                recommended(
                    "result",
                    "Observed outcomes, including null or inconclusive findings; distinguish them from the prediction.",
                ),
            ],
        ),
        change_type(
            "eval",
            "Change how performance or behavior is measured.",
            vec![
                required(
                    "target",
                    "The behavior or capability the evaluation is intended to measure.",
                ),
                required(
                    "protocol",
                    "The evaluation conditions and comparison rules that determine what a score means.",
                ),
                required(
                    "metrics",
                    "The chosen measurements and how their values should be interpreted.",
                ),
                recommended(
                    "rationale",
                    "Why this protocol and these metrics answer the research question better than alternatives.",
                ),
                recommended(
                    "limitations",
                    "Behaviors or populations this evaluation cannot support conclusions about.",
                ),
            ],
        ),
        change_type(
            "analysis",
            "Interpret evidence or record diagnostic findings.",
            vec![
                required(
                    "evidence",
                    "The observations or artifacts supporting this analysis and where they can be inspected.",
                ),
                required(
                    "finding",
                    "What the evidence shows, separated from an explanation of why it occurred.",
                ),
                required(
                    "interpretation",
                    "The explanation inferred from the finding and plausible alternatives still consistent with the evidence.",
                ),
                recommended(
                    "confidence",
                    "How strongly the evidence supports the interpretation and the reasons for uncertainty.",
                ),
                recommended(
                    "next-question",
                    "The unresolved question this finding makes useful to investigate next.",
                ),
            ],
        ),
        change_type(
            "reproduce",
            "Attempt to reproduce or replicate an existing result.",
            vec![
                required(
                    "source",
                    "The paper, artifact, or prior experiment whose result is being reproduced.",
                ),
                required(
                    "target-result",
                    "The specific reported result and conditions used as the reproduction target.",
                ),
                recommended(
                    "deviations",
                    "Known differences from the source procedure that may affect comparability.",
                ),
                required(
                    "result",
                    "The observed reproduction outcome, including failed or inconclusive attempts; distinguish it from the target result.",
                ),
                recommended(
                    "discrepancy",
                    "Differences from the target and supported explanations or remaining uncertainties.",
                ),
            ],
        ),
        change_type(
            "fix",
            "Correct an implementation or experimental defect.",
            vec![
                required(
                    "symptom",
                    "The observed failure or inconsistency that exposed the defect.",
                ),
                required(
                    "cause",
                    "The causal explanation established while investigating the defect.",
                ),
                recommended(
                    "affected-results",
                    "Prior experiments, measurements, or conclusions invalidated or put in doubt by this defect.",
                ),
                required(
                    "decision",
                    "Why this correction addresses the cause and was chosen over other interventions.",
                ),
                recommended(
                    "validation",
                    "Checks and reruns performed after the correction, including results not yet revalidated.",
                ),
            ],
        ),
        change_type(
            "infra",
            "Change execution machinery without changing the intended experiment.",
            vec![
                required(
                    "objective",
                    "The execution or development constraint motivating the infrastructure change.",
                ),
                required(
                    "change",
                    "The infrastructure intervention and why it addresses that constraint.",
                ),
                recommended(
                    "experimental-impact",
                    "Whether execution changes may affect the intended scientific comparison or observed behavior.",
                ),
                recommended(
                    "reproducibility-impact",
                    "Effects on repeatability, determinism, environment recovery, or comparability with prior runs.",
                ),
                recommended(
                    "validation",
                    "Checks performed to establish operational behavior and experimental equivalence, including remaining gaps.",
                ),
            ],
        ),
        change_type(
            "docs",
            "Change the research knowledge surface.",
            vec![
                required(
                    "intent",
                    "The research knowledge or interpretation that needs to be preserved or corrected.",
                ),
                required(
                    "decision",
                    "Why this explanation, organization, or level of detail was selected.",
                ),
                recommended(
                    "audience",
                    "The intended readers and the background or assumptions the documentation expects.",
                ),
                recommended(
                    "validation",
                    "Sources, examples, or procedures checked for accuracy and any unresolved documentation claims.",
                ),
            ],
        ),
    ]
});

pub(crate) fn ml_research() -> BuiltInBundle {
    bundle(
        "ml-research",
        "Change categories for empirical machine-learning research.",
        "Durable hypotheses, experimental decisions, and evidence for ML research.",
        "The ML Research taxonomy with its default durable-property typeset.",
        &DEFINITIONS,
    )
}
