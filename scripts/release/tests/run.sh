#!/usr/bin/env bash
set -euo pipefail

test_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

fixtures=(
  test-build-artifacts.sh
  test-build-release-binary.sh
  test-check-release.sh
  test-prepare-release.sh
  test-publish-release-candidate.sh
  test-publish-stable.sh
  test-release-workflow-contract.sh
  test-render-homebrew-formula.sh
  test-update-homebrew-tap.sh
  test-validate-homebrew-release.sh
  test-validate-prepare-release-request.sh
  test-validate-release-request.sh
  test-verify-release-bundle.sh
  test-write-release-summary.sh
)

for fixture in "${fixtures[@]}"; do
  bash "$test_dir/$fixture"
done

echo "Release script fixtures passed."
