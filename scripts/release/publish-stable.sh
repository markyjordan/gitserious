#!/usr/bin/env bash
set -euo pipefail

tag="${RELEASE_TAG:?RELEASE_TAG is required}"
release_mode="${RELEASE_MODE:-dry-run}"
artifact_dir="${ARTIFACT_DIR:-target/release-artifacts}"
repository="${GITHUB_REPOSITORY:-}"
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
bundle_verifier="${BUNDLE_VERIFIER:-$script_dir/verify-release-bundle.sh}"

if [[ ! "$tag" =~ ^v([0-9]+)\.([0-9]+)\.([0-9]+)$ ]]; then
  echo "Stable release tags must look like vX.Y.Z; got ${tag}." >&2
  exit 1
fi

case "$release_mode" in
  dry-run)
    echo "Dry-run mode selected; stable publishing is skipped for ${tag}."
    exit 0
    ;;
  publish) ;;
  *)
    echo "RELEASE_MODE must be dry-run or publish; got ${release_mode}." >&2
    exit 1
    ;;
esac

if [[ -n "${CRATES_IO_TOKEN:-}" && -z "${CARGO_REGISTRY_TOKEN:-}" ]]; then
  export CARGO_REGISTRY_TOKEN="$CRATES_IO_TOKEN"
fi

if [[ -z "${CARGO_REGISTRY_TOKEN:-}" ]]; then
  echo "CARGO_REGISTRY_TOKEN or CRATES_IO_TOKEN is required for publication." >&2
  exit 1
fi
if [[ -z "${GH_TOKEN:-${GITHUB_TOKEN:-}}" ]]; then
  echo "GH_TOKEN or GITHUB_TOKEN is required for publication." >&2
  exit 1
fi
if [[ -z "$repository" ]]; then
  echo "GITHUB_REPOSITORY is required for publication." >&2
  exit 1
fi

command -v python3 >/dev/null || {
  echo "python3 is required to order workspace packages." >&2
  exit 1
}
command -v gh >/dev/null || {
  echo "gh is required to publish the GitHub release." >&2
  exit 1
}
command -v curl >/dev/null || {
  echo "curl is required to verify indexed crate identity." >&2
  exit 1
}
command -v jq >/dev/null || {
  echo "jq is required to verify indexed crate identity." >&2
  exit 1
}

if [[ ! -f "$artifact_dir/SHA256SUMS" ]]; then
  echo "Release artifact checksums are required." >&2
  exit 1
fi
(
  cd "$artifact_dir"
  shasum -a 256 -c SHA256SUMS >/dev/null
)
RELEASE_TAG="$tag" RELEASE_MODE="$release_mode" ARTIFACT_DIR="$artifact_dir" \
  bash "$bundle_verifier"

if gh release view "$tag" --repo "$repository" >/dev/null 2>&1; then
  echo "GitHub release ${tag} already exists and will not be updated." >&2
  exit 1
fi

metadata_file="$(mktemp)"
package_order_file="$(mktemp)"
trap 'rm -f "$metadata_file" "$package_order_file"' EXIT
cargo metadata --locked --format-version 1 >"$metadata_file"
python3 "$script_dir/list-publishable-packages.py" "$metadata_file" >"$package_order_file"

if [[ ! -s "$package_order_file" ]]; then
  echo "No publishable workspace packages found." >&2
  exit 1
fi

package_is_indexed() {
  local package="$1"
  local version="$2"
  cargo info "${package}@${version}" >/dev/null 2>&1
}

verify_indexed_package() {
  local package="$1"
  local version="$2"
  local crate_file="target/package/${package}-${version}.crate"
  local local_checksum
  local registry_checksum

  cargo package --locked --no-verify -p "$package" >/dev/null
  [[ -f "$crate_file" ]] || {
    echo "Expected packaged crate at ${crate_file}." >&2
    return 1
  }

  local_checksum="$(shasum -a 256 "$crate_file" | awk '{print $1}')"
  registry_checksum="$(
    curl -fsSL --retry 5 --retry-all-errors \
      "https://crates.io/api/v1/crates/${package}/${version}" |
      jq -r '.version.checksum // empty'
  )"

  if [[ -z "$registry_checksum" || "$local_checksum" != "$registry_checksum" ]]; then
    echo "Indexed ${package} ${version} does not match the local package archive." >&2
    echo "Local checksum: ${local_checksum}" >&2
    echo "Registry checksum: ${registry_checksum:-<missing>}" >&2
    return 1
  fi

  echo "Verified indexed identity for ${package} ${version}."
}

wait_for_package() {
  local package="$1"
  local version="$2"

  for _ in {1..30}; do
    if package_is_indexed "$package" "$version"; then
      return 0
    fi
    sleep 10
  done

  echo "${package} ${version} did not appear in the crates.io index in time." >&2
  return 1
}

while IFS=$'\t' read -r package version; do
  [[ -n "$package" && -n "$version" ]] || continue

  if package_is_indexed "$package" "$version"; then
    verify_indexed_package "$package" "$version"
    echo "${package} ${version} is already indexed and identical; skipping upload."
    continue
  fi

  echo "Dry-running ${package} ${version}."
  cargo publish --locked --dry-run -p "$package"
  echo "Publishing ${package} ${version}."
  cargo publish --locked -p "$package"
  wait_for_package "$package" "$version"
  verify_indexed_package "$package" "$version"
done <"$package_order_file"

artifact_paths=("$artifact_dir"/*)
gh release create "$tag" "${artifact_paths[@]}" \
  --repo "$repository" \
  --verify-tag \
  --title "$tag" \
  --notes-file "$artifact_dir/release-notes.md"

echo "Published stable release ${tag}."
