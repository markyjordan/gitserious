#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
validator="$script_dir/validate-release-request.sh"
fixture_dir="$(mktemp -d)"
trap 'rm -rf "$fixture_dir"' EXIT

expect_pass() {
  local event_name="$1"
  local release_ref="$2"
  local tag="$3"
  local release_mode="$4"
  local expected_rc="$5"
  local output_file="$fixture_dir/output"

  : >"$output_file"
  EVENT_NAME="$event_name" \
    RELEASE_REF="$release_ref" \
    RELEASE_TAG="$tag" \
    RELEASE_MODE="$release_mode" \
    GITHUB_OUTPUT="$output_file" \
    bash "$validator" >/dev/null

  grep -Fx "release_tag=$tag" "$output_file" >/dev/null
  grep -Fx "release_mode=$release_mode" "$output_file" >/dev/null
  grep -Fx "is_rc=$expected_rc" "$output_file" >/dev/null
}

expect_fail() {
  local event_name="$1"
  local release_ref="$2"
  local tag="$3"
  local release_mode="$4"

  if EVENT_NAME="$event_name" \
    RELEASE_REF="$release_ref" \
    RELEASE_TAG="$tag" \
    RELEASE_MODE="$release_mode" \
    bash "$validator" >/dev/null 2>&1; then
    echo "Expected release request to fail: ${event_name} ${release_ref} ${tag} ${release_mode}" >&2
    exit 1
  fi
}

expect_pass push refs/tags/v0.1.0 v0.1.0 dry-run false
expect_pass push refs/tags/v0.1.0-rc1 v0.1.0-rc1 dry-run true
expect_pass workflow_dispatch refs/heads/dev v0.1.0 dry-run false
expect_pass workflow_dispatch refs/tags/v0.1.0 v0.1.0 publish false
expect_pass workflow_dispatch refs/tags/v0.1.0-rc2 v0.1.0-rc2 publish true

expect_fail push refs/tags/v0.1.0 v0.1.0 publish
expect_fail push refs/tags/v0.1.1 v0.1.0 dry-run
expect_fail workflow_dispatch refs/heads/dev v0.1.0 publish
expect_fail workflow_dispatch refs/tags/v0.1.1 v0.1.0 publish
expect_fail workflow_dispatch refs/tags/v0.1.0 v0.1.0-rc1 publish
expect_fail workflow_dispatch refs/tags/v0.1.0 v0.1 dry-run
expect_fail workflow_dispatch refs/tags/v0.1.0 v0.1.0 simulate
expect_fail pull_request refs/pull/1/merge v0.1.0 dry-run

echo "Release request fixtures passed."
