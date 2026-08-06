#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
release_workflow="$repo_root/.github/workflows/release.yml"
builder_workflow="$repo_root/.github/workflows/build-release-binaries.yml"
prepare_workflow="$repo_root/.github/workflows/prepare-release.yml"
homebrew_workflow="$repo_root/.github/workflows/update-homebrew-tap.yml"

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

grep -F 'actions/attest@508db95dd578ae2727ebd6217d5ba78e4fbda05d' \
  "$release_workflow" >/dev/null
grep -F 'target/release-artifacts/release-manifest.json' "$release_workflow" >/dev/null
grep -F 'run: bash scripts/release/write-release-summary.sh' "$release_workflow" >/dev/null
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
