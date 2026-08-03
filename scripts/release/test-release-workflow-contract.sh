#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
release_workflow="$repo_root/.github/workflows/release.yml"
builder_workflow="$repo_root/.github/workflows/build-release-binaries.yml"

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

if rg -n -- '--clobber|gh release upload' \
  "$repo_root/scripts/release/publish-release-candidate.sh" \
  "$repo_root/scripts/release/publish-stable.sh" \
  "$release_workflow" >/dev/null; then
  echo "Release path still permits mutable asset uploads." >&2
  exit 1
fi

echo "Release workflow contract fixtures passed."
