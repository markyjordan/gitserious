#!/usr/bin/env bash
set -euo pipefail

version_family="${VERSION_FAMILY:?VERSION_FAMILY is required}"
base_ref="${BASE_REF:-main}"
repository="${GITHUB_REPOSITORY:?GITHUB_REPOSITORY is required}"
actor="${GITHUB_ACTOR:-unknown}"
token="${GH_TOKEN:-${GITHUB_TOKEN:-}}"

if [[ "$base_ref" != "main" ]]; then
  echo "Release branches must be cut from main; got ${base_ref}." >&2
  exit 1
fi

if [[ ! "$version_family" =~ ^[0-9]+\.[0-9]+$ ]]; then
  echo "VERSION_FAMILY must look like 0.1; got ${version_family}." >&2
  exit 1
fi

if [[ -z "$token" ]]; then
  echo "GH_TOKEN or GITHUB_TOKEN is required." >&2
  exit 1
fi

command -v gh >/dev/null || {
  echo "gh is required to create the protected release ref." >&2
  exit 1
}

release_branch="release/${version_family}"
release_ref="refs/heads/${release_branch}"

git fetch --no-tags origin "+refs/heads/${base_ref}:refs/remotes/origin/${base_ref}"
base_sha="$(git rev-parse "refs/remotes/origin/${base_ref}^{commit}")"
checkout_sha="$(git rev-parse 'HEAD^{commit}')"

if [[ "$checkout_sha" != "$base_sha" ]]; then
  echo "Checked-out ${base_ref} moved during preparation; rerun from ${base_sha}." >&2
  exit 1
fi

if gh api "repos/${repository}/git/ref/heads/${release_branch}" >/dev/null 2>&1; then
  echo "Release branch already exists: ${release_branch}" >&2
  exit 1
fi

gh api --method POST "repos/${repository}/git/refs" \
  -f ref="$release_ref" \
  -f sha="$base_sha" >/dev/null

echo "Created ${release_branch} from ${base_ref} at ${base_sha} for ${version_family} by ${actor}."
