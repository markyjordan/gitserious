#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
verifier="$script_dir/verify-maintainer-signature.sh"
fixture_dir="$(mktemp -d)"
trap 'rm -rf "$fixture_dir"' EXIT

signing_home="$fixture_dir/signing-home"
registry_root="$fixture_dir/registry-root"
keys_dir="$registry_root/keys"
registry="$registry_root/registry.tsv"
permissions="$fixture_dir/permissions.tsv"
repository="$fixture_dir/repository"
mkdir -m 700 "$signing_home"
mkdir -p "$keys_dir" "$repository"

generate_key() {
  local identity="$1"
  gpg --homedir "$signing_home" --batch --quiet --pinentry-mode loopback --passphrase '' \
    --quick-generate-key "$identity" ed25519 sign 1d
  gpg --homedir "$signing_home" --batch --with-colons --list-secret-keys "$identity" |
    awk -F: '$1 == "fpr" {print $10; exit}'
}

alpha_fingerprint="$(generate_key 'Alpha Signer <alpha-signer@example.invalid>')"
beta_fingerprint="$(generate_key 'Beta Signer <beta-signer@example.invalid>')"
gpg --homedir "$signing_home" --batch --quiet --armor --export "$alpha_fingerprint" >"$keys_dir/alpha.asc"
gpg --homedir "$signing_home" --batch --quiet --armor --export "$beta_fingerprint" >"$keys_dir/beta.asc"
{
  printf 'alpha-maintainer\t%s\tkeys/alpha.asc\n' "$alpha_fingerprint"
  printf 'beta-maintainer\t%s\tkeys/beta.asc\n' "$beta_fingerprint"
} >"$registry"
printf 'alpha-maintainer\tadmin\tadmin\nbeta-maintainer\tread\tread\n' >"$permissions"

git -C "$repository" init -q
git -C "$repository" config user.name 'Signature Fixture'
git -C "$repository" config user.email 'signature-fixture@example.invalid'
git -C "$repository" config gpg.program gpg
git -C "$repository" config commit.gpgsign true

printf 'signed\n' >"$repository/fixture.txt"
git -C "$repository" add fixture.txt
GNUPGHOME="$signing_home" git -C "$repository" -c user.signingkey="$alpha_fingerprint" \
  commit -q -m 'signed commit'
signed_commit="$(git -C "$repository" rev-parse HEAD)"

printf 'unsigned\n' >>"$repository/fixture.txt"
git -C "$repository" add fixture.txt
git -C "$repository" -c commit.gpgsign=false commit -q -m 'unsigned commit'
unsigned_commit="$(git -C "$repository" rev-parse HEAD)"

printf 'wrong key\n' >>"$repository/fixture.txt"
git -C "$repository" add fixture.txt
GNUPGHOME="$signing_home" git -C "$repository" -c user.signingkey="$beta_fingerprint" \
  commit -q -m 'wrong-key commit'
wrong_key_commit="$(git -C "$repository" rev-parse HEAD)"

GNUPGHOME="$signing_home" git -C "$repository" -c user.signingkey="$alpha_fingerprint" \
  tag -s v1.0.0-rc1 "$signed_commit" -m 'valid signed tag'
git -C "$repository" tag -a v1.0.0-rc2 "$signed_commit" -m 'unsigned annotated tag'
git -C "$repository" tag v1.0.0-rc3 "$signed_commit"
GNUPGHOME="$signing_home" git -C "$repository" -c user.signingkey="$beta_fingerprint" \
  tag -s v1.0.0-rc4 "$signed_commit" -m 'inactive signer tag'

run_verifier() {
  env -i PATH="$PATH" HOME="${HOME:-/tmp}" \
    GITHUB_REPOSITORY=markyjordan/gitserious \
    MAINTAINER_KEYS_PREFIX=keys/ \
    MAINTAINER_PERMISSION_ROWS_FILE="$permissions" \
    MAINTAINER_REGISTRY_FILE="$registry" \
    MAINTAINER_REGISTRY_ROOT="$registry_root" \
    VERIFY_SIGNATURE_FETCH=false \
    bash "$verifier" "$@"
}

expect_pass() {
  local name="$1"
  shift
  if ! (cd "$repository" && "$@") >/dev/null; then
    echo "Expected OpenPGP fixture to pass: $name" >&2
    exit 1
  fi
}

expect_fail() {
  local name="$1"
  shift
  if (cd "$repository" && "$@") >/dev/null 2>&1; then
    echo "Expected OpenPGP fixture to fail: $name" >&2
    exit 1
  fi
}

expect_pass "registered maintainer commit" \
  run_verifier commit "$signed_commit" alpha-maintainer
expect_fail "unsigned maintainer commit" \
  run_verifier commit "$unsigned_commit" alpha-maintainer
expect_fail "commit signed by the wrong maintainer key" \
  run_verifier commit "$wrong_key_commit" alpha-maintainer

expect_pass "annotated tag signed by an active maintainer" \
  run_verifier tag v1.0.0-rc1
expect_fail "unsigned annotated release tag" \
  run_verifier tag v1.0.0-rc2
expect_fail "lightweight release tag" \
  run_verifier tag v1.0.0-rc3
expect_fail "release tag signed by an inactive maintainer" \
  run_verifier tag v1.0.0-rc4

echo "Maintainer OpenPGP signature fixtures passed."
