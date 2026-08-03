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

case "$release_mode" in
  dry-run | publish) ;;
  *) fail "mode must be dry-run or publish; got ${release_mode}" ;;
esac

is_rc=false
is_dry_run=false
checkout_ref=""
readiness_tag="$tag"
source_ref=""

if [[ "$tag" == dry-run ]]; then
  is_dry_run=true
  readiness_tag=""
  [[ "$event_name" == workflow_dispatch ]] ||
    fail "tag=dry-run is available only through workflow_dispatch"
  [[ "$release_mode" == dry-run ]] ||
    fail "tag=dry-run cannot be published"
  case "$release_ref" in
    refs/heads/main | refs/heads/release/*)
      checkout_ref="$release_ref"
      source_ref="${release_ref#refs/heads/}"
      ;;
    *)
      fail "tag=dry-run must be dispatched from main or release/X.Y; got ${release_ref:-<empty>}"
      ;;
  esac
else
  if [[ ! "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-rc[1-9][0-9]*)?$ ]]; then
    fail "tag must be dry-run, vX.Y.Z, or vX.Y.Z-rcN; got ${tag}"
  fi

  case "$event_name" in
    push)
      [[ "$release_ref" == "refs/tags/${tag}" ]] ||
        fail "tag push ref ${release_ref:-<empty>} does not match ${tag}"
      [[ "$release_mode" == dry-run ]] ||
        fail "tag pushes may only run in dry-run mode"
      ;;
    workflow_dispatch)
      [[ "$release_ref" == "refs/tags/${tag}" ]] ||
        fail "tag requests must be dispatched from refs/tags/${tag}; got ${release_ref:-<empty>}"
      ;;
    *) fail "unsupported event: ${event_name:-<empty>}" ;;
  esac

  checkout_ref="refs/tags/${tag}"
  if [[ "$tag" == *-rc* ]]; then
    is_rc=true
  fi
fi

if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
  printf '%s\n' \
    "release_tag=${tag}" \
    "release_mode=${release_mode}" \
    "is_rc=${is_rc}" \
    "is_dry_run=${is_dry_run}" \
    "checkout_ref=${checkout_ref}" \
    "readiness_tag=${readiness_tag}" \
    "source_ref=${source_ref}" >>"$GITHUB_OUTPUT"
fi

echo "Accepted ${release_mode} request for ${tag} from ${release_ref}."
