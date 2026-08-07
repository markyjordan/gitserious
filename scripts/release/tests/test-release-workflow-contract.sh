#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
release_workflow="$repo_root/.github/workflows/release.yml"
builder_workflow="$repo_root/.github/workflows/build-release-binaries.yml"
prepare_workflow="$repo_root/.github/workflows/prepare-release.yml"
homebrew_workflow="$repo_root/.github/workflows/update-homebrew-tap.yml"
automation_runner="$repo_root/scripts/ci/run-automation-quality.sh"

grep -F 'bash scripts/release/tests/run.sh' "$automation_runner" >/dev/null
if rg -n 'scripts/release/test-[^/[:space:]]+\.sh' \
  "$repo_root/.github/workflows" "$automation_runner" >/dev/null; then
  echo "Release tests must be invoked from scripts/release/tests/." >&2
  exit 1
fi

for fixture_reference in \
  'scripts/release/tests/test-validate-prepare-release-request.sh' \
  'scripts/release/tests/test-prepare-release.sh'; do
  grep -F "$fixture_reference" "$prepare_workflow" >/dev/null
done
grep -F 'scripts/release/tests/test-check-release.sh' \
  "$repo_root/.github/workflows/release-readiness.yml" >/dev/null
for fixture_reference in \
  'scripts/release/tests/test-validate-release-request.sh' \
  'scripts/release/tests/test-build-artifacts.sh' \
  'scripts/release/tests/test-publish-release-candidate.sh' \
  'scripts/release/tests/test-publish-stable.sh'; do
  grep -F "$fixture_reference" "$release_workflow" >/dev/null
done
for fixture_reference in \
  'scripts/release/tests/test-render-homebrew-formula.sh' \
  'scripts/release/tests/test-update-homebrew-tap.sh'; do
  grep -F "$fixture_reference" "$homebrew_workflow" >/dev/null
done

grep -F 'environment: release-branch-management' "$prepare_workflow" >/dev/null
if grep -F 'environment: release-management' "$prepare_workflow" >/dev/null; then
  echo "Prepare Release still uses the retired environment name." >&2
  exit 1
fi

tag_triggers="$(sed -n '/^  push:/,/^  workflow_dispatch:/p' "$release_workflow" |
  grep -Fc -- '- "v*.*.*"')"
[[ "$tag_triggers" == 1 ]] || {
  echo "Release workflow must use one broad tag trigger and strict request validation." >&2
  exit 1
}

for target in \
  x86_64-unknown-linux-gnu \
  x86_64-apple-darwin \
  aarch64-apple-darwin \
  x86_64-pc-windows-msvc; do
  grep -F "target: ${target}" "$builder_workflow" >/dev/null
done

grep -F 'actions/attest@1e69f48acb82d1966a394da916b4c1698aa569d6' \
  "$release_workflow" >/dev/null
grep -F 'target/release-artifacts/release-manifest.json' "$release_workflow" >/dev/null
grep -F 'run: bash scripts/release/write-release-summary.sh' "$release_workflow" >/dev/null

request_validation_line="$(
  grep -nF 'run: bash scripts/release/validate-release-request.sh' "$release_workflow" |
    head -n 1 | cut -d: -f1
)"
# shellcheck disable=SC2016
tag_verification_line="$(
  grep -nF 'run: bash scripts/ci/verify-maintainer-signature.sh tag "$RELEASE_TAG"' \
    "$release_workflow" | head -n 1 | cut -d: -f1
)"
# shellcheck disable=SC2016
tag_checkout_line="$(
  grep -nF 'ref: ${{ needs.validate.outputs.checkout_ref }}' "$release_workflow" |
    head -n 1 | cut -d: -f1
)"
if ((request_validation_line >= tag_verification_line || tag_verification_line >= tag_checkout_line)); then
  echo "Release tags must be validated and signature-verified before tagged source is checked out." >&2
  exit 1
fi

readiness_verifier_line="$(
  grep -nF 'name: Check out trusted release tag verifier' "$repo_root/.github/workflows/release-readiness.yml" |
    head -n 1 | cut -d: -f1
)"
# shellcheck disable=SC2016
readiness_signature_line="$(
  grep -nF 'run: bash scripts/ci/verify-maintainer-signature.sh tag "$RELEASE_TAG"' \
    "$repo_root/.github/workflows/release-readiness.yml" | head -n 1 | cut -d: -f1
)"
# shellcheck disable=SC2016
readiness_checkout_line="$(
  grep -nF 'ref: ${{ inputs.tag || github.ref }}' "$repo_root/.github/workflows/release-readiness.yml" |
    head -n 1 | cut -d: -f1
)"
if ((readiness_verifier_line >= readiness_signature_line || readiness_signature_line >= readiness_checkout_line)); then
  echo "Manual readiness must verify a selected tag before checking it out." >&2
  exit 1
fi

homebrew_policy_line="$(
  grep -nF 'name: Check out trusted source-release policy' "$homebrew_workflow" |
    head -n 1 | cut -d: -f1
)"
# shellcheck disable=SC2016
homebrew_signature_line="$(
  grep -nF 'run: bash scripts/ci/verify-maintainer-signature.sh tag "$RELEASE_TAG"' \
    "$homebrew_workflow" | head -n 1 | cut -d: -f1
)"
homebrew_tag_checkout_line="$(
  grep -nF 'name: Check out verified stable source' "$homebrew_workflow" |
    head -n 1 | cut -d: -f1
)"
if ((homebrew_policy_line >= homebrew_signature_line || homebrew_signature_line >= homebrew_tag_checkout_line)); then
  echo "Homebrew handoff must verify the stable tag before checking it out." >&2
  exit 1
fi

bundle_verifications="$(
  grep -Fc 'run: bash scripts/release/verify-release-bundle.sh' "$release_workflow"
)"
[[ "$bundle_verifications" == 3 ]] || {
  echo "Release assembly, RC publication, and stable publication must verify the bundle." >&2
  exit 1
}
checksum_attestations="$(grep -Fc 'target/release-artifacts/SHA256SUMS' "$release_workflow")"
[[ "$checksum_attestations" == 2 ]] || {
  echo "RC and stable publication must attest SHA256SUMS." >&2
  exit 1
}
for archive_output in \
  linux_x86_64_archive \
  macos_x86_64_archive \
  macos_aarch64_archive \
  windows_x86_64_archive; do
  grep -F "target/release-artifacts/\${{ steps.verify-release-candidate.outputs.${archive_output} }}" \
    "$release_workflow" >/dev/null || {
    echo "RC provenance must use verified ${archive_output}." >&2
    exit 1
  }
  grep -F "target/release-artifacts/\${{ steps.verify-stable.outputs.${archive_output} }}" \
    "$release_workflow" >/dev/null || {
    echo "Stable provenance must use verified ${archive_output}." >&2
    exit 1
  }
done
if grep -E 'gitserious-\*\.(tar\.gz|zip)' "$release_workflow" >/dev/null; then
  echo "Release provenance still uses a broad target archive glob." >&2
  exit 1
fi
grep -F 'uses: ./.github/workflows/update-homebrew-tap.yml' "$release_workflow" >/dev/null
homebrew_job="$(sed -n '/^  update-homebrew-tap:/,$p' "$release_workflow")"
printf '%s\n' "$homebrew_job" | grep -F 'attestations: read' >/dev/null
printf '%s\n' "$homebrew_job" | grep -F 'contents: read' >/dev/null
grep -F 'bash scripts/release/validate-homebrew-release.sh' "$homebrew_workflow" >/dev/null
grep -F -- '--json isDraft,isPrerelease,tagName,url' "$homebrew_workflow" >/dev/null
grep -F "repos/\${SOURCE_REPOSITORY}/commits/\${RELEASE_TAG}" \
  "$homebrew_workflow" >/dev/null

if rg -n -- '--clobber|gh release upload' \
  "$repo_root/scripts/release/publish-release-candidate.sh" \
  "$repo_root/scripts/release/publish-stable.sh" \
  "$release_workflow" >/dev/null; then
  echo "Release path still permits mutable asset uploads." >&2
  exit 1
fi

echo "Release workflow contract fixtures passed."
