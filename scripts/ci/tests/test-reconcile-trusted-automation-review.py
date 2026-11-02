#!/usr/bin/env python3
import dataclasses
import importlib.util
import json
import pathlib
import sys
import subprocess
import tempfile
import unittest
from unittest.mock import patch

PATH = pathlib.Path(__file__).resolve().parents[1] / "reconcile-trusted-automation-review.py"
SPEC = importlib.util.spec_from_file_location("reconcile", PATH)
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)
M = MODULE


def member(number, *, head=None, author="owner", stack=None, base="dev", comments=()):
    return M.Member(number, head or f"{number:040x}", author, base, f"branch-{number}", stack,
                    ("src/example.rs",), (), tuple(comments))


def comment(head, author="owner", identifier=1):
    return (author, "OWNER", f"/approve-automation {head}", "2026-01-01T00:00:00Z", identifier)


class Fake(M.Coordinator):
    def __init__(self, scopes):
        self.repository = "owner/repo"
        self.policy = "f" * 40
        self.target = "https://example.invalid/run"
        self.known_heads = set()
        self.current = M.Snapshot(self.policy, (("owner", "admin", "admin"),), tuple(scopes))
        self.statuses = {}
        self.writes = []
        self.accept = lambda m, c: c is not None
        self.linear = True
        self.changed = None
        self.reads = 0
        self.fail_write = None

    def snapshot(self):
        self.reads += 1
        result = self.changed if self.changed and self.reads % 2 == 0 else self.current
        self.known_heads.update(m.head for s in result.scopes for m in s.members)
        return result

    def latest(self, head):
        return self.statuses.get(head)

    def publish(self, head, state, fingerprint):
        def report(arguments, *, env=None):
            if self.fail_write == (head, state):
                raise M.ReviewError("publication failed")
            self.statuses[head] = {"state": env["TRUST_STATUS"], "description": env["TRUST_STATUS_DESCRIPTION"]}
            self.writes.append((head, state, fingerprint))
        with patch.object(M, "run", side_effect=report):
            M.Coordinator.publish(self, head, state, fingerprint)

    def ancestry(self, members):
        return self.linear

    def validate(self, member, permissions, directory, *, top=None, comment=None):
        return self.accept(member, comment)


class ReconciliationTests(unittest.TestCase):
    def stack(self):
        lower = member(1, stack=9)
        top = member(2, stack=9, base=lower.head_ref)
        top = dataclasses.replace(top, comments=(comment(top.head),))
        return M.NativeStack(9, "dev", (lower, top))

    def test_top_comment_clears_all_heads_and_unchanged_poll_writes_nothing(self):
        scope = self.stack()
        fake = Fake((scope,))
        fake.reconcile()
        self.assertEqual([x[1] for x in fake.writes], ["pending", "pending", "success", "success"])
        count = len(fake.writes)
        fake.reconcile()
        self.assertEqual(len(fake.writes), count)

    def test_stale_deleted_edited_and_lower_comments_do_not_cover_stack(self):
        scope = self.stack()
        for evidence in [(), (comment("a" * 40),), (("owner", "OWNER", "edited", "now", 4),)]:
            top = dataclasses.replace(scope.top, comments=evidence)
            lower = dataclasses.replace(scope.members[0], comments=(comment(top.head),))
            fake = Fake((dataclasses.replace(scope, members=(lower, top)),))
            fake.reconcile()
            self.assertTrue(all(s["state"] == "failure" for s in fake.statuses.values()))
            count = len(fake.writes)
            fake.reconcile()
            self.assertEqual(len(fake.writes), count)

    def test_top_change_revokes_unchanged_lower_head_before_validation(self):
        scope = self.stack()
        fake = Fake((scope,))
        fake.reconcile()
        fake.writes.clear()
        top = dataclasses.replace(scope.top, head="c" * 40)
        fake.current = dataclasses.replace(fake.current, scopes=(dataclasses.replace(scope, members=(scope.members[0], top)),))
        fake.reconcile()
        self.assertEqual(fake.writes[0][:2], (scope.members[0].head, "pending"))
        self.assertEqual(fake.statuses[scope.members[0].head]["state"], "failure")

    def test_one_commenter_must_be_eligible_for_every_member(self):
        scope = self.stack()
        top = dataclasses.replace(scope.top, comments=(comment(scope.top.head, "a"), comment(scope.top.head, "b", 2)))
        fake = Fake((dataclasses.replace(scope, members=(scope.members[0], top)),))
        fake.accept = lambda m, c: c is not None and c[0] == ("a" if m.number == 1 else "b")
        fake.reconcile()
        self.assertTrue(all(s["state"] == "failure" for s in fake.statuses.values()))

    def test_signature_failure_or_divergence_blocks_scope(self):
        for signature_ok, linear in [(False, True), (True, False)]:
            fake = Fake((self.stack(),))
            fake.accept = lambda m, c: signature_ok and c is not None
            fake.linear = linear
            fake.reconcile()
            self.assertTrue(all(s["state"] == "failure" for s in fake.statuses.values()))

    def test_documentation_only_stack_needs_no_attestation(self):
        scope = self.stack()
        scope = dataclasses.replace(scope, members=(scope.members[0], dataclasses.replace(scope.top, comments=())))
        fake = Fake((scope,))
        fake.accept = lambda m, c: True
        fake.reconcile()
        self.assertTrue(all(s["state"] == "success" for s in fake.statuses.values()))

    def test_shared_head_requires_all_scopes_to_pass(self):
        a = member(1)
        b = member(2, head=a.head)
        fake = Fake((M.Standalone(a), M.Standalone(b)))
        fake.accept = lambda m, c: m.number == 1
        fake.reconcile()
        self.assertEqual(fake.statuses[a.head]["state"], "failure")
        self.assertEqual(len(fake.statuses), 1)

    def test_membership_change_or_dissolution_recomputes_old_heads(self):
        scope = self.stack()
        fake = Fake((scope,))
        fake.reconcile()
        fake.current = dataclasses.replace(fake.current, scopes=(M.Standalone(scope.members[0]),))
        fake.reconcile()
        self.assertEqual(fake.statuses[scope.members[0].head]["state"], "failure")
        self.assertEqual(fake.writes[-2][1], "pending")

    def test_failed_protected_fixtures_invalidate_known_heads(self):
        fake = Fake((self.stack(),))
        fake.reconcile()
        fake.writes.clear()
        with patch.dict(M.os.environ, {"TRUST_REVIEW_FIXTURES_OUTCOME": "failure"}):
            with self.assertRaises(M.ReviewError):
                fake.reconcile()
            fake.fail("Protected policy fixtures did not pass")
        self.assertFalse(any(state == "success" for _, state, _ in fake.writes))
        self.assertTrue(all(s["state"] == "error" for s in fake.statuses.values()))

    def test_snapshot_drift_cannot_publish_success(self):
        fake = Fake((self.stack(),))
        fake.changed = dataclasses.replace(fake.current, permissions=())
        with self.assertRaises(M.ReviewError):
            fake.reconcile()
        self.assertFalse(any(state == "success" for _, state, _ in fake.writes))
        fake.fail("snapshot changed")
        self.assertTrue(all(s["state"] == "error" for s in fake.statuses.values()))

    def test_partial_publication_failure_can_mark_remaining_heads_error(self):
        scope = self.stack()
        fake = Fake((scope,))
        fake.fail_write = (scope.top.head, "success")
        with self.assertRaises(M.ReviewError):
            fake.reconcile()
        fake.fail("publication failed")
        self.assertTrue(all(s["state"] == "error" for s in fake.statuses.values()))


class GitHistoryTests(unittest.TestCase):
    def test_shared_upper_head_keeps_history_across_scope_validation(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            origin = root / "origin"
            work = root / "work"
            def git(directory, *arguments, input=None):
                return subprocess.run(["git", "-C", str(directory), *arguments], input=input,
                    text=True, check=True, capture_output=True).stdout.strip()
            subprocess.run(["git", "init", "--bare", str(origin)], check=True, capture_output=True)
            tree = git(origin, "hash-object", "-w", "-t", "tree", "--stdin", input="")
            def commit(parent=None):
                arguments = ["-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid",
                             "commit-tree", tree]
                if parent:
                    arguments.extend(["-p", parent])
                return git(origin, *arguments, input="fixture\n")
            base = commit()
            lower = commit(base)
            upper = commit(lower)
            for number, head in [(1, lower), (2, upper), (3, base), (4, upper)]:
                git(origin, "update-ref", f"refs/pull/{number}/head", head)
            subprocess.run(["git", "init", str(work)], check=True, capture_output=True)
            git(work, "remote", "add", "origin", str(origin))
            scripts = work / "scripts/ci"
            scripts.mkdir(parents=True)
            (scripts / "validate-trusted-automation-review.sh").write_text(
                '#!/bin/sh\nif [ "$VERIFY_SIGNATURE_FETCH" != false ]; then\n'
                'git fetch --depth=1 origin "refs/pull/$PR_NUMBER/head"\nfi\n')
            first = M.NativeStack(9, "dev", (member(1, head=lower), member(2, head=upper)))
            second = M.NativeStack(10, "dev", (member(3, head=base), member(4, head=upper)))
            coordinator = object.__new__(M.Coordinator)
            coordinator.repository = "owner/repo"
            snapshot = M.Snapshot("f" * 40, (), (first, second))
            with patch.object(M, "ROOT", work):
                self.assertTrue(coordinator.evaluate(first, snapshot, root))
                self.assertTrue(coordinator.evaluate(second, snapshot, root))
                self.assertEqual(git(work, "rev-list", "--count", upper), "3")


class Discovery(M.Coordinator):
    def __init__(self, data):
        self.repository = "owner/repo"
        self.policy = "f" * 40
        self.known_heads = set()
        self.data = data

    def api(self, path, *, pages=False):
        value = self.data[path]
        if isinstance(value, Exception):
            raise value
        return value


class DiscoveryTests(unittest.TestCase):
    def data(self):
        data = {"pulls?state=open&per_page=100": [{"number": 1}, {"number": 2}],
                "collaborators?affiliation=all&per_page=100": [],
                "stacks/9": {"number": 9, "open": True, "base": {"ref": "dev"},
                             "pull_requests": [{"number": 1, "state": "open"}, {"number": 2, "state": "open"}]}}
        for n in [1, 2]:
            data[f"pulls/{n}"] = {"number": n, "head": {"sha": f"{n:040x}", "ref": f"branch-{n}"},
                                  "state": "open", "user": {"login": "owner"},
                                  "base": {"ref": "dev" if n == 1 else "branch-1"}, "stack": {"number": 9},
                                  "changed_files": 1}
            data[f"pulls/{n}/files?per_page=100"] = [{"filename": "README.md", "previous_filename": "Cargo.lock"}]
            data[f"pulls/{n}/reviews?per_page=100"] = []
            data[f"issues/{n}/comments?per_page=100"] = []
        return data

    def test_current_native_order_and_previous_filenames(self):
        snapshot = Discovery(self.data()).snapshot()
        self.assertEqual([m.number for m in snapshot.scopes[0].members], [1, 2])
        self.assertEqual(snapshot.scopes[0].members[0].paths, ("Cargo.lock", "README.md"))

    def test_irrelevant_comments_and_stack_reviews_do_not_change_snapshot(self):
        data = self.data()
        before = Discovery(data).snapshot()
        data["issues/2/comments?per_page=100"] = [{"user": {"login": "someone"},
            "author_association": "NONE", "body": "ordinary discussion", "updated_at": "now", "id": 3}]
        data["pulls/1/reviews?per_page=100"] = [{"user": {"login": "someone"},
            "author_association": "NONE", "state": "APPROVED", "submitted_at": "now", "commit_id": f"{1:040x}"}]
        self.assertEqual(Discovery(data).snapshot(), before)

    def test_closed_members_are_excluded_after_partial_merge(self):
        data = self.data()
        data["pulls?state=open&per_page=100"] = [{"number": 2}]
        data["pulls/2"]["base"]["ref"] = "dev"
        data["stacks/9"]["pull_requests"][0]["state"] = "closed"
        scope = Discovery(data).snapshot().scopes[0]
        self.assertEqual([m.number for m in scope.members], [2])

    def test_missing_stack_never_falls_back_to_standalone(self):
        data = self.data()
        data["stacks/9"] = M.ReviewError("API unavailable")
        with self.assertRaises(M.ReviewError):
            Discovery(data).snapshot()

    def test_incomplete_files_membership_and_nonlinear_metadata_fail(self):
        for mutate in [lambda d: d["pulls/1"].update(changed_files=2),
                       lambda d: d["stacks/9"].update(pull_requests=[]),
                       lambda d: d["pulls/2"]["base"].update(ref="other")]:
            data = self.data()
            mutate(data)
            with self.assertRaises(M.ReviewError):
                Discovery(data).snapshot()

    def test_standalone_and_unsupported_targets(self):
        data = self.data()
        for n in [1, 2]:
            data[f"pulls/{n}"]["stack"] = None
        data["pulls/2"]["base"]["ref"] = "feature"
        scopes = Discovery(data).snapshot().scopes
        self.assertEqual(len(scopes), 1)
        self.assertIsInstance(scopes[0], M.Standalone)

    def test_api_pagination_and_malformed_responses(self):
        coordinator = object.__new__(M.Coordinator)
        coordinator.repository = "owner/repo"
        result = type("Result", (), {"stdout": '[[{"n":1}],[{"n":2}]]'})()
        with patch.object(M, "run", return_value=result) as call:
            self.assertEqual(coordinator.api("pulls", pages=True), [{"n": 1}, {"n": 2}])
            self.assertIn("--paginate", call.call_args.args[0])
            self.assertIn("--slurp", call.call_args.args[0])
        result.stdout = '[{"n":1}]'
        with patch.object(M, "run", return_value=result), self.assertRaises(M.ReviewError):
            coordinator.api("pulls", pages=True)

    def test_workflow_executes_only_default_policy_and_review_wakeup_is_read_only(self):
        root = PATH.parents[2]
        writer = (root / ".github/workflows/trusted-automation-review.yml").read_text()
        wakeup = (root / ".github/workflows/trusted-review-wakeup.yml").read_text()
        self.assertIn("ref: ${{ github.event.repository.default_branch }}", writer)
        self.assertNotIn("pull_request.head", writer)
        self.assertNotIn("pull_request_review:", writer)
        self.assertIn("cancel-in-progress: false", writer)
        self.assertIn("workflow_run:", writer)
        self.assertIn("TRUST_REVIEW_FIXTURES_OUTCOME: ${{ steps.fixtures.outcome }}", writer)
        self.assertIn("if: always() && steps.policy.outcome == 'success'", writer)
        self.assertIn("2-59/5 * * * *", writer)
        self.assertIn("permissions: {}", wakeup)
        self.assertNotIn("checkout", wakeup)
        self.assertNotIn("github.token", wakeup)


if __name__ == "__main__":
    unittest.main()
