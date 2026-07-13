#!/usr/bin/env bash
set -euo pipefail

event_name="${EVENT_NAME:-${GITHUB_EVENT_NAME:-}}"
release_ref="${RELEASE_REF:-${GITHUB_REF:-}}"
tag="${RELEASE_TAG:?RELEASE_TAG is required}"
release_mode="${RELEASE_MODE:-dry-run}"

fail() {
  echo "Release request rejected: $*" >&2
  exit 1
}

if [[ ! "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-rc[1-9][0-9]*)?$ ]]; then
  fail "tag must look like vX.Y.Z or vX.Y.Z-rcN; got ${tag}"
fi

case "$release_mode" in
  dry-run | publish) ;;
  *) fail "mode must be dry-run or publish; got ${release_mode}" ;;
esac

case "$event_name" in
  push)
    [[ "$release_ref" == "refs/tags/${tag}" ]] ||
      fail "tag push ref ${release_ref:-<empty>} does not match ${tag}"
    [[ "$release_mode" == dry-run ]] ||
      fail "tag pushes may only run in dry-run mode"
    ;;
  workflow_dispatch)
    if [[ "$release_mode" == publish && "$release_ref" != "refs/tags/${tag}" ]]; then
      fail "publish mode must be dispatched from refs/tags/${tag}; got ${release_ref:-<empty>}"
    fi
    ;;
  *) fail "unsupported event: ${event_name:-<empty>}" ;;
esac

is_rc=false
if [[ "$tag" == *-rc* ]]; then
  is_rc=true
fi

if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
  printf 'release_tag=%s\nrelease_mode=%s\nis_rc=%s\n' \
    "$tag" "$release_mode" "$is_rc" >>"$GITHUB_OUTPUT"
fi

echo "Accepted ${release_mode} request for ${tag} from ${release_ref}."
