#!/usr/bin/env bash
set -euo pipefail

test_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

fixtures=(
  test-zizmor-runner.sh
  test-check-compiled-sources.sh
  test-check-dead-code.sh
  test-check-crate-dependencies.sh
  test-strict-workflows.sh
  test-prerequisite-errors.sh
  test-rust-setup-workflows.sh
  test-setup-rust-toolchain.sh
  test-rust-toolchain.sh
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

python3 "$test_dir/test-reconcile-trusted-automation-review.py"

echo "CI script fixtures passed."
