/// A commit message of one built-in type: what a filled-in message of that type looks like.
#[derive(Debug, Eq, PartialEq)]
pub struct CommitSample {
    subject: &'static str,
    values: &'static [(&'static str, &'static str)],
}

impl CommitSample {
    /// The subject line, with its type and scope.
    #[must_use]
    pub const fn subject(&self) -> &'static str {
        self.subject
    }

    /// The sample value of the property `key`.
    #[must_use]
    pub fn value(&self, key: &str) -> Option<&'static str> {
        self.values
            .iter()
            .find(|(candidate, _)| *candidate == key)
            .map(|(_, value)| *value)
    }
}

/// The sample message of the built-in type `commit_type` in the built-in taxonomy `taxonomy`.
#[must_use]
pub fn built_in_sample(taxonomy: &str, commit_type: &str) -> Option<&'static CommitSample> {
    SAMPLES
        .iter()
        .find(|(owner, id, _)| *owner == taxonomy && *id == commit_type)
        .map(|(_, _, sample)| sample)
}

const fn sample(
    subject: &'static str,
    values: &'static [(&'static str, &'static str)],
) -> CommitSample {
    CommitSample { subject, values }
}

const SAMPLES: &[(&str, &str, CommitSample)] = &[
    (
        "conventional",
        "feat",
        sample(
            "feat(api): rate-limit public endpoints",
            &[
                (
                    "intent",
                    "Unauthenticated clients could exhaust the search workers.",
                ),
                (
                    "decision",
                    "A token bucket per client key; a global limit would punish every client for one.",
                ),
                (
                    "constraints",
                    "Partner keys keep their contracted 1000 rps.",
                ),
            ],
        ),
    ),
    (
        "conventional",
        "fix",
        sample(
            "fix(billing): stop double charge on retry",
            &[
                (
                    "cause",
                    "A retried request reused no idempotency key, so the processor charged twice.",
                ),
                (
                    "decision",
                    "Key each charge by order id at the client; deduplicating later loses the receipt.",
                ),
                ("constraints", "Existing receipts keep their ids."),
            ],
        ),
    ),
    (
        "conventional",
        "refactor",
        sample(
            "refactor(parser): split tokenizing from parsing",
            &[
                (
                    "motivation",
                    "Every grammar change touched the tokenizer too.",
                ),
                (
                    "decision",
                    "A token stream between two passes, not a parser combinator rewrite.",
                ),
                ("invariant", "The same input yields the same syntax tree."),
            ],
        ),
    ),
    (
        "conventional",
        "perf",
        sample(
            "perf(search): cache compiled queries",
            &[
                ("bottleneck", "Query compilation took 40% of request time."),
                ("decision", "An LRU cache keyed by normalized query text."),
                ("result", "p99 latency fell from 180 ms to 95 ms."),
                ("tradeoff", "Up to 64 MB more memory per worker."),
            ],
        ),
    ),
    (
        "conventional",
        "test",
        sample(
            "test(auth): cover expired refresh tokens",
            &[
                ("risk", "An expired refresh token could mint a new session."),
                (
                    "rationale",
                    "The token expires one second before the check, the boundary that failed.",
                ),
            ],
        ),
    ),
    (
        "conventional",
        "docs",
        sample(
            "docs(readme): explain the release steps",
            &[(
                "reason",
                "The readme still described the manual upload that CI replaced.",
            )],
        ),
    ),
    (
        "conventional",
        "chore",
        sample(
            "chore(deps): update the TLS library",
            &[
                (
                    "reason",
                    "The old version reaches end of support next month.",
                ),
                ("impact", "Builds now need OpenSSL 3."),
            ],
        ),
    ),
    (
        "conventional",
        "build",
        sample(
            "build(release): link the binary statically",
            &[
                (
                    "intent",
                    "Users on older distributions could not run the release.",
                ),
                ("constraint", "Their glibc is older than the build host's."),
                ("impact", "The binary grows by 2 MB."),
            ],
        ),
    ),
    (
        "conventional",
        "ci",
        sample(
            "ci(checks): run lints before tests",
            &[
                ("intent", "Fail fast on cheap checks."),
                (
                    "decision",
                    "Lints gate the test job instead of running beside it.",
                ),
                ("failure-semantics", "A lint failure now blocks the merge."),
            ],
        ),
    ),
    (
        "conventional",
        "style",
        sample(
            "style(core): format with the new rustfmt edition",
            &[(
                "reason",
                "One formatting commit keeps later blame readable.",
            )],
        ),
    ),
    (
        "conventional",
        "revert",
        sample(
            "revert(api): restore the v1 pagination",
            &[
                (
                    "reason",
                    "Cursor pagination broke two partner integrations.",
                ),
                ("impact", "Page links use offsets again."),
                ("follow-up", "Reland behind a version header next sprint."),
            ],
        ),
    ),
    (
        "research",
        "hypothesis",
        sample(
            "hypothesis(retrieval): reranking helps long queries",
            &[
                (
                    "claim",
                    "A cross-encoder reranker raises recall on queries over 20 tokens.",
                ),
                (
                    "motivation",
                    "Long queries make up most of the failed searches.",
                ),
                (
                    "prediction",
                    "Recall@10 on long queries rises by at least 5 points.",
                ),
                ("falsifier", "No gain, or a gain only on short queries."),
                (
                    "assumptions",
                    "The eval set reflects production query lengths.",
                ),
            ],
        ),
    ),
    (
        "research",
        "data",
        sample(
            "data(corpus): drop near-duplicate documents",
            &[
                (
                    "objective",
                    "Stop duplicates from inflating retrieval scores.",
                ),
                (
                    "population",
                    "The 2024 support-ticket corpus, 1.2 M documents.",
                ),
                (
                    "transformation",
                    "MinHash at 0.9 similarity; keep the oldest copy.",
                ),
                ("assumptions", "Near-duplicates carry no extra signal."),
                (
                    "leakage-risk",
                    "Duplicates had crossed the train and test split.",
                ),
                ("validation", "Spot-checked 200 removed pairs by hand."),
            ],
        ),
    ),
    (
        "research",
        "model",
        sample(
            "model(encoder): add a projection head",
            &[
                (
                    "objective",
                    "Separate the retrieval space from the pretraining space.",
                ),
                ("change", "A two-layer MLP on top of the pooled embedding."),
                (
                    "rationale",
                    "Contrastive results favor a head over fine-tuning the encoder.",
                ),
                (
                    "assumptions",
                    "The pooled embedding already holds the signal.",
                ),
                (
                    "tradeoffs",
                    "Adds 2 M parameters and a second learning rate.",
                ),
            ],
        ),
    ),
    (
        "research",
        "experiment",
        sample(
            "experiment(lr): sweep the warmup length",
            &[
                ("question", "Does a longer warmup stabilize early training?"),
                ("intervention", "Warmup of 500, 2000, and 8000 steps."),
                (
                    "control",
                    "The current 1000-step warmup with the same seeds.",
                ),
                ("prediction", "Loss spikes vanish at 2000 steps or more."),
                ("confounders", "Batch size changed in the same week."),
                (
                    "result",
                    "Spikes vanish at 2000 steps; final loss unchanged.",
                ),
            ],
        ),
    ),
    (
        "research",
        "eval",
        sample(
            "eval(qa): score answers against references",
            &[
                ("target", "Answer quality on the support QA set."),
                (
                    "protocol",
                    "Exact match plus a judged score on 500 held-out questions.",
                ),
                ("metrics", "Exact match and mean judge score."),
                ("rationale", "Exact match alone misses paraphrased answers."),
                ("limitations", "The judge favors longer answers."),
            ],
        ),
    ),
    (
        "research",
        "analysis",
        sample(
            "analysis(errors): group failures by query type",
            &[
                ("evidence", "800 failed queries from the last eval run."),
                ("finding", "62% of failures are multi-hop questions."),
                (
                    "interpretation",
                    "The retriever finds one hop but not the second.",
                ),
                ("confidence", "Moderate; labels came from one annotator."),
                ("next-question", "Does iterative retrieval close the gap?"),
            ],
        ),
    ),
    (
        "research",
        "reproduce",
        sample(
            "reproduce(baseline): rerun the published BM25 numbers",
            &[
                ("source", "Table 2 of the benchmark paper."),
                ("target-result", "MRR@10 of 0.187."),
                (
                    "deviations",
                    "A newer tokenizer; the original is unavailable.",
                ),
                ("result", "MRR@10 of 0.184."),
                ("discrepancy", "Within the tokenizer's expected effect."),
            ],
        ),
    ),
    (
        "research",
        "fix",
        sample(
            "fix(eval): shuffle before splitting",
            &[
                ("symptom", "Test accuracy beat train accuracy."),
                ("cause", "The split took the newest rows as the test set."),
                ("affected-results", "Every eval since the March run."),
                (
                    "decision",
                    "Shuffle with a fixed seed instead of splitting by date.",
                ),
                ("validation", "Train and test accuracy now track."),
            ],
        ),
    ),
    (
        "research",
        "infra",
        sample(
            "infra(cluster): pin the CUDA version",
            &[
                ("objective", "Make runs repeatable across nodes."),
                ("change", "Every job image pins CUDA 12.4."),
                ("experimental-impact", "None expected; kernels match."),
                (
                    "reproducibility-impact",
                    "Reruns no longer drift with node upgrades.",
                ),
                (
                    "validation",
                    "The same seed gives the same loss on two nodes.",
                ),
            ],
        ),
    ),
    (
        "research",
        "docs",
        sample(
            "docs(protocol): write down the eval procedure",
            &[
                ("intent", "New members ran the eval differently."),
                (
                    "decision",
                    "One checked-in protocol instead of notebook comments.",
                ),
                ("audience", "Anyone reporting a number."),
                (
                    "validation",
                    "Two members reproduced the last score from it.",
                ),
            ],
        ),
    ),
    (
        "infra-ops",
        "provision",
        sample(
            "provision(db): add a read replica in eu-west",
            &[
                ("purpose", "Serve European reads locally."),
                ("topology", "One async replica of the primary in us-east."),
                ("capacity-assumption", "European reads stay under 2000 qps."),
                (
                    "failure-domain",
                    "A separate availability zone from the app tier.",
                ),
                ("dependencies", "The primary's replication slot."),
                (
                    "rollback",
                    "Point reads back at the primary and drop the replica.",
                ),
            ],
        ),
    ),
    (
        "infra-ops",
        "configure",
        sample(
            "configure(nginx): raise the upload limit",
            &[
                ("objective", "Let users attach video files."),
                ("change", "client_max_body_size from 10 MB to 200 MB."),
                (
                    "rationale",
                    "Uploads stream to storage, so memory stays flat.",
                ),
                ("assumptions", "Uploads over 200 MB are rare."),
                ("rollback", "Restore the 10 MB limit."),
                ("validation", "A 150 MB upload succeeds in staging."),
            ],
        ),
    ),
    (
        "infra-ops",
        "deploy",
        sample(
            "deploy(api): roll out v2.4.0 to production",
            &[
                ("objective", "Ship the rate limiter to all users."),
                ("artifact", "api image v2.4.0, digest sha256:4be1."),
                ("environment", "Production, us-east and eu-west."),
                ("strategy", "Canary at 5% for an hour, then all hosts."),
                ("risk", "Limits may be too tight for batch clients."),
                ("rollback", "Redeploy v2.3.2; no schema change."),
                ("validation", "Error rate flat through the canary."),
            ],
        ),
    ),
    (
        "infra-ops",
        "migrate",
        sample(
            "migrate(storage): move uploads to object storage",
            &[
                ("from-state", "Uploads on an NFS volume."),
                ("to-state", "Uploads in a versioned bucket."),
                ("reason", "The volume is 90% full and cannot grow."),
                ("compatibility", "Old URLs redirect to the bucket."),
                ("invariants", "No upload is lost or changes its URL."),
                ("rollback", "Keep the volume read-only for 30 days."),
                ("validation", "Checksums match for every copied file."),
            ],
        ),
    ),
    (
        "infra-ops",
        "scale",
        sample(
            "scale(workers): add four queue workers",
            &[
                ("constraint", "The queue backs up at peak."),
                ("signal", "Queue depth over 10000 for an hour daily."),
                ("change", "Workers from 8 to 12."),
                (
                    "capacity-assumption",
                    "Each worker clears 300 jobs a minute.",
                ),
                ("tradeoff", "About $400 more a month."),
                ("validation", "Peak queue depth stays under 1000."),
            ],
        ),
    ),
    (
        "infra-ops",
        "observe",
        sample(
            "observe(api): alert on rising 5xx rates",
            &[
                ("blind-spot", "A partial outage went unnoticed for an hour."),
                ("signal", "The 5xx share of requests per route."),
                ("interpretation", "Over 2% means a route is failing."),
                ("threshold", "2% for 5 minutes."),
                ("response", "Page the on-call engineer."),
                ("cost", "One more metric series per route."),
            ],
        ),
    ),
    (
        "infra-ops",
        "incident",
        sample(
            "incident(dns): record the resolver outage",
            &[
                ("symptom", "Requests to internal services timed out."),
                ("impact", "Checkout failed for 40 minutes."),
                ("trigger", "A resolver config push."),
                ("cause", "The push dropped the internal zone."),
                ("mitigation", "Rolled the config back."),
                ("follow-up", "Validate zones before each push."),
            ],
        ),
    ),
    (
        "infra-ops",
        "recover",
        sample(
            "recover(db): restore orders from backup",
            &[
                (
                    "failure-state",
                    "A bad migration truncated the orders table.",
                ),
                ("target-state", "Orders as of 02:00 UTC."),
                ("action", "Point-in-time restore into a new instance."),
                ("data-loss", "Orders placed between 02:00 and 02:10."),
                ("residual-risk", "Those orders need manual reentry."),
                ("validation", "Row counts match the 02:00 snapshot."),
            ],
        ),
    ),
    (
        "infra-ops",
        "secure",
        sample(
            "secure(api): require TLS 1.2 or later",
            &[
                ("threat", "Downgrade attacks on old TLS versions."),
                ("exposure", "Every public endpoint."),
                ("control", "The load balancer refuses older protocols."),
                ("assumptions", "No supported client needs TLS 1.1."),
                ("residual-risk", "Clients pinned to old libraries break."),
                ("validation", "An SSL scan reports no TLS 1.0 or 1.1."),
            ],
        ),
    ),
    (
        "infra-ops",
        "decommission",
        sample(
            "decommission(legacy): shut down the old reporting host",
            &[
                ("target", "The reporting VM in the old data center."),
                ("reason", "Reports moved to the warehouse in May."),
                ("dependencies", "One cron job that mailed weekly totals."),
                ("migration", "The cron job now runs in the scheduler."),
                ("residual-state", "A 30-day snapshot of its disk."),
                ("validation", "No traffic reached it for two weeks."),
            ],
        ),
    ),
];

#[cfg(test)]
mod tests {
    use super::built_in_sample;
    use crate::built_in_taxonomies;

    #[test]
    fn every_built_in_type_has_a_sample_with_a_value_for_each_of_its_keys() {
        for taxonomy in built_in_taxonomies() {
            for commit_type in taxonomy.commit_types() {
                let id = commit_type.id().as_str();
                let sample = built_in_sample(taxonomy.id().as_str(), id);
                assert!(sample.is_some(), "{}/{id} has no sample", taxonomy.id());
                let Some(sample) = sample else { continue };
                assert!(
                    sample.subject().starts_with(&format!("{id}(")),
                    "{id}: {}",
                    sample.subject()
                );
                let keys = commit_type
                    .properties()
                    .iter()
                    .map(|property| property.key().as_str())
                    .collect::<Vec<_>>();
                let sampled = sample
                    .values
                    .iter()
                    .map(|(key, _)| *key)
                    .collect::<Vec<_>>();
                assert_eq!(sampled, keys, "{}/{id}", taxonomy.id());
            }
        }
    }
}
