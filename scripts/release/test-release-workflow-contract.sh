#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
release_workflow="$repo_root/.github/workflows/release.yml"
builder_workflow="$repo_root/.github/workflows/build-release-binaries.yml"
prepare_workflow="$repo_root/.github/workflows/prepare-release.yml"

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
for target_archive in \
  gitserious-x86_64-unknown-linux-gnu.tar.gz \
  gitserious-x86_64-apple-darwin.tar.gz \
  gitserious-aarch64-apple-darwin.tar.gz \
  gitserious-x86_64-pc-windows-msvc.zip; do
  [[ "$(grep -Fc "target/release-artifacts/${target_archive}" "$release_workflow")" == 2 ]] || {
    echo "RC and stable provenance must name ${target_archive} explicitly." >&2
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

if rg -n -- '--clobber|gh release upload' \
  "$repo_root/scripts/release/publish-release-candidate.sh" \
  "$repo_root/scripts/release/publish-stable.sh" \
  "$release_workflow" >/dev/null; then
  echo "Release path still permits mutable asset uploads." >&2
  exit 1
fi

echo "Release workflow contract fixtures passed."
