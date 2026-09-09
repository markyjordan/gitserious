#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

require_tool() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "error: missing $1; install the version pinned in automation-quality.yml and add it to PATH" >&2
    return 1
  fi
}

run_actionlint() {
  require_tool actionlint
  actionlint
}

run_shellcheck() {
  require_tool shellcheck
  find scripts -type f -name '*.sh' -print0 | xargs -0 shellcheck
}

run_release_policy_fixtures() {
  bash scripts/release/tests/run.sh
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
