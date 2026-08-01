#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

run_actionlint() {
  command -v actionlint >/dev/null
  actionlint
}

run_shellcheck() {
  command -v shellcheck >/dev/null
  find scripts -type f -name '*.sh' -print0 | xargs -0 shellcheck
}

run_release_policy_fixtures() {
  bash scripts/release/test-validate-release-request.sh
  bash scripts/release/test-validate-prepare-release-request.sh
}

component="${1:-}"
case "$component" in
  actionlint) run_actionlint ;;
  shellcheck) run_shellcheck ;;
  release-policy-fixtures) run_release_policy_fixtures ;;
  all)
    for component in actionlint shellcheck release-policy-fixtures; do
      "$0" "$component"
    done
    ;;
  *)
    echo "Usage: $0 <actionlint|shellcheck|release-policy-fixtures|all>" >&2
    exit 2
    ;;
esac
