#!/usr/bin/env bash
set -euo pipefail

event_name="${EVENT_NAME:-${GITHUB_EVENT_NAME:-}}"
request_ref="${REQUEST_REF:-${GITHUB_REF:-}}"
default_branch="${DEFAULT_BRANCH:?DEFAULT_BRANCH is required}"
version_family="${VERSION_FAMILY:?VERSION_FAMILY is required}"
base_ref="${BASE_REF:-main}"

fail() {
  echo "Release preparation request rejected: $*" >&2
  exit 1
}

[[ "$event_name" == workflow_dispatch ]] ||
  fail "unsupported event: ${event_name:-<empty>}"

[[ "$request_ref" == "refs/heads/${default_branch}" ]] ||
  fail "dispatch from refs/heads/${default_branch}; got ${request_ref:-<empty>}"

[[ "$base_ref" == main ]] ||
  fail "release branches must be cut from main; got ${base_ref}"

[[ "$version_family" =~ ^[0-9]+\.[0-9]+$ ]] ||
  fail "version family must look like 0.1; got ${version_family}"

if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
  printf 'base_ref=%s\nrelease_branch=release/%s\n' \
    "$base_ref" "$version_family" >>"$GITHUB_OUTPUT"
fi

echo "Accepted release/${version_family} preparation from ${request_ref} using ${base_ref}."
