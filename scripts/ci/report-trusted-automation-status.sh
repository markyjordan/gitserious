#!/usr/bin/env bash
set -euo pipefail

repository="${GITHUB_REPOSITORY:?GITHUB_REPOSITORY is required}"
head_sha="${PR_HEAD_SHA:?PR_HEAD_SHA is required}"
status="${TRUST_STATUS:?TRUST_STATUS is required}"
token="${GH_TOKEN:-${GITHUB_TOKEN:-}}"
target_url="${TRUST_STATUS_TARGET_URL:-}"
context="trusted-automation-review"

if [[ ! "$head_sha" =~ ^[0-9a-fA-F]{40}$ ]]; then
  echo "PR_HEAD_SHA must be a full commit SHA; got ${head_sha}." >&2
  exit 1
fi

case "$status" in
  pending)
    description="Waiting for exact-head automation approval"
    ;;
  success)
    description="Exact-head automation approval satisfied"
    ;;
  failure)
    description="Exact-head automation approval required"
    ;;
  error)
    description="Trusted automation review could not complete"
    ;;
  *)
    echo "TRUST_STATUS must be pending, success, failure, or error; got ${status}." >&2
    exit 1
    ;;
esac

if [[ -z "$token" ]]; then
  echo "GH_TOKEN or GITHUB_TOKEN is required." >&2
  exit 1
fi

command -v gh >/dev/null || {
  echo "gh is required to report trusted automation status." >&2
  exit 1
}

arguments=(
  --method POST
  "repos/${repository}/statuses/${head_sha}"
  -f "state=${status}"
  -f "context=${context}"
  -f "description=${description}"
)
if [[ -n "$target_url" ]]; then
  arguments+=(-f "target_url=${target_url}")
fi

gh api "${arguments[@]}" >/dev/null
echo "Reported ${context}=${status} for ${head_sha}."
