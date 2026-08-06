#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/ci/maintainer-registry.sh
source "$script_dir/maintainer-registry.sh"

fixture_dir="$(mktemp -d)"
trap 'rm -rf "$fixture_dir"' EXIT

registry_root="$fixture_dir/registry-root"
keys_dir="$registry_root/keys"
registry="$registry_root/registry.tsv"
permissions="$fixture_dir/permissions.tsv"
mkdir -p "$keys_dir"

generate_public_key() {
  local identity="$1"
  local output="$2"
  local key_home fingerprint
  key_home="$(mktemp -d "$fixture_dir/key-home.XXXXXX")"
  chmod 700 "$key_home"
  gpg --homedir "$key_home" --batch --quiet --pinentry-mode loopback --passphrase '' \
    --quick-generate-key "$identity" ed25519 sign 1d
  fingerprint="$(
    gpg --homedir "$key_home" --batch --with-colons --list-secret-keys "$identity" |
      awk -F: '$1 == "fpr" {print $10; exit}'
  )"
  gpg --homedir "$key_home" --batch --quiet --armor --export "$fingerprint" >"$output"
  printf '%s\n' "$fingerprint"
}

alpha_fingerprint="$(generate_public_key 'Alpha Maintainer <alpha@example.invalid>' "$keys_dir/alpha.asc")"
rotated_fingerprint="$(generate_public_key 'Alpha Rotated <alpha-rotated@example.invalid>' "$keys_dir/alpha-rotated.asc")"
beta_fingerprint="$(generate_public_key 'Beta Maintainer <beta@example.invalid>' "$keys_dir/beta.asc")"

export MAINTAINER_KEYS_PREFIX=keys/
export MAINTAINER_PERMISSION_ROWS_FILE="$permissions"
export MAINTAINER_REGISTRY_FILE="$registry"
export MAINTAINER_REGISTRY_ROOT="$registry_root"

expect_pass() {
  local name="$1"
  shift
  if ! "$@" >/dev/null; then
    echo "Expected maintainer registry fixture to pass: $name" >&2
    exit 1
  fi
}

expect_fail() {
  local name="$1"
  shift
  if "$@" >/dev/null 2>&1; then
    echo "Expected maintainer registry fixture to fail: $name" >&2
    exit 1
  fi
}

write_valid_registry() {
  printf 'alpha-maintainer\t%s\tkeys/alpha.asc\n' "$alpha_fingerprint" >"$registry"
}

write_valid_registry
printf 'alpha-maintainer\tadmin\tadmin\n' >"$permissions"
expect_pass "one active maintainer" validate_maintainer_registry
expect_pass "owner access is active" maintainer_is_active alpha-maintainer

printf 'alpha-maintainer\t%s\tkeys/alpha-rotated.asc\n' "$rotated_fingerprint" >>"$registry"
expect_pass "multiple fingerprints support key rotation" validate_maintainer_registry
fingerprint_count="$(maintainer_fingerprints_for_login alpha-maintainer | wc -l | tr -d ' ')"
[[ "$fingerprint_count" == 2 ]] || {
  echo "Expected both key-rotation fingerprints for alpha-maintainer." >&2
  exit 1
}

write_valid_registry
printf 'malformed registry row\n' >"$registry"
expect_fail "malformed row" validate_maintainer_registry

write_valid_registry
printf 'alpha-maintainer\t%s\tkeys/alpha.asc\n' "$alpha_fingerprint" >>"$registry"
expect_fail "duplicate login and fingerprint" validate_maintainer_registry

write_valid_registry
printf 'beta-maintainer\t%s\tkeys/beta.asc\n' "$alpha_fingerprint" >>"$registry"
expect_fail "fingerprint assigned to multiple logins" validate_maintainer_registry

write_valid_registry
printf 'alpha-maintainer\t%s\tkeys/beta.asc\n' "$alpha_fingerprint" >"$registry"
expect_fail "fingerprint does not match key" validate_maintainer_registry

write_valid_registry
printf 'alpha-maintainer\t%s\tkeys/missing.asc\n' "$alpha_fingerprint" >"$registry"
expect_fail "missing public key" validate_maintainer_registry

write_valid_registry
printf 'alpha-maintainer\t%s\t../alpha.asc\n' "$alpha_fingerprint" >"$registry"
expect_fail "key path escapes registry root" validate_maintainer_registry

{
  printf 'alpha-maintainer\t%s\tkeys/alpha.asc\n' "$alpha_fingerprint"
  printf 'beta-maintainer\t%s\tkeys/beta.asc\n' "$beta_fingerprint"
} >"$registry"
printf 'alpha-maintainer\tadmin\tadmin\nbeta-maintainer\tread\tread\n' >"$permissions"
expect_pass "two well-formed registry entries" validate_maintainer_registry
expect_fail "revoked or read-only collaborator is inactive" maintainer_is_active beta-maintainer
active_logins="$(active_maintainer_logins)"
[[ "$active_logins" == alpha-maintainer ]] || {
  echo "Expected only alpha-maintainer to remain active." >&2
  exit 1
}

printf 'alpha-maintainer\tadmin\tadmin\nbeta-maintainer\twrite\twrite\n' >"$permissions"
expect_pass "write collaborator becomes active" maintainer_is_active beta-maintainer

echo "Maintainer registry fixtures passed."
