#!/usr/bin/env bash
set -euo pipefail

tag="${RELEASE_TAG:?RELEASE_TAG is required}"
release_mode="${RELEASE_MODE:-dry-run}"
artifact_dir="${ARTIFACT_DIR:-target/release-artifacts}"
repository="${GITHUB_REPOSITORY:-}"

if [[ ! "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+-rc[1-9][0-9]*$ ]]; then
  echo "Release candidate tags must look like vX.Y.Z-rcN; got ${tag}." >&2
  exit 1
fi

case "$release_mode" in
  dry-run)
    echo "Dry-run mode selected; prerelease publishing is skipped for ${tag}."
    exit 0
    ;;
  publish) ;;
  *)
    echo "RELEASE_MODE must be dry-run or publish; got ${release_mode}." >&2
    exit 1
    ;;
esac

if [[ -z "${GH_TOKEN:-${GITHUB_TOKEN:-}}" ]]; then
  echo "GH_TOKEN or GITHUB_TOKEN is required for prerelease publication." >&2
  exit 1
fi
if [[ -z "$repository" ]]; then
  echo "GITHUB_REPOSITORY is required for prerelease publication." >&2
  exit 1
fi
command -v gh >/dev/null || {
  echo "gh is required to publish the GitHub prerelease." >&2
  exit 1
}

if [[ ! -f "$artifact_dir/SHA256SUMS" ]]; then
  echo "Release artifact checksums are required." >&2
  exit 1
fi
if [[ ! -f "$artifact_dir/release-notes.md" ]]; then
  echo "Release notes are required." >&2
  exit 1
fi
(
  cd "$artifact_dir"
  shasum -a 256 -c SHA256SUMS >/dev/null
)

gh release view "$tag" --repo "$repository" >/dev/null 2>&1 ||
  gh release create "$tag" --repo "$repository" --prerelease --title "$tag" \
    --notes-file "$artifact_dir/release-notes.md"
gh release upload "$tag" "$artifact_dir"/* --repo "$repository" --clobber

echo "Published release candidate ${tag}."
