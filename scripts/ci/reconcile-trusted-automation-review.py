#!/usr/bin/env python3
"""Reconcile approval evidence from protected policy and current GitHub metadata."""
from __future__ import annotations

import dataclasses
import hashlib
import json
import os
import pathlib
import re
import subprocess
import sys
import tempfile

CONTEXT = "trusted-automation-review"
SHA = re.compile(r"[0-9a-f]{40}")
ROOT = pathlib.Path(__file__).resolve().parents[2]


class ReviewError(Exception):
    pass


def run(arguments, *, env=None, check=True):
    result = subprocess.run(arguments, cwd=ROOT, env=env, text=True, capture_output=True)
    if check and result.returncode:
        raise ReviewError(result.stderr.strip() or result.stdout.strip() or str(arguments[0]))
    return result


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


@dataclasses.dataclass(frozen=True)
class Member:
    number: int
    head: str
    author: str
    base: str
    head_ref: str
    stack: int | None
    paths: tuple
    reviews: tuple
    comments: tuple


@dataclasses.dataclass(frozen=True)
class Standalone:
    member: Member

    @property
    def members(self):
        return (self.member,)


@dataclasses.dataclass(frozen=True)
class NativeStack:
    number: int
    base: str
    members: tuple

    @property
    def top(self):
        return self.members[-1]


@dataclasses.dataclass(frozen=True)
class Snapshot:
    policy: str
    permissions: tuple
    scopes: tuple


class Coordinator:
    def __init__(self, repository):
        if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository):
            raise ReviewError("Invalid repository")
        self.repository = repository
        self.known_heads = set()
        self.policy = run(["git", "rev-parse", "HEAD"]).stdout.strip()
        self.target = os.environ.get("TRUST_STATUS_TARGET_URL", "")

    def api(self, path, *, pages=False):
        arguments = ["gh", "api", "-H", "X-GitHub-Api-Version: 2026-03-10"]
        if pages:
            arguments.extend(["--paginate", "--slurp"])
        arguments.append(f"repos/{self.repository}/{path}")
        try:
            data = json.loads(run(arguments).stdout)
        except (ValueError, TypeError) as error:
            raise ReviewError(f"Invalid GitHub response for {path}") from error
        if pages:
            if not isinstance(data, list) or any(not isinstance(page, list) for page in data):
                raise ReviewError(f"Invalid paginated GitHub response for {path}")
            return [item for page in data for item in page]
        return data

    @staticmethod
    def protected(base):
        return base in ("dev", "main") or base.startswith("release/")

    def member(self, number):
        data = self.api(f"pulls/{number}")
        head = data["head"]["sha"]
        if not SHA.fullmatch(head):
            raise ReviewError("Invalid pull request head")
        self.known_heads.add(head)
        if data["state"] != "open" or data["number"] != number:
            raise ReviewError("Pull request changed during discovery")
        paths = self.api(f"pulls/{number}/files?per_page=100", pages=True)
        if len(paths) != data["changed_files"]:
            raise ReviewError(f"Incomplete changed-path response for #{number}")
        reviews = self.api(f"pulls/{number}/reviews?per_page=100", pages=True)
        comments = self.api(f"issues/{number}/comments?per_page=100", pages=True)
        return Member(number, head, data["user"]["login"], data["base"]["ref"],
                      data["head"]["ref"], (data.get("stack") or {}).get("number"),
                      tuple(sorted({name for item in paths for name in
                                    (item["filename"], item.get("previous_filename")) if name})),
                      tuple((r["user"]["login"], r["author_association"], r["state"],
                             r["submitted_at"], r["commit_id"]) for r in reviews if r["commit_id"] == head),
                      tuple((c["user"]["login"], c["author_association"], c["body"],
                             c["updated_at"], c["id"]) for c in comments if c["body"] == f"/approve-automation {head}"))

    def snapshot(self):
        pulls = self.api("pulls?state=open&per_page=100", pages=True)
        members = {p["number"]: self.member(p["number"]) for p in pulls}
        if len(members) != len(pulls):
            raise ReviewError("Duplicate pull request in paginated discovery")
        permissions = self.api("collaborators?affiliation=all&per_page=100", pages=True)
        permissions = tuple(sorted((p["login"], "admin" if p["permissions"].get("admin")
                                    else "write" if p["permissions"].get("push") else "none",
                                    p.get("role_name", "")) for p in permissions))
        scopes, assigned = [], set()
        for number in sorted({m.stack for m in members.values() if m.stack is not None}):
            if not isinstance(number, int):
                raise ReviewError("Invalid stack number")
            stack = self.api(f"stacks/{number}")
            if stack["number"] != number or not stack["open"]:
                raise ReviewError("Stack changed during discovery")
            ordered = tuple(members[p["number"]] for p in stack["pull_requests"] if p["state"] == "open")
            expected = {m.number for m in members.values() if m.stack == number}
            if {m.number for m in ordered} != expected or len(ordered) != len(expected):
                raise ReviewError("Incomplete or inconsistent stack membership")
            base = stack["base"]["ref"]
            if not ordered or ordered[0].base != base:
                raise ReviewError("Invalid stack base")
            for lower, upper in zip(ordered, ordered[1:]):
                if upper.base != lower.head_ref:
                    raise ReviewError("Nonlinear stack branch chain")
            assigned.update(expected)
            if self.protected(base):
                ordered = tuple(dataclasses.replace(m, reviews=(), comments=m.comments if m == ordered[-1] else ())
                                for m in ordered)
                scopes.append(NativeStack(number, base, ordered))
        for number, member in sorted(members.items()):
            if number not in assigned and self.protected(member.base):
                scopes.append(Standalone(member))
        return Snapshot(self.policy, permissions, tuple(scopes))

    def latest(self, head):
        statuses = self.api(f"commits/{head}/statuses?per_page=100", pages=True)
        return next((s for s in statuses if s["context"] == CONTEXT), None)

    def publish(self, head, state, fingerprint):
        description = f"{state}: evidence {fingerprint}"
        previous = self.latest(head)
        if previous and previous["state"] == state and previous["description"] == description:
            return
        env = dict(os.environ, GITHUB_REPOSITORY=self.repository, PR_HEAD_SHA=head,
                   TRUST_STATUS=state, TRUST_STATUS_DESCRIPTION=description,
                   TRUST_STATUS_TARGET_URL=self.target)
        run(["bash", str(ROOT / "scripts/ci/report-trusted-automation-status.sh")], env=env)

    @staticmethod
    def tsv(path, rows):
        def field(value):
            value = str(value or "")
            if any(c in value for c in "\t\r\n"):
                raise ReviewError("Unexpected control character in approval metadata")
            return value
        path.write_text("".join("\t".join(field(v) for v in row) + "\n" for row in rows))

    def validate(self, member, permissions, directory, *, top=None, comment=None):
        self.tsv(directory / "permissions", permissions)
        (directory / "paths").write_text("\n".join(member.paths) + "\n")
        self.tsv(directory / "reviews", () if top else member.reviews)
        comments = (comment,) if comment else (() if top else tuple(
            c for c in member.comments if c[2] == f"/approve-automation {member.head}"))
        self.tsv(directory / "comments", (c[:4] for c in comments))
        env = dict(os.environ, GITHUB_REPOSITORY=self.repository,
                   PR_NUMBER=str(member.number), PR_HEAD_SHA=member.head, PR_AUTHOR_LOGIN=member.author,
                   MAINTAINER_PERMISSION_ROWS_FILE=str(directory / "permissions"),
                   CHANGED_PATHS_FILE=str(directory / "paths"), REVIEW_ROWS_FILE=str(directory / "reviews"),
                   ATTESTATION_ROWS_FILE=str(directory / "comments"),
                   VERIFY_SIGNATURE_FETCH="false",
                   APPROVAL_MODE="stack" if top else "standalone",
                   APPROVAL_PR_NUMBER=str(top.number if top else member.number),
                   APPROVAL_HEAD_SHA=top.head if top else member.head)
        result = run(["bash", str(ROOT / "scripts/ci/validate-trusted-automation-review.sh")],
                     env=env, check=False)
        if result.returncode:
            print(f"#{member.number}: {result.stderr.strip() or result.stdout.strip()}", file=sys.stderr)
        return result.returncode == 0

    def ancestry(self, members):
        for member in members:
            reference = f"refs/trusted-review/{member.number}"
            run(["git", "fetch", "--no-tags", "origin", f"+refs/pull/{member.number}/head:{reference}"])
            actual = run(["git", "rev-parse", reference]).stdout.strip()
            if actual != member.head:
                raise ReviewError("Head moved while fetching ancestry")
        return all(run(["git", "merge-base", "--is-ancestor", lower.head, upper.head], check=False).returncode == 0
                   for lower, upper in zip(members, members[1:]))

    def evaluate(self, scope, snapshot, directory):
        if not self.ancestry(scope.members):
            return False
        if isinstance(scope, Standalone):
            return self.validate(scope.member, snapshot.permissions, directory)
        candidates = [c for c in scope.top.comments if c[2] == f"/approve-automation {scope.top.head}"]
        for comment in candidates or [None]:
            results = [self.validate(m, snapshot.permissions, directory, top=scope.top, comment=comment)
                       for m in scope.members]
            if all(results):
                return True
        return False

    def reconcile(self):
        snapshot = self.snapshot()
        if os.environ.get("TRUST_REVIEW_FIXTURES_OUTCOME", "success") != "success":
            raise ReviewError("Protected policy fixtures did not pass")
        bindings = {}
        for scope in snapshot.scopes:
            evidence = digest(dataclasses.asdict(scope) | {"policy": snapshot.policy,
                                                          "permissions": snapshot.permissions})
            for member in scope.members:
                bindings.setdefault(member.head, []).append((scope, evidence))
        fingerprints = {head: digest(sorted(e for _, e in scopes)) for head, scopes in bindings.items()}
        for head, fingerprint in fingerprints.items():
            latest = self.latest(head)
            if not latest or latest["description"] not in (f"success: evidence {fingerprint}",
                                                           f"failure: evidence {fingerprint}"):
                self.publish(head, "pending", fingerprint)
        outcomes = {}
        with tempfile.TemporaryDirectory(prefix="trusted-review-") as temporary:
            for scope in snapshot.scopes:
                outcomes[scope] = self.evaluate(scope, snapshot, pathlib.Path(temporary))
        if self.snapshot() != snapshot:
            raise ReviewError("Repository changed during validation; retry reconciliation")
        for head, scopes in bindings.items():
            state = "success" if all(outcomes[scope] for scope, _ in scopes) else "failure"
            self.publish(head, state, fingerprints[head])

    def fail(self, message):
        for head in sorted(self.known_heads):
            try:
                self.publish(head, "error", digest({"error": message, "policy": self.policy}))
            except ReviewError as error:
                print(f"Cannot report error for {head}: {error}", file=sys.stderr)


def main():
    coordinator = None
    try:
        coordinator = Coordinator(os.environ["GITHUB_REPOSITORY"])
        coordinator.reconcile()
    except (ReviewError, KeyError, TypeError, ValueError, OSError, AttributeError, IndexError) as error:
        if coordinator:
            coordinator.fail(str(error))
        print(f"Trusted review reconciliation failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
