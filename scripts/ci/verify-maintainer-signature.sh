#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/ci/maintainer-registry.sh
source "$script_dir/maintainer-registry.sh"

kind="${1:-}"
object="${2:-}"
expected_login="${3:-}"

case "$kind" in
  commit | tag) ;;
  *)
    echo "Usage: $0 <commit|tag> <object> [expected-login]" >&2
    exit 2
    ;;
esac
[[ -n "$object" ]] || {
  echo "A commit SHA or tag name is required." >&2
  exit 2
}
if [[ "$kind" == commit && -z "$expected_login" ]]; then
  echo "Commit verification requires the expected maintainer login." >&2
  exit 2
fi

validate_maintainer_registry
permission_rows="$(maintainer_permission_rows)"
MAINTAINER_PERMISSION_ROWS_DATA="$permission_rows"

signature_object="$object"
if [[ "${VERIFY_SIGNATURE_FETCH:-true}" == true ]]; then
  case "$kind" in
    commit)
      pr_number="${PR_NUMBER:?PR_NUMBER is required to fetch a pull request head}"
      git fetch --no-tags --depth=1 origin "refs/pull/${pr_number}/head"
      fetched_sha="$(git rev-parse FETCH_HEAD)"
      if [[ "$fetched_sha" != "$object" ]]; then
        echo "Fetched pull request head $fetched_sha does not match expected head $object." >&2
        exit 1
      fi
      ;;
    tag)
      git fetch --no-tags origin "refs/tags/${object}"
      signature_object=FETCH_HEAD
      ;;
  esac
fi

if [[ "$kind" == commit && "$(git cat-file -t "$signature_object" 2>/dev/null || true)" != commit ]]; then
  echo "Maintainer signature target is not a commit: $object" >&2
  exit 1
fi
if [[ "$kind" == tag && "$(git cat-file -t "$signature_object" 2>/dev/null || true)" != tag ]]; then
  echo "Release tag must be an annotated tag object: $object" >&2
  exit 1
fi

gnupg_home="$(mktemp -d)"
trap 'rm -rf "$gnupg_home"' EXIT
chmod 700 "$gnupg_home"

root="$(maintainer_registry_repo_root)"
if [[ "$kind" == commit ]]; then
  if ! maintainer_is_active "$expected_login"; then
    echo "Expected signer is not an active registered maintainer: $expected_login" >&2
    exit 1
  fi
  key_paths="$(maintainer_key_paths_for_login "$expected_login")"
else
  key_paths="$(maintainer_registry_rows | awk -F '\t' '{print $3}' | sort -u)"
fi

[[ -n "$key_paths" ]] || {
  echo "No approved public keys are registered for signature verification." >&2
  exit 1
}
while IFS= read -r key_path; do
  [[ -n "$key_path" ]] || continue
  gpg --homedir "$gnupg_home" --batch --quiet --import "$root/$key_path"
done <<<"$key_paths"

set +e
if [[ "$kind" == commit ]]; then
  verification="$(GNUPGHOME="$gnupg_home" git -c gpg.program=gpg verify-commit --raw "$signature_object" 2>&1)"
  verification_status=$?
else
  verification="$(GNUPGHOME="$gnupg_home" git -c gpg.program=gpg verify-tag --raw "$signature_object" 2>&1)"
  verification_status=$?
fi
set -e
if [[ "$verification_status" -ne 0 ]]; then
  printf '%s\n' "$verification" >&2
  echo "OpenPGP verification failed for ${kind} ${object}." >&2
  exit 1
fi

fingerprint="$(
  awk '/^\[GNUPG:\] VALIDSIG / {
    fingerprint = $3
    if ($NF ~ /^[0-9A-F]{40}$/) {
      fingerprint = $NF
    }
    print fingerprint
    exit
  }' <<<"$verification"
)"
if [[ ! "$fingerprint" =~ ^[0-9A-F]{40}$ ]]; then
  echo "OpenPGP verification did not report a primary fingerprint for ${kind} ${object}." >&2
  exit 1
fi

if [[ "$kind" == commit ]]; then
  if ! maintainer_fingerprints_for_login "$expected_login" | grep -Fx "$fingerprint" >/dev/null; then
    echo "Commit signature fingerprint $fingerprint is not registered for $expected_login." >&2
    exit 1
  fi
  signer_login="$expected_login"
else
  signer_login="$(maintainer_login_for_fingerprint "$fingerprint")"
  if [[ -z "$signer_login" ]] || ! maintainer_is_active "$signer_login"; then
    echo "Tag signature fingerprint $fingerprint does not belong to an active maintainer." >&2
    exit 1
  fi
fi

echo "Verified ${kind} ${object} with registered OpenPGP key ${fingerprint} for ${signer_login}."
