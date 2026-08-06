#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
repo_root="$(git rev-parse --show-toplevel)"
validator="$script_dir/validate-trusted-automation-review.sh"
signature_fixture="$(command -v true)"
failing_signature_fixture="$(command -v false)"
fixture_dir="$(mktemp -d)"
trap 'rm -rf "$fixture_dir"' EXIT

head_sha="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
old_sha="bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
normal_paths="$fixture_dir/normal-paths.txt"
protected_paths="$fixture_dir/protected-paths.txt"
reviews="$fixture_dir/reviews.tsv"
attestations="$fixture_dir/attestations.tsv"
permissions="$fixture_dir/permissions.tsv"
registry_root="$fixture_dir/registry-root"
registry="$registry_root/registry.tsv"
keys_dir="$registry_root/keys"

mkdir -p "$keys_dir"
cp "$repo_root/.github/maintainers/keys/markyjordan-6A04F89D74210E5922AB916BFA77CAC10AB52129.asc" \
  "$keys_dir/markyjordan.asc"
printf 'markyjordan\t%s\tkeys/markyjordan.asc\n' \
  6A04F89D74210E5922AB916BFA77CAC10AB52129 >"$registry"
printf 'markyjordan\tadmin\tadmin\n' >"$permissions"
printf '%s\n' README.md >"$normal_paths"
: >"$reviews"
: >"$attestations"

run_fixture() {
  local paths="$1"
  local review_rows="$2"
  local attestation_rows="$3"
  local author="${4:-contributor}"
  local verifier_fixture="${5:-$signature_fixture}"
  env -i PATH="$PATH" HOME="${HOME:-/tmp}" \
    PR_NUMBER=17 \
    PR_AUTHOR_LOGIN="$author" \
    PR_HEAD_SHA="$head_sha" \
    GITHUB_REPOSITORY=markyjordan/gitserious \
    MAINTAINER_KEYS_PREFIX=keys/ \
    MAINTAINER_PERMISSION_ROWS_FILE="$permissions" \
    MAINTAINER_REGISTRY_FILE="$registry" \
    MAINTAINER_REGISTRY_ROOT="$registry_root" \
    MAINTAINER_SIGNATURE_VERIFIER="$verifier_fixture" \
    CHANGED_PATHS_FILE="$paths" \
    REVIEW_ROWS_FILE="$review_rows" \
    ATTESTATION_ROWS_FILE="$attestation_rows" \
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

expect_pass "ordinary documentation change" \
  run_fixture "$normal_paths" "$reviews" "$attestations"
expect_fail "unsigned maintainer documentation head" \
  run_fixture "$normal_paths" "$reviews" "$attestations" markyjordan "$failing_signature_fixture"
expect_pass "external contributor documentation head needs no personal signature" \
  run_fixture "$normal_paths" "$reviews" "$attestations" contributor "$failing_signature_fixture"

protected_path_cases=(
  .github/workflows/ci.yml
  .github/actions/example/action.yml
  .github/dependabot.yml
  .github/zizmor.yml
  .github/maintainers/registry.tsv
  scripts/ci/example.sh
  scripts/release/example.sh
  scripts/archive/example.sh
  scripts/security/example.sh
  scripts/dev/example.sh
  Cargo.toml
  crates/gitserious-core/Cargo.toml
  Cargo.lock
  crates/gitserious-core/src/lib.rs
  rust-toolchain.toml
  clippy.toml
  rustfmt.toml
  justfile
  .cargo/config.toml
  Dockerfile
  containers/release/Dockerfile.linux
  Makefile
  Taskfile.yml
)

for protected_path in "${protected_path_cases[@]}"; do
  printf '%s\n' "$protected_path" >"$protected_paths"
  expect_fail "${protected_path} without approval" \
    run_fixture "$protected_paths" "$reviews" "$attestations"
done

# The changed-path feed includes previous filenames, so a rename out of the
# protected boundary cannot evade review.
printf '%s\n' README.md Cargo.lock >"$protected_paths"
expect_fail "rename from protected lockfile" \
  run_fixture "$protected_paths" "$reviews" "$attestations"

printf '%s\n' "${protected_path_cases[@]}" >"$protected_paths"

printf 'markyjordan\tOWNER\tAPPROVED\t2026-07-12T20:00:00Z\t%s\n' "$head_sha" >"$reviews"
expect_pass "owner approval on current head" \
  run_fixture "$protected_paths" "$reviews" "$attestations"

printf 'outside-collaborator\tCOLLABORATOR\tAPPROVED\t2026-07-12T20:00:00Z\t%s\n' \
  "$head_sha" >"$reviews"
expect_fail "unregistered collaborator approval" \
  run_fixture "$protected_paths" "$reviews" "$attestations"

printf 'markyjordan\tOWNER\tAPPROVED\t2026-07-12T20:00:00Z\t%s\n' "$old_sha" >"$reviews"
expect_fail "approval on stale head" \
  run_fixture "$protected_paths" "$reviews" "$attestations"

printf 'visitor\tCONTRIBUTOR\tAPPROVED\t2026-07-12T20:00:00Z\t%s\n' "$head_sha" >"$reviews"
expect_fail "untrusted approval" \
  run_fixture "$protected_paths" "$reviews" "$attestations"

{
  printf 'markyjordan\tOWNER\tAPPROVED\t2026-07-12T20:00:00Z\t%s\n' "$head_sha"
  printf 'markyjordan\tOWNER\tCHANGES_REQUESTED\t2026-07-12T20:01:00Z\t%s\n' "$head_sha"
} >"$reviews"
expect_fail "approval superseded by changes request" \
  run_fixture "$protected_paths" "$reviews" "$attestations"

printf 'markyjordan\tOWNER\tDISMISSED\t2026-07-12T20:01:00Z\t%s\n' "$head_sha" >"$reviews"
expect_fail "dismissed approval" \
  run_fixture "$protected_paths" "$reviews" "$attestations"

: >"$reviews"
printf 'markyjordan\tOWNER\t/approve-automation %s\t2026-07-12T20:02:00Z\n' \
  "$head_sha" >"$attestations"
expect_pass "solo owner attestation on current head" \
  run_fixture "$protected_paths" "$reviews" "$attestations" markyjordan

printf 'markyjordan\tOWNER\t/approve-automation %s\t2026-07-12T20:02:00Z\n' \
  "$old_sha" >"$attestations"
expect_fail "attestation on stale head" \
  run_fixture "$protected_paths" "$reviews" "$attestations"

printf 'outside-collaborator\tCOLLABORATOR\t/approve-automation %s\t2026-07-12T20:02:00Z\n' \
  "$head_sha" >"$attestations"
expect_fail "unregistered collaborator attestation" \
  run_fixture "$protected_paths" "$reviews" "$attestations"

printf 'markyjordan\tOWNER\tapprove-automation %s\t2026-07-12T20:02:00Z\n' \
  "$head_sha" >"$attestations"
expect_fail "malformed attestation" \
  run_fixture "$protected_paths" "$reviews" "$attestations"

# Add an independently keyed peer. Once both registry entries have live write
# access, a maintainer-authored sensitive PR can no longer self-attest.
peer_home="$fixture_dir/peer-gnupg"
mkdir -m 700 "$peer_home"
gpg --homedir "$peer_home" --batch --quiet --pinentry-mode loopback --passphrase '' \
  --quick-generate-key 'Peer Maintainer <peer@example.invalid>' ed25519 sign 1d
peer_fingerprint="$(
  gpg --homedir "$peer_home" --batch --with-colons --list-secret-keys |
    awk -F: '$1 == "fpr" {print $10; exit}'
)"
gpg --homedir "$peer_home" --batch --quiet --armor --export "$peer_fingerprint" \
  >"$keys_dir/peer.asc"
printf 'peer-maintainer\t%s\tkeys/peer.asc\n' "$peer_fingerprint" >>"$registry"

printf 'markyjordan\tadmin\tadmin\npeer-maintainer\tread\tread\n' >"$permissions"
printf 'peer-maintainer\tCOLLABORATOR\t/approve-automation %s\t2026-07-12T20:03:00Z\n' \
  "$head_sha" >"$attestations"
expect_fail "registered but revoked peer approval" \
  run_fixture "$protected_paths" "$reviews" "$attestations" contributor

printf 'markyjordan\tadmin\tadmin\npeer-maintainer\twrite\twrite\n' >"$permissions"
printf 'markyjordan\tOWNER\t/approve-automation %s\t2026-07-12T20:04:00Z\n' \
  "$head_sha" >"$attestations"
expect_fail "multi-maintainer author self-attestation" \
  run_fixture "$protected_paths" "$reviews" "$attestations" markyjordan

printf 'peer-maintainer\tCOLLABORATOR\t/approve-automation %s\t2026-07-12T20:05:00Z\n' \
  "$head_sha" >"$attestations"
expect_pass "multi-maintainer peer attestation" \
  run_fixture "$protected_paths" "$reviews" "$attestations" markyjordan

cp "$registry" "$fixture_dir/valid-registry.tsv"
printf 'malformed registry row\n' >"$registry"
expect_fail "malformed registry" \
  run_fixture "$normal_paths" "$reviews" "$attestations"
cp "$fixture_dir/valid-registry.tsv" "$registry"

echo "Trusted automation review fixtures passed."
