#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
quality_runner="${QUALITY_RUNNER:-$script_dir/../ci/run-rust-quality.sh}"

required_files=(
  Cargo.toml
  Cargo.lock
  README.md
  CHANGELOG.md
  LICENSE-MIT
  LICENSE-APACHE-2.0
)

for required_file in "${required_files[@]}"; do
  if [[ ! -f "$required_file" ]]; then
    echo "$required_file is required for release readiness." >&2
    exit 1
  fi
done

command -v jq >/dev/null || {
  echo "jq is required to inspect Cargo metadata." >&2
  exit 1
}

release_mode="${RELEASE_MODE:-dry-run}"
case "$release_mode" in
  dry-run | publish) ;;
  *)
    echo "RELEASE_MODE must be dry-run or publish; got $release_mode." >&2
    exit 1
    ;;
esac

tag="${RELEASE_TAG:-${GITHUB_REF_NAME:-}}"
if [[ -n "$tag" && "$tag" =~ ^v([0-9]+)\.([0-9]+)\.([0-9]+)(-rc([1-9][0-9]*))?$ ]]; then
  major="${BASH_REMATCH[1]}"
  minor="${BASH_REMATCH[2]}"
  patch="${BASH_REMATCH[3]}"
  rc_suffix="${BASH_REMATCH[4]:-}"
  version="${major}.${minor}.${patch}"
  release_branch="release/${major}.${minor}"

  git fetch --no-tags origin "refs/tags/${tag}:refs/tags/${tag}" || {
    echo "Expected release tag ${tag} on origin." >&2
    exit 1
  }
  git fetch --no-tags origin "+refs/heads/${release_branch}:refs/remotes/origin/${release_branch}" || {
    echo "Expected release branch origin/${release_branch} for ${tag}." >&2
    exit 1
  }

  tag_commit="$(git rev-parse "refs/tags/${tag}^{commit}")"
  release_head="$(git rev-parse "refs/remotes/origin/${release_branch}^{commit}")"

  if [[ "$tag_commit" != "$release_head" ]]; then
    echo "Release tag ${tag} must point at the current ${release_branch} head." >&2
    echo "Tag commit: ${tag_commit}" >&2
    echo "Release head: ${release_head}" >&2
    exit 1
  fi

  workspace_versions="$(
    cargo metadata --locked --no-deps --format-version 1 |
      jq -r '[.packages[] | select(.source == null) | .version] | unique | .[]'
  )"
  workspace_version_count="$(
    printf '%s\n' "$workspace_versions" |
      sed '/^$/d' |
      wc -l |
      tr -d ' '
  )"

  if [[ "$workspace_version_count" != "1" ]]; then
    echo "Expected one workspace version, got: ${workspace_versions}" >&2
    exit 1
  fi

  if [[ "$workspace_versions" != "$version" ]]; then
    echo "Workspace version ${workspace_versions} does not match tag version ${version}." >&2
    exit 1
  fi

  if ! grep -F "## [${version}]" CHANGELOG.md >/dev/null; then
    echo "CHANGELOG.md must contain a section for ${version}." >&2
    exit 1
  fi

  if [[ -n "$rc_suffix" ]]; then
    echo "Validated release candidate ${tag} from ${release_branch}."
  else
    echo "Validated stable release ${tag} from ${release_branch}."
  fi
elif [[ -n "$tag" && "$tag" != "main" && "$tag" != "dev" && "$tag" != release/* ]]; then
  echo "Unsupported release tag or ref: $tag" >&2
  exit 1
fi

for component in check fmt lint test release; do
  "$quality_runner" "$component"
done
