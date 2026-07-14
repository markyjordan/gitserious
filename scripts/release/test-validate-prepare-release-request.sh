#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
validator="$script_dir/validate-prepare-release-request.sh"
fixture_dir="$(mktemp -d)"
trap 'rm -rf "$fixture_dir"' EXIT

expect_pass() {
  local output_file="$fixture_dir/output"

  : >"$output_file"
  EVENT_NAME=workflow_dispatch \
    REQUEST_REF=refs/heads/dev \
    DEFAULT_BRANCH=dev \
    VERSION_FAMILY=0.1 \
    BASE_REF=main \
    GITHUB_OUTPUT="$output_file" \
    bash "$validator" >/dev/null

  grep -Fx 'base_ref=main' "$output_file" >/dev/null
  grep -Fx 'release_branch=release/0.1' "$output_file" >/dev/null
}

expect_fail() {
  local event_name="$1"
  local request_ref="$2"
  local default_branch="$3"
  local version_family="$4"
  local base_ref="$5"

  if EVENT_NAME="$event_name" \
    REQUEST_REF="$request_ref" \
    DEFAULT_BRANCH="$default_branch" \
    VERSION_FAMILY="$version_family" \
    BASE_REF="$base_ref" \
    bash "$validator" >/dev/null 2>&1; then
    echo "Expected release preparation request to fail: ${event_name} ${request_ref} ${version_family} ${base_ref}" >&2
    exit 1
  fi
}

expect_pass

expect_fail push refs/heads/dev dev 0.1 main
expect_fail workflow_dispatch refs/heads/main dev 0.1 main
expect_fail workflow_dispatch refs/heads/topic dev 0.1 main
expect_fail workflow_dispatch refs/heads/dev dev v0.1 main
expect_fail workflow_dispatch refs/heads/dev dev 0.1 dev

echo "Release preparation request fixtures passed."
