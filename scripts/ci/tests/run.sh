#!/usr/bin/env bash
set -euo pipefail

test_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

fixtures=(
  test-maintainer-registry.sh
  test-maintainer-signatures.sh
  test-report-trusted-automation-status.sh
  test-require-job-results.sh
  test-run-quality.sh
  test-trusted-automation-review.sh
  test-validate-ref-policy.sh
)

for fixture in "${fixtures[@]}"; do
  bash "$test_dir/$fixture"
done

echo "CI script fixtures passed."
