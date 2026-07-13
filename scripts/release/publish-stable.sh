#!/usr/bin/env bash
set -euo pipefail

tag="${RELEASE_TAG:?RELEASE_TAG is required}"
release_mode="${RELEASE_MODE:-dry-run}"
artifact_dir="${ARTIFACT_DIR:-target/release-artifacts}"

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

if [[ ! "$tag" =~ ^v([0-9]+)\.([0-9]+)\.([0-9]+)$ ]]; then
  echo "Stable release tags must look like vX.Y.Z; got ${tag}." >&2
  exit 1
fi

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

command -v python3 >/dev/null || {
  echo "python3 is required to order workspace packages." >&2
  exit 1
}
command -v gh >/dev/null || {
  echo "gh is required to publish the GitHub release." >&2
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

metadata_file="$(mktemp)"
package_order_file="$(mktemp)"
trap 'rm -f "$metadata_file" "$package_order_file"' EXIT
cargo metadata --locked --format-version 1 >"$metadata_file"

python3 - "$metadata_file" >"$package_order_file" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    metadata = json.load(handle)

workspace = set(metadata.get("workspace_members", []))
packages = {}
versions = {}
for package in metadata.get("packages", []):
    package_id = package["id"]
    if package_id not in workspace or package.get("source") is not None:
        continue
    if package.get("publish") == []:
        continue
    packages[package_id] = package["name"]
    versions[package_id] = package["version"]

dependencies = {package_id: set() for package_id in packages}
for node in metadata.get("resolve", {}).get("nodes", []):
    node_id = node["id"]
    if node_id not in packages:
        continue
    for dependency in node.get("dependencies", []):
        if dependency in packages:
            dependencies[node_id].add(dependency)
    for dependency in node.get("deps", []):
        dependency_id = dependency.get("pkg")
        if dependency_id in packages:
            dependencies[node_id].add(dependency_id)

ordered = []
temporary = set()
permanent = set()

def visit(package_id):
    if package_id in permanent:
        return
    if package_id in temporary:
        raise SystemExit("dependency cycle in publishable workspace packages")
    temporary.add(package_id)
    for dependency_id in sorted(dependencies[package_id], key=lambda item: packages[item]):
        visit(dependency_id)
    temporary.remove(package_id)
    permanent.add(package_id)
    ordered.append(package_id)

for package_id in sorted(packages, key=lambda item: packages[item]):
    visit(package_id)

for package_id in ordered:
    print(f"{packages[package_id]}\t{versions[package_id]}")
PY

if [[ ! -s "$package_order_file" ]]; then
  echo "No publishable workspace packages found." >&2
  exit 1
fi

package_is_indexed() {
  local package="$1"
  local version="$2"
  cargo info "${package}@${version}" >/dev/null 2>&1
}

wait_for_package() {
  local package="$1"
  local version="$2"
  local attempt

  for attempt in {1..30}; do
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
    echo "${package} ${version} is already indexed; skipping upload."
    continue
  fi

  echo "Dry-running ${package} ${version}."
  cargo publish --locked --dry-run -p "$package"
  echo "Publishing ${package} ${version}."
  cargo publish --locked -p "$package"
  wait_for_package "$package" "$version"
done <"$package_order_file"

gh release view "$tag" >/dev/null 2>&1 ||
  gh release create "$tag" --title "$tag" --notes-file "$artifact_dir/release-notes.md"
gh release upload "$tag" "$artifact_dir"/* --clobber

echo "Published stable release ${tag}."
