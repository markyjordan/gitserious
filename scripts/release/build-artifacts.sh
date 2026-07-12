#!/usr/bin/env bash
set -euo pipefail

tag="${RELEASE_TAG:?RELEASE_TAG is required}"
release_mode="${RELEASE_MODE:-dry-run}"
artifact_dir="${ARTIFACT_DIR:-target/release-artifacts}"

if [[ ! "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-rc[1-9][0-9]*)?$ ]]; then
  echo "RELEASE_TAG must be a stable or release-candidate tag; got ${tag}." >&2
  exit 1
fi

case "$release_mode" in
  dry-run | publish) ;;
  *)
    echo "RELEASE_MODE must be dry-run or publish; got ${release_mode}." >&2
    exit 1
    ;;
esac

if [[ ! -f CHANGELOG.md ]]; then
  echo "CHANGELOG.md is required to build release artifacts." >&2
  exit 1
fi

rm -rf "$artifact_dir"
mkdir -p "$artifact_dir"

cargo package --locked --workspace --list >"$artifact_dir/package-files.txt"
cargo build --locked --workspace --all-targets --all-features --release

binary="target/release/gitserious"
if [[ ! -x "$binary" ]]; then
  echo "Expected release binary at ${binary}." >&2
  exit 1
fi

cp "$binary" "$artifact_dir/${tag}-gitserious"
cp CHANGELOG.md "$artifact_dir/CHANGELOG.md"

created_at="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
dry_run=true
publish_operations_enabled=false
if [[ "$release_mode" == "publish" ]]; then
  dry_run=false
  publish_operations_enabled=true
fi

cat >"$artifact_dir/release-plan.json" <<EOF
{
  "release_tag": "${tag}",
  "release_mode": "${release_mode}",
  "dry_run": ${dry_run},
  "created_at": "${created_at}",
  "publish_operations_enabled": ${publish_operations_enabled}
}
EOF

(
  cd "$artifact_dir"
  shasum -a 256 CHANGELOG.md package-files.txt release-plan.json "${tag}-gitserious" >SHA256SUMS
)

echo "Built ${release_mode} release artifacts in ${artifact_dir}."
