#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/ci/maintainer-registry.sh
source "$script_dir/maintainer-registry.sh"

pr_number="${PR_NUMBER:?PR_NUMBER is required}"
head_sha="${PR_HEAD_SHA:?PR_HEAD_SHA is required}"
pr_author="${PR_AUTHOR_LOGIN:?PR_AUTHOR_LOGIN is required}"
repository="${GITHUB_REPOSITORY:?GITHUB_REPOSITORY is required}"
token="${GH_TOKEN:-${GITHUB_TOKEN:-}}"
signature_verifier="${MAINTAINER_SIGNATURE_VERIFIER:-$script_dir/verify-maintainer-signature.sh}"

if [[ ! "$pr_number" =~ ^[0-9]+$ ]]; then
  echo "PR_NUMBER must be numeric; got $pr_number." >&2
  exit 1
fi
if [[ ! "$head_sha" =~ ^[0-9a-fA-F]{40}$ ]]; then
  echo "PR_HEAD_SHA must be a full commit SHA; got $head_sha." >&2
  exit 1
fi
if [[ ! "$pr_author" =~ ^[A-Za-z0-9]([A-Za-z0-9-]{0,37}[A-Za-z0-9])?$ ]]; then
  echo "PR_AUTHOR_LOGIN is not a valid GitHub login: $pr_author" >&2
  exit 1
fi

validate_maintainer_registry
permission_rows="$(maintainer_permission_rows)"
active_logins="$(
  MAINTAINER_PERMISSION_ROWS_DATA="$permission_rows" active_maintainer_logins
)"
if [[ -z "$active_logins" ]]; then
  echo "The trusted review gate has no active registered maintainers." >&2
  exit 1
fi
active_approvers="$(printf '%s\n' "$active_logins" | paste -sd, -)"

author_is_maintainer=false
if grep -Fxi "$pr_author" <<<"$active_logins" >/dev/null; then
  author_is_maintainer=true
  MAINTAINER_PERMISSION_ROWS_DATA="$permission_rows" \
    GITHUB_REPOSITORY="$repository" PR_NUMBER="$pr_number" \
    "$signature_verifier" commit "$head_sha" "$pr_author"
fi

is_protected_change_path() {
  case "$1" in
    .github/workflows/* | .github/actions/* | .github/dependabot.yml | .github/zizmor.yml | \
      .github/maintainers/* | scripts/* | Cargo.toml | */Cargo.toml | Cargo.lock | \
      rust-toolchain.toml | clippy.toml | rustfmt.toml | justfile | Justfile | \
      .cargo/* | */.cargo/* | Dockerfile | Dockerfile.* | */Dockerfile | */Dockerfile.* | \
      Makefile | */Makefile | Taskfile.yml | Taskfile.yaml | */Taskfile.yml | */Taskfile.yaml | \
      *.rs)
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
  GH_TOKEN="$token" gh api --paginate "repos/${repository}/pulls/${pr_number}/files?per_page=100" \
    --jq '.[] | .filename, (.previous_filename // empty)'
}

load_review_rows() {
  if [[ -n "${REVIEW_ROWS_FILE:-}" ]]; then
    cat "$REVIEW_ROWS_FILE"
    return
  fi

  require_github_api
  GH_TOKEN="$token" gh api --paginate "repos/${repository}/pulls/${pr_number}/reviews?per_page=100" \
    --jq '.[] | [.user.login, .author_association, .state, .submitted_at, .commit_id] | @tsv'
}

load_attestation_rows() {
  if [[ -n "${ATTESTATION_ROWS_FILE:-}" ]]; then
    cat "$ATTESTATION_ROWS_FILE"
    return
  fi

  require_github_api
  GH_TOKEN="$token" gh api --paginate "repos/${repository}/issues/${pr_number}/comments?per_page=100" \
    --jq ".[] |
      select(.body == \"/approve-automation ${head_sha}\") |
      [.user.login, .author_association, .body, .updated_at] |
      @tsv"
}

changed_paths="$(load_changed_paths)"
protected_paths=""
while IFS= read -r path; do
  [[ -n "$path" ]] || continue
  if is_protected_change_path "$path"; then
    protected_paths="${protected_paths}${path}"$'\n'
  fi
done < <(printf '%s\n' "$changed_paths" | sed '/^$/d' | sort -u)

if [[ -z "$protected_paths" ]]; then
  echo "No protected code or supply-chain changes detected."
  exit 0
fi

echo "Protected code or supply-chain changes detected:"
printf '%s' "$protected_paths" | sed 's/^/- /'

active_count="$(grep -c . <<<"$active_logins")"
excluded_approver=""
if [[ "$author_is_maintainer" == true && "$active_count" -ge 2 ]]; then
  excluded_approver="$pr_author"
fi

trusted_approval="$({ load_review_rows || exit $?; } | awk -F '\t' \
  -v approvers="$active_approvers" -v excluded="$excluded_approver" -v head="$head_sha" '
  BEGIN {
    count = split(approvers, approver_list, ",")
    for (position = 1; position <= count; position++) {
      trusted[tolower(approver_list[position])] = 1
    }
  }
  $5 == head && trusted[tolower($1)] && tolower($1) != tolower(excluded) {
    key = tolower($1)
    if (!(key in latest_at) || $4 >= latest_at[key]) {
      latest_at[key] = $4
      latest[key] = $0
    }
  }
  END {
    for (key in latest) {
      split(latest[key], fields, "\t")
      if (fields[3] == "APPROVED") {
        print latest[key]
        exit 0
      }
    }
  }
')"

approval_kind=review
if [[ -z "$trusted_approval" ]]; then
  attestation_command="/approve-automation ${head_sha}"
  trusted_approval="$({ load_attestation_rows || exit $?; } | awk -F '\t' \
    -v approvers="$active_approvers" -v command="$attestation_command" -v excluded="$excluded_approver" '
    BEGIN {
      count = split(approvers, approver_list, ",")
      for (position = 1; position <= count; position++) {
        trusted[tolower(approver_list[position])] = 1
      }
    }
    $3 == command && trusted[tolower($1)] && tolower($1) != tolower(excluded) {
      print
      exit 0
    }
  ')"
  approval_kind=attestation
fi

if [[ -z "$trusted_approval" ]]; then
  cat >&2 <<EOF
Protected code or supply-chain changes require trusted approval on the current PR head.

Provide either:
- An approving review on ${head_sha}; or
- An exact PR comment: /approve-automation ${head_sha}

The reviewer or commenter must be active in the protected maintainer registry.
EOF
  if [[ -n "$excluded_approver" ]]; then
    echo "Because multiple maintainers are active, ${pr_author} cannot approve their own sensitive change." >&2
  fi
  cat >&2 <<EOF

Pushing a new commit intentionally invalidates approval of the previous head.
EOF
  exit 1
fi

reviewer="$(printf '%s' "$trusted_approval" | awk -F '\t' '{print $1}')"
association="$(printf '%s' "$trusted_approval" | awk -F '\t' '{print $2}')"
submitted_at="$(printf '%s' "$trusted_approval" | awk -F '\t' '{print $4}')"

echo "Protected changes approved by ${approval_kind} from ${reviewer} (${association}) at ${submitted_at} for ${head_sha}."
