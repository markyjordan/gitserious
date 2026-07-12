#!/usr/bin/env bash
set -euo pipefail

event_name="${EVENT_NAME:-${GITHUB_EVENT_NAME:-}}"
base_ref="${BASE_REF:-${GITHUB_BASE_REF:-}}"
head_ref="${HEAD_REF:-${GITHUB_HEAD_REF:-}}"
head_repository="${HEAD_REPOSITORY:-}"
repository="${REPOSITORY:-${GITHUB_REPOSITORY:-}}"
ref_name="${REF_NAME:-${GITHUB_REF_NAME:-}}"
before_sha="${BEFORE_SHA:-}"
current_sha="${CURRENT_SHA:-${GITHUB_SHA:-HEAD}}"

fail() {
  echo "Ref policy rejected this event: $*" >&2
  exit 1
}

require_merge_commit() {
  local commit="$1"
  local parent_line
  local field_count

  git cat-file -e "${commit}^{commit}" 2>/dev/null ||
    fail "commit ${commit} is unavailable; checkout at least two commits"

  parent_line="$(git rev-list --parents -n 1 "$commit")"
  read -r -a fields <<<"$parent_line"
  field_count="${#fields[@]}"

  if ((field_count < 3)); then
    fail "protected branch updates after creation must be merge commits"
  fi
}

case "$event_name" in
  pull_request)
    [[ -n "$base_ref" ]] || fail "pull request base branch is missing"
    [[ -n "$head_ref" ]] || fail "pull request source branch is missing"

    case "$base_ref" in
      dev)
        case "$head_ref" in
          dev | main | release/* | archive/*)
            fail "pull requests into dev must come from a focused topic branch"
            ;;
        esac
        ;;
      main)
        [[ "$head_ref" == "dev" ]] ||
          fail "pull requests into main must come from dev"
        [[ -n "$repository" && "$head_repository" == "$repository" ]] ||
          fail "pull requests into main must come from this repository's dev branch"
        ;;
      release/*)
        case "$head_ref" in
          fix/* | hotfix/* | release-fix/*) ;;
          *)
            fail "pull requests into release branches must use fix/, hotfix/, or release-fix/"
            ;;
        esac
        ;;
      *)
        fail "unsupported pull request target: $base_ref"
        ;;
    esac
    ;;
  push)
    [[ -n "$ref_name" ]] || fail "push branch is missing"

    case "$ref_name" in
      dev)
        # This is post-merge evidence. Repository rules must prevent direct pushes.
        ;;
      main)
        require_merge_commit "$current_sha"
        ;;
      release/*)
        if [[ ! "$before_sha" =~ ^0+$ ]]; then
          require_merge_commit "$current_sha"
        fi
        ;;
      *)
        fail "unsupported protected-branch push: $ref_name"
        ;;
    esac
    ;;
  *)
    fail "unsupported event: ${event_name:-<empty>}"
    ;;
esac

echo "Ref policy accepted ${event_name} for ${base_ref:-$ref_name}."
