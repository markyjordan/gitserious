#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
release_workflow="$repo_root/.github/workflows/release.yml"
builder_workflow="$repo_root/.github/workflows/build-release-binaries.yml"

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
