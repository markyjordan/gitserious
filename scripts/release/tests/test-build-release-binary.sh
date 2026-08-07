#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
builder="$script_dir/build-release-binary.sh"
fixture_dir="$(mktemp -d)"
trap 'rm -rf "$fixture_dir"' EXIT

fake_bin="$fixture_dir/bin"
repo="$fixture_dir/repo"
mkdir -p "$fake_bin" "$repo"
for file in README.md LICENSE-MIT LICENSE-APACHE-2.0; do
  printf '%s\n' "$file" >"$repo/$file"
done

cat >"$fake_bin/cargo" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
target=""
while (($#)); do
  if [[ "$1" == --target ]]; then
    target="$2"
    break
  fi
  shift
done
[[ -n "$target" ]] || exit 2
suffix=""
[[ "$target" == *-windows-* ]] && suffix=.exe
mkdir -p "target/${target}/release"
printf '%s\n' '#!/usr/bin/env bash' 'echo gitserious fixture' >"target/${target}/release/gitserious${suffix}"
chmod +x "target/${target}/release/gitserious${suffix}"
EOF
chmod +x "$fake_bin/cargo"

(
  cd "$repo"
  env CARGO="$fake_bin/cargo" TARGET=x86_64-unknown-linux-gnu \
    ARCHIVE_FORMAT=tar.gz OUTPUT_DIR=out bash "$builder" >/dev/null
  env CARGO="$fake_bin/cargo" TARGET=x86_64-pc-windows-msvc \
    ARCHIVE_FORMAT=zip OUTPUT_DIR=out bash "$builder" >/dev/null
)

for archive in \
  gitserious-x86_64-unknown-linux-gnu.tar.gz \
  gitserious-x86_64-pc-windows-msvc.zip; do
  [[ -f "$repo/out/$archive" && -f "$repo/out/${archive}.sha256" ]] || {
    echo "Missing native archive or checksum: ${archive}" >&2
    exit 1
  }
  (
    cd "$repo/out"
    shasum -a 256 -c "${archive}.sha256" >/dev/null
  )
done

tar -tzf "$repo/out/gitserious-x86_64-unknown-linux-gnu.tar.gz" | sort >"$fixture_dir/tar-files"
expected_tar=$'LICENSE-APACHE-2.0\nLICENSE-MIT\nREADME.md\ngitserious'
[[ "$(cat "$fixture_dir/tar-files")" == "$expected_tar" ]] || {
  echo "Unix archive layout is incorrect." >&2
  exit 1
}

python3 - "$repo/out/gitserious-x86_64-pc-windows-msvc.zip" >"$fixture_dir/zip-files" <<'PY'
import sys
import zipfile

with zipfile.ZipFile(sys.argv[1]) as archive:
    print("\n".join(sorted(archive.namelist())))
PY
expected_zip=$'LICENSE-APACHE-2.0\nLICENSE-MIT\nREADME.md\ngitserious.exe'
[[ "$(cat "$fixture_dir/zip-files")" == "$expected_zip" ]] || {
  echo "Windows archive layout is incorrect." >&2
  exit 1
}

if (
  cd "$repo"
  env CARGO="$fake_bin/cargo" TARGET=x86_64-unknown-linux-gnu \
    ARCHIVE_FORMAT=zip OUTPUT_DIR=out bash "$builder" >/dev/null 2>&1
); then
  echo "Native builder accepted a zip archive for a non-Windows target." >&2
  exit 1
fi

echo "Native binary archive fixtures passed."
