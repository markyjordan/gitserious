#!/usr/bin/env bash
set -euo pipefail

target="${TARGET:?TARGET is required}"
archive_format="${ARCHIVE_FORMAT:-tar.gz}"
output_dir="${OUTPUT_DIR:-target/release-binaries}"
cargo_command="${CARGO:-cargo}"

case "$archive_format" in
  tar.gz | zip) ;;
  *)
    echo "ARCHIVE_FORMAT must be tar.gz or zip; got ${archive_format}." >&2
    exit 1
    ;;
esac

binary_suffix=""
if [[ "$target" == *-windows-* ]]; then
  binary_suffix=".exe"
  [[ "$archive_format" == zip ]] || {
    echo "Windows targets must use zip archives." >&2
    exit 1
  }
elif [[ "$archive_format" == zip ]]; then
  echo "Only Windows targets may use zip archives." >&2
  exit 1
fi

for required_file in README.md LICENSE-MIT LICENSE-APACHE-2.0; do
  [[ -f "$required_file" ]] || {
    echo "${required_file} is required in release archives." >&2
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
    echo "Python is required to create portable release archives." >&2
    exit 1
  fi
fi

"$cargo_command" build --locked --package gitserious --bin gitserious --release --target "$target"

binary="target/${target}/release/gitserious${binary_suffix}"
[[ -f "$binary" ]] || {
  echo "Expected release binary at ${binary}." >&2
  exit 1
}
chmod +x "$binary"
"$binary" >/dev/null

stage_dir="$(mktemp -d)"
trap 'rm -rf "$stage_dir"' EXIT
cp "$binary" "$stage_dir/gitserious${binary_suffix}"
cp README.md LICENSE-MIT LICENSE-APACHE-2.0 "$stage_dir/"

mkdir -p "$output_dir"
archive="gitserious-${target}.${archive_format}"
archive_path="${output_dir}/${archive}"
checksum_path="${archive_path}.sha256"
rm -f "$archive_path" "$checksum_path"

"$python_command" - "$stage_dir" "$archive_path" "$archive_format" <<'PY'
import os
import pathlib
import tarfile
import sys
import zipfile

stage = pathlib.Path(sys.argv[1])
archive = pathlib.Path(sys.argv[2])
archive_format = sys.argv[3]
members = sorted(stage.iterdir(), key=lambda path: path.name)

if archive_format == "tar.gz":
    with tarfile.open(archive, "w:gz", format=tarfile.PAX_FORMAT) as handle:
        for member in members:
            handle.add(member, arcname=member.name)
else:
    with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as handle:
        for member in members:
            info = zipfile.ZipInfo.from_file(member, arcname=member.name)
            if member.name == "gitserious.exe":
                info.external_attr = (0o755 & 0xFFFF) << 16
            with member.open("rb") as source:
                handle.writestr(info, source.read(), compress_type=zipfile.ZIP_DEFLATED)
PY

digest="$($python_command - "$archive_path" <<'PY'
import hashlib
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
digest = hashlib.sha256()
with path.open("rb") as handle:
    for chunk in iter(lambda: handle.read(1024 * 1024), b""):
        digest.update(chunk)
print(digest.hexdigest())
PY
)"
printf '%s  %s\n' "$digest" "$archive" >"$checksum_path"

echo "Built and exercised ${archive}."
