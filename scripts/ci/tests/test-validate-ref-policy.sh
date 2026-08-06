#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
validator="$script_dir/validate-ref-policy.sh"
fixture_dir="$(mktemp -d)"
trap 'rm -rf "$fixture_dir"' EXIT

expect_pass() {
  local name="$1"
  shift
  if ! env -i PATH="$PATH" HOME="${HOME:-/tmp}" "$@" bash "$validator" >/dev/null; then
    echo "Expected policy fixture to pass: $name" >&2
    exit 1
  fi
}

expect_fail() {
  local name="$1"
  shift
  if env -i PATH="$PATH" HOME="${HOME:-/tmp}" "$@" bash "$validator" >/dev/null 2>&1; then
    echo "Expected policy fixture to fail: $name" >&2
    exit 1
  fi
}

expect_pass "topic into dev" \
  EVENT_NAME=pull_request BASE_REF=dev HEAD_REF=marky/chore/ci
expect_pass "Dependabot security update into dev" \
  EVENT_NAME=pull_request BASE_REF=dev HEAD_REF=dependabot/cargo/rustsec-fix
expect_fail "protected branch into dev" \
  EVENT_NAME=pull_request BASE_REF=dev HEAD_REF=main
expect_pass "same-repository dev into main" \
  EVENT_NAME=pull_request BASE_REF=main HEAD_REF=dev \
  HEAD_REPOSITORY=markyjordan/gitserious REPOSITORY=markyjordan/gitserious
expect_fail "fork dev into main" \
  EVENT_NAME=pull_request BASE_REF=main HEAD_REF=dev \
  HEAD_REPOSITORY=someone/gitserious REPOSITORY=markyjordan/gitserious
expect_pass "hotfix into release line" \
  EVENT_NAME=pull_request BASE_REF=release/0.1 HEAD_REF=hotfix/GHSA-example
expect_fail "feature into release line" \
  EVENT_NAME=pull_request BASE_REF=release/0.1 HEAD_REF=marky/feat/new-work
expect_pass "post-merge dev audit" \
  EVENT_NAME=push REF_NAME=dev CURRENT_SHA=HEAD
expect_pass "new release branch" \
  EVENT_NAME=push REF_NAME=release/0.1 BEFORE_SHA=0000000000000000000000000000000000000000 \
  CURRENT_SHA=HEAD

git -C "$fixture_dir" init -q -b fixture-base
git -C "$fixture_dir" config user.name "CI Fixture"
git -C "$fixture_dir" config user.email "ci-fixture@example.invalid"
git -C "$fixture_dir" commit --allow-empty -qm base
base_sha="$(git -C "$fixture_dir" rev-parse HEAD)"
git -C "$fixture_dir" checkout -qb topic
git -C "$fixture_dir" commit --allow-empty -qm topic
git -C "$fixture_dir" checkout -qb main "$base_sha"
git -C "$fixture_dir" commit --allow-empty -qm main
git -C "$fixture_dir" merge --no-ff -qm merge topic
merge_sha="$(git -C "$fixture_dir" rev-parse HEAD)"

(
  cd "$fixture_dir"
  expect_pass "merge commit into main" \
    EVENT_NAME=push REF_NAME=main CURRENT_SHA="$merge_sha"
  expect_fail "single-parent commit into main" \
    EVENT_NAME=push REF_NAME=main CURRENT_SHA="$base_sha"
)

echo "Ref policy fixtures passed."
