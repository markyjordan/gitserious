#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
validator="$script_dir/validate-trusted-automation-review.sh"
fixture_dir="$(mktemp -d)"
trap 'rm -rf "$fixture_dir"' EXIT

head_sha="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
old_sha="bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
normal_paths="$fixture_dir/normal-paths.txt"
protected_paths="$fixture_dir/protected-paths.txt"
reviews="$fixture_dir/reviews.tsv"

printf '%s\n' README.md >"$normal_paths"
printf '%s\n' .github/workflows/ci.yml scripts/ci/example.sh >"$protected_paths"
: >"$reviews"

run_fixture() {
  env -i PATH="$PATH" HOME="${HOME:-/tmp}" \
    PR_NUMBER=17 \
    PR_HEAD_SHA="$head_sha" \
    GITHUB_REPOSITORY=markyjordan/gitserious \
    CHANGED_PATHS_FILE="$1" \
    REVIEW_ROWS_FILE="$2" \
    bash "$validator"
}

expect_pass() {
  local name="$1"
  shift
  if ! "$@" >/dev/null; then
    echo "Expected trusted review fixture to pass: $name" >&2
    exit 1
  fi
}

expect_fail() {
  local name="$1"
  shift
  if "$@" >/dev/null 2>&1; then
    echo "Expected trusted review fixture to fail: $name" >&2
    exit 1
  fi
}

expect_pass "ordinary source change" run_fixture "$normal_paths" "$reviews"
expect_fail "protected path without approval" run_fixture "$protected_paths" "$reviews"

printf 'maintainer\tOWNER\tAPPROVED\t2026-07-12T20:00:00Z\t%s\n' "$head_sha" >"$reviews"
expect_pass "owner approval on current head" run_fixture "$protected_paths" "$reviews"

printf 'maintainer\tOWNER\tAPPROVED\t2026-07-12T20:00:00Z\t%s\n' "$old_sha" >"$reviews"
expect_fail "approval on stale head" run_fixture "$protected_paths" "$reviews"

printf 'visitor\tCONTRIBUTOR\tAPPROVED\t2026-07-12T20:00:00Z\t%s\n' "$head_sha" >"$reviews"
expect_fail "untrusted approval" run_fixture "$protected_paths" "$reviews"

{
  printf 'maintainer\tMEMBER\tAPPROVED\t2026-07-12T20:00:00Z\t%s\n' "$head_sha"
  printf 'maintainer\tMEMBER\tCHANGES_REQUESTED\t2026-07-12T20:01:00Z\t%s\n' "$head_sha"
} >"$reviews"
expect_fail "approval superseded by changes request" run_fixture "$protected_paths" "$reviews"

echo "Trusted automation review fixtures passed."
