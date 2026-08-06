#!/usr/bin/env bash
set -euo pipefail

tag="${RELEASE_TAG:?RELEASE_TAG is required}"
release_mode="${RELEASE_MODE:-dry-run}"
binary_dir="${BINARY_ARTIFACT_DIR:-target/release-binaries}"
artifact_dir="${ARTIFACT_DIR:-target/release-artifacts}"

case "$release_mode" in
  dry-run | publish) ;;
  *)
    echo "RELEASE_MODE must be dry-run or publish; got ${release_mode}." >&2
    exit 1
    ;;
esac

if [[ "$tag" == dry-run ]]; then
  [[ "$release_mode" == dry-run ]] || {
    echo "The dry-run sentinel cannot be assembled in publish mode." >&2
    exit 1
  }
elif [[ ! "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-rc[1-9][0-9]*)?$ ]]; then
  echo "RELEASE_TAG must be dry-run, vX.Y.Z, or vX.Y.Z-rcN; got ${tag}." >&2
  exit 1
fi

for required_file in CHANGELOG.md Cargo.toml Cargo.lock rust-toolchain.toml; do
  [[ -f "$required_file" ]] || {
    echo "${required_file} is required to assemble release artifacts." >&2
    exit 1
  }
done

python_command="${PYTHON:-}"
if [[ -z "$python_command" ]]; then
  if command -v python3 >/dev/null; then
    python_command=python3
  elif command -v python >/dev/null; then
    python_command=python
  else
    echo "Python is required to assemble release artifacts." >&2
    exit 1
  fi
fi

metadata_file="$(mktemp)"
target_file="$(mktemp)"
trap 'rm -f "$metadata_file" "$target_file"' EXIT
cargo metadata --locked --no-deps --format-version 1 >"$metadata_file"
version="$($python_command - "$metadata_file" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    metadata = json.load(handle)
workspace = set(metadata.get("workspace_members", []))
versions = sorted({
    package["version"]
    for package in metadata.get("packages", [])
    if package.get("id") in workspace and package.get("source") is None
})
if len(versions) != 1:
    raise SystemExit(f"expected one workspace version, got: {versions}")
print(versions[0])
PY
)"

if [[ "$tag" != dry-run ]]; then
  tag_version="${tag#v}"
  tag_version="${tag_version%%-rc*}"
  [[ "$tag_version" == "$version" ]] || {
    echo "Tag version ${tag_version} does not match workspace version ${version}." >&2
    exit 1
  }
fi

artifact_version="$version"
if [[ "$tag" != dry-run ]]; then
  artifact_version="${tag#v}"
fi

rm -rf "$artifact_dir"
mkdir -p "$artifact_dir"

targets=(
  x86_64-unknown-linux-gnu
  x86_64-apple-darwin
  aarch64-apple-darwin
  x86_64-pc-windows-msvc
)

for target in "${targets[@]}"; do
  extension=tar.gz
  if [[ "$target" == *-windows-* ]]; then
    extension=zip
  fi
  input_archive="gitserious-${target}.${extension}"
  public_archive="gitserious-${artifact_version}-${target}.${extension}"
  count="$(find "$binary_dir" -type f -name "$input_archive" | wc -l | tr -d ' ')"
  [[ "$count" == 1 ]] || {
    echo "Expected exactly one ${input_archive} below ${binary_dir}; found ${count}." >&2
    exit 1
  }
  archive_path="$(find "$binary_dir" -type f -name "$input_archive" -print | head -1)"
  checksum_count="$(find "$binary_dir" -type f -name "${input_archive}.sha256" | wc -l | tr -d ' ')"
  [[ "$checksum_count" == 1 ]] || {
    echo "Expected exactly one ${input_archive}.sha256 below ${binary_dir}; found ${checksum_count}." >&2
    exit 1
  }
  checksum_path="$(find "$binary_dir" -type f -name "${input_archive}.sha256" -print | head -1)"

  digest="$($python_command - "$archive_path" "$checksum_path" "$input_archive" <<'PY'
import hashlib
import pathlib
import re
import sys

archive_path = pathlib.Path(sys.argv[1])
checksum_path = pathlib.Path(sys.argv[2])
expected_name = sys.argv[3]
line = checksum_path.read_text(encoding="utf-8").strip()
match = re.fullmatch(r"([0-9a-f]{64}) [ *](.+)", line)
if not match or match.group(2) != expected_name:
    raise SystemExit(f"invalid checksum record in {checksum_path}")
digest = hashlib.sha256(archive_path.read_bytes()).hexdigest()
if digest != match.group(1):
    raise SystemExit(f"checksum mismatch for {expected_name}")
print(digest)
PY
)"
  cp "$archive_path" "$artifact_dir/$public_archive"
  printf '%s  %s\n' "$digest" "$public_archive" \
    >"$artifact_dir/${public_archive}.sha256"
  printf '%s\t%s\t%s\n' "$target" "$public_archive" "$digest" >>"$target_file"
done

cp CHANGELOG.md "$artifact_dir/CHANGELOG.md"
cargo package --locked --workspace --list >"$artifact_dir/package-files.txt"

awk -v heading="## [${version}]" '
  index($0, heading) == 1 { capture = 1 }
  capture && printed && /^## \[/ { exit }
  capture { print; printed = 1 }
  END { if (!capture) exit 1 }
' CHANGELOG.md >"$artifact_dir/release-notes.md" || {
  echo "CHANGELOG.md does not contain release notes for ${version}." >&2
  exit 1
}

source_archive="gitserious-${artifact_version}-source.tar.gz"
git archive --format=tar.gz --prefix="gitserious-${artifact_version}/" HEAD \
  >"$artifact_dir/$source_archive"

source_commit="$(git rev-parse HEAD)"
rust_toolchain="$(awk -F'"' '$1 ~ /^[[:space:]]*channel[[:space:]]*=/ { print $2; exit }' rust-toolchain.toml)"
[[ -n "$rust_toolchain" ]] || {
  echo "Could not determine the locked Rust toolchain." >&2
  exit 1
}
created_at="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
source_digest="$($python_command - "$artifact_dir/$source_archive" <<'PY'
import hashlib
import pathlib
import sys

print(hashlib.sha256(pathlib.Path(sys.argv[1]).read_bytes()).hexdigest())
PY
)"

"$python_command" - "$target_file" "$artifact_dir/release-manifest.json" <<PY
import json
import pathlib
import sys

targets = []
with open(sys.argv[1], encoding="utf-8") as handle:
    for line in handle:
        target, filename, digest = line.rstrip("\n").split("\t")
        targets.append({"target": target, "filename": filename, "sha256": digest})

manifest = {
    "release_tag": "${tag}",
    "release_mode": "${release_mode}",
    "source_commit": "${source_commit}",
    "workspace_version": "${version}",
    "rust_toolchain": "${rust_toolchain}",
    "created_at": "${created_at}",
    "targets": targets,
    "source_archive": {
        "filename": "${source_archive}",
        "sha256": "${source_digest}",
    },
}
pathlib.Path(sys.argv[2]).write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
PY

"$python_command" - "$artifact_dir" <<'PY'
import hashlib
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
records = []
for path in sorted(root.iterdir(), key=lambda item: item.name):
    if path.is_file() and path.name != "SHA256SUMS":
        records.append(f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}")
(root / "SHA256SUMS").write_text("\n".join(records) + "\n", encoding="utf-8")
PY

echo "Assembled ${release_mode} release bundle in ${artifact_dir}."
