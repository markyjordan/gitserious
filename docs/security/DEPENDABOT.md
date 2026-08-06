# Dependabot Security Process

This document is the source of truth for dependency-vulnerability intake and
remediation in `gitserious`. Dependabot detects actionable advisories and may
propose patches. It is not the risk owner, reviewer, merge authority,
disclosure channel, or incident commander.

## Platform Posture

Keep `dev` as the GitHub default branch. Dependabot security-update pull
requests target the default branch, which puts them into the same reviewed
integration lane as other changes. There is no permanent `security` branch and
no direct-to-`main` bot exception.

The repository must keep these GitHub features enabled:

- dependency graph;
- Dependabot alerts and alert notifications;
- Dependabot security updates; and
- private vulnerability reporting.

The committed `.github/dependabot.yml` monitors Cargo and GitHub Actions while
setting `open-pull-requests-limit: 0`. This disables scheduled version-update
pull requests without disabling advisory-triggered security updates. The
weekly schedule is required configuration, not the security trigger. Do not
add `target-branch`: security updates use the repository default branch, and a
custom target would apply only to version updates.

Initially, keep grouped security updates and Dependabot auto-merge disabled.
One remediation per dependency/advisory set is easier to attribute, test,
revert, and assess for release impact. Every remediation requires human
review.

## Advisory Intake And Triage

Use this process for a Dependabot alert, a GHSA or CVE learned from another
trusted source, or a Dependabot security-update pull request.

Record:

- the alert number, GHSA, and CVE when present;
- the affected package or GitHub Action;
- the resolved version or commit SHA and vulnerable range;
- the first patched version or secure SHA;
- whether the dependency is direct or transitive, runtime or development-only;
- severity, publication date, and reachability; and
- every supported release line that may be affected.

For a Cargo dependency, inspect the committed graph with:

```console
cargo tree -i <crate> --locked
```

An alert is actionable when the repository resolves an affected version and
the vulnerable behavior is reachable, plausibly reachable, executed during a
build, or not yet disproven. Do not dismiss an alert only because the
dependency is transitive. Uncertainty defaults to remediation rather than
dismissal.

Use these response targets:

| Condition | Triage target | Remediation target |
| --- | --- | --- |
| Known exploitation, malware, leaked credentials, or a critical reachable flaw | Same day | Mitigate immediately and release as soon as the fix is safely testable |
| High severity or a network/credential boundary is affected | 1 business day | 3 business days |
| Moderate severity with a patch | 3 business days | 14 days |
| Low severity or demonstrably unreachable | 7 days | 30 days or a documented exception review |

Dismiss a non-actionable alert only with the concrete technical reason,
supporting evidence, responsible maintainer, and review date. Reopen it when
reachability, enabled features, supported platforms, or advisory facts change.
Do not use blanket auto-dismiss rules during initial adoption.

## Remediation Pull Requests

When Dependabot proposes a patch:

1. Verify that `dependabot[bot]` authored the pull request, it targets `dev`, it
   links the advisory, and its manifest, lockfile, or Action-reference change is
   the smallest credible delta.
2. Review the minimum patched version, upstream release notes, lockfile delta,
   build scripts, proc macros, native code, feature changes, new transitives,
   and compatibility impact.
3. Keep the Dependabot pull request when its patch is safe and sufficient. A
   human owns approval and the merge decision even when the bot owns the branch.
4. If the bot proposal is absent, incomplete, or too broad, branch
   `<author>/security/GHSA-or-CVE-short-slug` from current `dev`, implement the
   smallest safe remediation, and link the alert and any superseded bot pull
   request.
5. Run every ordinary `dev` pull-request check. Once the CI security phase is
   implemented, also require the dependency-security check and exact-current-
   head trusted approval when automation changes.
6. Squash merge the human-approved remediation into `dev`, confirm the fixed
   graph and alert state there, then promote green `dev` to `main` through the
   ordinary reviewed path.

Security urgency changes response time, not branch ownership, test depth, or
the requirement for human approval. Dependabot must never approve itself,
merge itself, or satisfy a trusted-automation approval requirement.

## Release-Line Impact And Backports

Before closing an advisory, compare every supported `release/X.Y` lockfile and
artifact dependency set with the vulnerable range.

- If no published line is affected, record that evidence and complete normal
  `dev` to `main` promotion.
- If a published line is affected, create
  `hotfix/GHSA-or-CVE-short-slug` from that release line, apply the smallest
  compatible fix, and use the full release pull-request lane.
- Publish a new patch version. Never move or replace an existing tag.
- Yank, withdraw, or mark an artifact affected only when the advisory and user
  impact justify it, and link the superseding patch release.
- Propagate an equivalent fix to `main` and `dev`; do not leave the branches
  permanently divergent.

## No Patch, Manual Advisories, And Confidential Reports

If no patched version exists, prefer removal, feature disablement,
replacement, sandboxing, or another concrete mitigation. Keep the alert open
unless a reviewed exception identifies the exact advisory, explains the
accepted exposure, names an owner, and sets an expiry or review date.

For a credible advisory that GitHub did not detect, use the same human-owned
security branch and evidence record. A missing Dependabot alert is not proof of
safety.

GitHub Actions pinned to full commit SHAs retain execution integrity, but
GitHub's dependency alerts do not cover SHA-only Action references. When an
Action GHSA or CVE becomes known, manually replace the pin with a reviewed
secure SHA and require exact-current-head trusted approval after that gate is
implemented.

Handle private vulnerability reports through GitHub's confidential reporting
channel. Do not create a public issue, Dependabot pull request, topic pull
request, or changelog entry until coordinated disclosure makes that safe. Do
not design Dependabot workflows around privileged secrets; Dependabot-triggered
workflows receive restricted credentials.

## Closure Evidence

Close the response only when all applicable evidence exists:

1. The alert, GHSA, or CVE and affected scope were recorded.
2. The remediation or time-bounded exception was human-reviewed.
3. All available branch and dependency-security checks passed.
4. The fixed graph reached `dev` and `main`.
5. Every affected release line received a patch or an explicit disposition.
6. The GitHub alert resolved, or the no-patch exception has an owner and review
   date.
7. Public notes identify affected and fixed versions once disclosure is safe.

## CI Security Handoff

The later `gitserious-CI` phase owns enforcement. It should add a separate
`dependency-security.yml` that:

- reviews dependency deltas and runs `cargo audit` against the committed
  `Cargo.lock` for dependency-changing pull requests;
- reruns the advisory scan on relevant protected-branch pushes;
- runs daily and through manual dispatch so new advisories are visible without
  a source change;
- uses read-only permissions, pinned tools and Actions, and no ordinary
  repository secrets on Dependabot events;
- exposes a stable required-check name distinct from formatting, linting,
  checking, and tests; and
- fails closed for a known advisory unless a reviewed, time-bounded exception
  matches that exact advisory.

That workflow is deliberately out of scope for this security-only transfer.

## References

- [GitHub: Configuring Dependabot security updates](https://docs.github.com/en/code-security/how-tos/secure-your-supply-chain/secure-your-dependencies/configure-security-updates)
- [GitHub: Dependabot options reference](https://docs.github.com/en/code-security/reference/supply-chain-security/dependabot-options-reference)
- [GitHub: Dependabot pull requests](https://docs.github.com/en/enterprise-cloud@latest/code-security/concepts/supply-chain-security/dependabot-pull-requests)
- [GitHub: Dependabot alerts](https://docs.github.com/en/code-security/concepts/supply-chain-security/dependabot-alerts)
- [GitHub: Secure use of GitHub Actions](https://docs.github.com/en/actions/reference/security/secure-use)
- [RustSec Advisory Database and `cargo-audit`](https://github.com/rustsec/rustsec/tree/main/cargo-audit)
