#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readiness_validator="${READINESS_VALIDATOR:-$script_dir/validate-release-readiness.sh}"
quality_runner="${QUALITY_RUNNER:-$script_dir/../ci/run-quality.sh}"

"$readiness_validator"

for component in check fmt lint test release; do
  "$quality_runner" "$component"
done
