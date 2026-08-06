#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
validator="$script_dir/require-job-results.sh"

expect_failure() {
  local description="$1"
  local needs_json="$2"
  shift 2

  if NEEDS_JSON="$needs_json" "$validator" "$@" >/dev/null 2>&1; then
    echo "Expected aggregate validation to reject: $description" >&2
    exit 1
  fi
}

NEEDS_JSON='{"check":{"result":"success"},"test":{"result":"success"}}' \
  "$validator" >/dev/null

expect_failure \
  "failure" \
  '{"check":{"result":"failure"}}'
expect_failure \
  "cancellation" \
  '{"check":{"result":"cancelled"}}'
expect_failure \
  "unlisted skip" \
  '{"optional":{"result":"skipped"}}'

NEEDS_JSON='{"check":{"result":"success"},"optional":{"result":"skipped"}}' \
  "$validator" optional >/dev/null

expect_failure \
  "unknown allowed-skip job" \
  '{"check":{"result":"success"}}' \
  optional

echo "Aggregate result fixtures passed."
