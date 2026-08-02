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
attestations="$fixture_dir/attestations.tsv"

printf '%s\n' README.md >"$normal_paths"
: >"$reviews"
: >"$attestations"

run_fixture() {
  env -i PATH="$PATH" HOME="${HOME:-/tmp}" \
    PR_NUMBER=17 \
    PR_HEAD_SHA="$head_sha" \
    GITHUB_REPOSITORY=markyjordan/gitserious \
    TRUSTED_AUTOMATION_APPROVERS=maintainer \
    CHANGED_PATHS_FILE="$1" \
    REVIEW_ROWS_FILE="$2" \
    ATTESTATION_ROWS_FILE="$3" \
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

expect_pass "ordinary source change" run_fixture "$normal_paths" "$reviews" "$attestations"

protected_path_cases=(
  .github/workflows/ci.yml
  .github/actions/example/action.yml
  .github/dependabot.yml
  .github/zizmor.yml
  scripts/ci/example.sh
  scripts/release/example.sh
  scripts/archive/example.sh
  scripts/security/example.sh
)

for protected_path in "${protected_path_cases[@]}"; do
  printf '%s\n' "$protected_path" >"$protected_paths"
  expect_fail "${protected_path} without approval" \
    run_fixture "$protected_paths" "$reviews" "$attestations"
done

printf '%s\n' "${protected_path_cases[@]}" >"$protected_paths"

printf 'maintainer\tOWNER\tAPPROVED\t2026-07-12T20:00:00Z\t%s\n' "$head_sha" >"$reviews"
expect_pass "owner approval on current head" run_fixture "$protected_paths" "$reviews" "$attestations"

printf 'maintainer\tCOLLABORATOR\tAPPROVED\t2026-07-12T20:00:00Z\t%s\n' "$head_sha" >"$reviews"
expect_pass "allowlisted maintainer approval" run_fixture "$protected_paths" "$reviews" "$attestations"

printf 'outside-collaborator\tCOLLABORATOR\tAPPROVED\t2026-07-12T20:00:00Z\t%s\n' \
  "$head_sha" >"$reviews"
expect_fail "non-maintainer collaborator approval" \
  run_fixture "$protected_paths" "$reviews" "$attestations"

printf 'maintainer\tOWNER\tAPPROVED\t2026-07-12T20:00:00Z\t%s\n' "$old_sha" >"$reviews"
expect_fail "approval on stale head" run_fixture "$protected_paths" "$reviews" "$attestations"

printf 'visitor\tCONTRIBUTOR\tAPPROVED\t2026-07-12T20:00:00Z\t%s\n' "$head_sha" >"$reviews"
expect_fail "untrusted approval" run_fixture "$protected_paths" "$reviews" "$attestations"

{
  printf 'maintainer\tMEMBER\tAPPROVED\t2026-07-12T20:00:00Z\t%s\n' "$head_sha"
  printf 'maintainer\tMEMBER\tCHANGES_REQUESTED\t2026-07-12T20:01:00Z\t%s\n' "$head_sha"
} >"$reviews"
expect_fail "approval superseded by changes request" run_fixture "$protected_paths" "$reviews" "$attestations"

: >"$reviews"
printf 'maintainer\tOWNER\t/approve-automation %s\t2026-07-12T20:02:00Z\n' \
  "$head_sha" >"$attestations"
expect_pass "owner attestation on current head" run_fixture "$protected_paths" "$reviews" "$attestations"

printf 'maintainer\tOWNER\t/approve-automation %s\t2026-07-12T20:02:00Z\n' \
  "$old_sha" >"$attestations"
expect_fail "attestation on stale head" run_fixture "$protected_paths" "$reviews" "$attestations"

printf 'visitor\tCONTRIBUTOR\t/approve-automation %s\t2026-07-12T20:02:00Z\n' \
  "$head_sha" >"$attestations"
expect_fail "untrusted attestation" run_fixture "$protected_paths" "$reviews" "$attestations"

printf 'outside-collaborator\tCOLLABORATOR\t/approve-automation %s\t2026-07-12T20:02:00Z\n' \
  "$head_sha" >"$attestations"
expect_fail "non-maintainer collaborator attestation" \
  run_fixture "$protected_paths" "$reviews" "$attestations"

printf 'maintainer\tOWNER\tapprove-automation %s\t2026-07-12T20:02:00Z\n' \
  "$head_sha" >"$attestations"
expect_fail "malformed attestation" run_fixture "$protected_paths" "$reviews" "$attestations"

echo "Trusted automation review fixtures passed."
