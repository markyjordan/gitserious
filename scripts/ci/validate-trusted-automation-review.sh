#!/usr/bin/env bash
set -euo pipefail

pr_number="${PR_NUMBER:?PR_NUMBER is required}"
head_sha="${PR_HEAD_SHA:?PR_HEAD_SHA is required}"
repository="${GITHUB_REPOSITORY:?GITHUB_REPOSITORY is required}"
token="${GH_TOKEN:-${GITHUB_TOKEN:-}}"

if [[ ! "$pr_number" =~ ^[0-9]+$ ]]; then
  echo "PR_NUMBER must be numeric; got $pr_number." >&2
  exit 1
fi

if [[ ! "$head_sha" =~ ^[0-9a-fA-F]{40}$ ]]; then
  echo "PR_HEAD_SHA must be a full commit SHA; got $head_sha." >&2
  exit 1
fi

is_protected_automation_path() {
  case "$1" in
    .github/workflows/* | .github/actions/* | .github/dependabot.yml | .github/zizmor.yml | \
      scripts/ci/* | scripts/release/* | scripts/archive/* | scripts/security/*)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

require_github_api() {
  if [[ -z "$token" ]]; then
    echo "GH_TOKEN or GITHUB_TOKEN is required." >&2
    exit 1
  fi

  command -v gh >/dev/null || {
    echo "gh is required to query pull request metadata." >&2
    exit 1
  }
}

load_changed_paths() {
  if [[ -n "${CHANGED_PATHS_FILE:-}" ]]; then
    cat "$CHANGED_PATHS_FILE"
    return
  fi

  require_github_api
  gh api --paginate "repos/${repository}/pulls/${pr_number}/files?per_page=100" \
    --jq '.[] | .filename, (.previous_filename // empty)'
}

load_review_rows() {
  if [[ -n "${REVIEW_ROWS_FILE:-}" ]]; then
    cat "$REVIEW_ROWS_FILE"
    return
  fi

  require_github_api
  gh api --paginate "repos/${repository}/pulls/${pr_number}/reviews?per_page=100" \
    --jq '.[] | [.user.login, .author_association, .state, .submitted_at, .commit_id] | @tsv'
}

load_attestation_rows() {
  if [[ -n "${ATTESTATION_ROWS_FILE:-}" ]]; then
    cat "$ATTESTATION_ROWS_FILE"
    return
  fi

  require_github_api
  gh api --paginate "repos/${repository}/issues/${pr_number}/comments?per_page=100" \
    --jq ".[] |
      select(.body == \"/approve-automation ${head_sha}\") |
      [.user.login, .author_association, .body, .updated_at] |
      @tsv"
}

changed_paths="$(load_changed_paths)"
protected_paths=""
while IFS= read -r path; do
  [[ -n "$path" ]] || continue
  if is_protected_automation_path "$path"; then
    protected_paths="${protected_paths}${path}"$'\n'
  fi
done < <(printf '%s\n' "$changed_paths" | sed '/^$/d' | sort -u)

if [[ -z "$protected_paths" ]]; then
  echo "No protected automation changes detected."
  exit 0
fi

echo "Protected automation changes detected:"
printf '%s' "$protected_paths" | sed 's/^/- /'

trusted_approval="$({ load_review_rows || exit $?; } | awk -F '\t' -v head="$head_sha" '
  $5 == head && ($2 == "OWNER" || $2 == "MEMBER" || $2 == "COLLABORATOR") {
    latest[$1] = $0
  }
  END {
    for (login in latest) {
      split(latest[login], fields, "\t")
      if (fields[3] == "APPROVED") {
        print latest[login]
        exit 0
      }
    }
  }
')"

approval_kind=review
if [[ -z "$trusted_approval" ]]; then
  attestation_command="/approve-automation ${head_sha}"
  trusted_approval="$({ load_attestation_rows || exit $?; } | awk -F '\t' -v command="$attestation_command" '
    $3 == command && ($2 == "OWNER" || $2 == "MEMBER" || $2 == "COLLABORATOR") {
      print
      exit 0
    }
  ')"
  approval_kind=attestation
fi

if [[ -z "$trusted_approval" ]]; then
  cat >&2 <<EOF
Protected automation changes require trusted approval on the current PR head.

Provide either:
- An approving review on ${head_sha}; or
- An exact PR comment: /approve-automation ${head_sha}

The reviewer or commenter must be an OWNER, MEMBER, or COLLABORATOR.

Pushing a new commit intentionally invalidates approval of the previous head.
EOF
  exit 1
fi

reviewer="$(printf '%s' "$trusted_approval" | awk -F '\t' '{print $1}')"
association="$(printf '%s' "$trusted_approval" | awk -F '\t' '{print $2}')"
submitted_at="$(printf '%s' "$trusted_approval" | awk -F '\t' '{print $4}')"

echo "Protected automation approved by ${approval_kind} from ${reviewer} (${association}) at ${submitted_at} for ${head_sha}."
