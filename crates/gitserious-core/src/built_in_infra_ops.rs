use std::sync::LazyLock;

use crate::CommitTypeDefinition;
use crate::built_in_domain::{BuiltInBundle, bundle, change_type, recommended, required};

static DEFINITIONS: LazyLock<Vec<CommitTypeDefinition>> = LazyLock::new(|| {
    vec![
        change_type(
            "provision",
            "Introduce operational resources or systems.",
            vec![
                required(
                    "purpose",
                    "The operational need that justifies introducing this resource or system.",
                ),
                required(
                    "topology",
                    "The chosen placement and relationships, including why this arrangement fits the operational need.",
                ),
                recommended(
                    "capacity-assumption",
                    "The demand or growth assumptions used to size the new resource.",
                ),
                recommended(
                    "failure-domain",
                    "Failures the resource must tolerate and failures shared with adjacent systems.",
                ),
                recommended(
                    "dependencies",
                    "External services, ownership, or prerequisites the resource depends on.",
                ),
                recommended(
                    "rollback",
                    "How to undo provisioning and what state, cost, or dependencies would remain.",
                ),
            ],
        ),
        change_type(
            "configure",
            "Change operational configuration.",
            vec![
                required(
                    "objective",
                    "The operational behavior the configuration change is intended to achieve.",
                ),
                required(
                    "change",
                    "The configuration intervention and the behavior it is expected to alter.",
                ),
                required(
                    "rationale",
                    "Why these settings were selected over plausible alternatives.",
                ),
                recommended(
                    "assumptions",
                    "Environmental or workload assumptions on which the settings depend.",
                ),
                recommended(
                    "rollback",
                    "How to restore the prior configuration and any limits on doing so.",
                ),
                recommended(
                    "validation",
                    "Checks performed and their observed outcomes, including behavior not yet verified.",
                ),
            ],
        ),
        change_type(
            "deploy",
            "Roll an artifact into an operational environment.",
            vec![
                required(
                    "objective",
                    "The operational or user outcome motivating this deployment.",
                ),
                required(
                    "artifact",
                    "The exact artifact or version being deployed and where it can be identified.",
                ),
                required(
                    "environment",
                    "The target environment and conditions relevant to interpreting deployment behavior.",
                ),
                required(
                    "strategy",
                    "The rollout approach, why it was chosen, and how progression is decided.",
                ),
                recommended(
                    "risk",
                    "Known failure modes, likely impact, and unresolved deployment uncertainty.",
                ),
                recommended(
                    "rollback",
                    "The recovery path if deployment fails, including data or compatibility limits.",
                ),
                recommended(
                    "validation",
                    "Observed rollout checks and acceptance signals, distinguished from planned checks.",
                ),
            ],
        ),
        change_type(
            "migrate",
            "Move between operational states.",
            vec![
                required(
                    "from-state",
                    "The relevant source state and assumptions about its contents or behavior.",
                ),
                required(
                    "to-state",
                    "The intended destination state and the conditions that define completion.",
                ),
                required("reason", "Why the state transition is necessary now."),
                recommended(
                    "compatibility",
                    "How old and new consumers or states coexist, including unsupported transitions.",
                ),
                recommended(
                    "invariants",
                    "Data or behavior that must remain true throughout the transition.",
                ),
                recommended(
                    "rollback",
                    "How to recover from an incomplete or incorrect migration and which steps cannot be reversed.",
                ),
                recommended(
                    "validation",
                    "Checks performed on the transition and destination state, with observed outcomes and gaps.",
                ),
            ],
        ),
        change_type(
            "scale",
            "Adjust capacity in response to a constraint or signal.",
            vec![
                required(
                    "constraint",
                    "The resource or service constraint that makes a capacity change necessary.",
                ),
                required(
                    "signal",
                    "The observed demand, saturation, or service signal used to justify scaling.",
                ),
                required(
                    "change",
                    "The chosen capacity adjustment and why it addresses the observed constraint.",
                ),
                recommended(
                    "capacity-assumption",
                    "The workload and headroom assumptions supporting the new capacity.",
                ),
                recommended(
                    "tradeoff",
                    "Cost, complexity, efficiency, or reliability consequences accepted with this adjustment.",
                ),
                recommended(
                    "validation",
                    "Observed capacity and service behavior after the change, including assumptions not yet tested.",
                ),
            ],
        ),
        change_type(
            "observe",
            "Address a monitoring or diagnostic blind spot.",
            vec![
                required(
                    "blind-spot",
                    "The operational uncertainty that existing observations cannot resolve.",
                ),
                required(
                    "signal",
                    "The observation being added or changed and the behavior it is intended to reveal.",
                ),
                required(
                    "interpretation",
                    "What the signal means and alternative explanations operators must consider.",
                ),
                recommended(
                    "threshold",
                    "The decision boundary and why it separates actionable behavior from expected variation.",
                ),
                recommended(
                    "response",
                    "The operator or automated response the observation is intended to inform.",
                ),
                recommended(
                    "cost",
                    "Collection, storage, noise, or operational costs accepted for this visibility.",
                ),
            ],
        ),
        change_type(
            "incident",
            "Record operational impact and mitigation.",
            vec![
                required(
                    "symptom",
                    "The observed operational failure, including how and when it was detected.",
                ),
                required(
                    "impact",
                    "Affected users, services, or data and the known extent of disruption.",
                ),
                recommended(
                    "trigger",
                    "The event associated with the incident's onset, distinguished from its underlying cause.",
                ),
                recommended(
                    "cause",
                    "The supported causal explanation and what remains uncertain about it.",
                ),
                required(
                    "mitigation",
                    "Actions taken to limit impact and their observed effect, including incomplete mitigation.",
                ),
                recommended(
                    "follow-up",
                    "Remaining recovery, investigation, or prevention work and the reasons it is needed.",
                ),
            ],
        ),
        change_type(
            "recover",
            "Restore service or data from a failed state.",
            vec![
                required(
                    "failure-state",
                    "The starting failure condition and the known service or data damage.",
                ),
                required(
                    "target-state",
                    "The recovery target and the conditions that define an acceptable restored state.",
                ),
                required(
                    "action",
                    "The recovery intervention and why it was selected given the failure state.",
                ),
                recommended(
                    "data-loss",
                    "Known or possible data loss and the evidence or uncertainty behind that assessment.",
                ),
                recommended(
                    "residual-risk",
                    "Failure modes or degraded conditions that may remain after recovery.",
                ),
                recommended(
                    "validation",
                    "Checks performed on restored service or data and what has not yet been verified.",
                ),
            ],
        ),
        change_type(
            "secure",
            "Address an operational threat or exposure.",
            vec![
                required(
                    "threat",
                    "The threat scenario this change is intended to address.",
                ),
                required(
                    "exposure",
                    "The assets, access paths, or conditions that make the threat relevant.",
                ),
                required(
                    "control",
                    "The chosen protective measure and why it addresses the identified exposure.",
                ),
                recommended(
                    "assumptions",
                    "Trust, environment, or attacker assumptions on which the control depends.",
                ),
                recommended(
                    "residual-risk",
                    "Threats or exposure that remain outside the control's protection.",
                ),
                recommended(
                    "validation",
                    "Checks performed on the control and their observed results; do not treat the description itself as proof of protection.",
                ),
            ],
        ),
        change_type(
            "decommission",
            "Retire an operational resource or system.",
            vec![
                required(
                    "target",
                    "The resource or system being retired and the boundary of its removal.",
                ),
                required(
                    "reason",
                    "Why the resource is no longer needed or should be replaced.",
                ),
                recommended(
                    "dependencies",
                    "Consumers or prerequisites affected by retirement and how they were accounted for.",
                ),
                recommended(
                    "migration",
                    "Where remaining users, data, or responsibilities move and what is not yet transferred.",
                ),
                recommended(
                    "residual-state",
                    "Data, access, resources, or costs that remain after retirement.",
                ),
                recommended(
                    "validation",
                    "Checks performed to confirm retirement and continued operation of affected consumers.",
                ),
            ],
        ),
    ]
});

pub(crate) fn infra_ops() -> BuiltInBundle {
    bundle(
        "infra-ops",
        "Change categories for infrastructure and operational systems.",
        "Durable operational decisions, assumptions, and recovery context.",
        "The Infra Ops taxonomy with its default durable-property typeset.",
        &DEFINITIONS,
    )
}
