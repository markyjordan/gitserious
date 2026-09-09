#!/usr/bin/env bash
set -euo pipefail

expected_version=1.30.0
mode=online
if (($#)); then
  if [[ $# != 1 || "$1" != --offline ]]; then
    echo "Usage: $0 [--offline]" >&2
    exit 2
  fi
  mode=offline
fi
if ! command -v zizmor >/dev/null 2>&1; then
  echo "error: install zizmor $expected_version and add it to PATH" >&2
  exit 1
fi
actual_version="$(zizmor --version)"
if [[ "$actual_version" != "zizmor $expected_version" ]]; then
  echo "error: expected zizmor $expected_version; got $actual_version" >&2
  exit 1
fi
arguments=(--pedantic --min-confidence medium --min-severity medium --collect all)
if [[ "$mode" == offline ]]; then
  echo "zizmor $expected_version: OFFLINE / PARTIAL; online audits are excluded" >&2
  arguments+=(--offline)
else
  export GH_TOKEN="${GH_TOKEN:-${GITHUB_TOKEN:-}}"
  if [[ -z "$GH_TOKEN" ]]; then
    echo "error: authenticated audit requires GH_TOKEN or GITHUB_TOKEN; use --offline only for partial verification" >&2
    exit 1
  fi
  echo "zizmor $expected_version: authenticated ONLINE audit" >&2
fi
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"
zizmor "${arguments[@]}" --config .github/zizmor.yml .
