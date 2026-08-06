#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
builder="$script_dir/build-artifacts.sh"
fixture_dir="$(mktemp -d)"
trap 'rm -rf "$fixture_dir"' EXIT

fake_bin="$fixture_dir/bin"
repo="$fixture_dir/repo"
binary_dir="$repo/native"
mkdir -p "$fake_bin" "$repo" "$binary_dir"

cat >"$fake_bin/cargo" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
case "${1:-}" in
  metadata)
    printf '%s\n' '{"workspace_members":["gitserious 0.1.0"],"packages":[{"id":"gitserious 0.1.0","version":"0.1.0","source":null}]}'
    ;;
  package)
    printf '%s\n' Cargo.toml Cargo.lock src/main.rs
    ;;
  *) exit 2 ;;
esac
EOF
chmod +x "$fake_bin/cargo"

printf '%s\n' '[workspace]' >"$repo/Cargo.toml"
: >"$repo/Cargo.lock"
printf '%s\n' '[toolchain]' 'channel = "1.96.0"' >"$repo/rust-toolchain.toml"
printf '%s\n' '# Changelog' '' '## [0.1.0] - TBD' '' '- Fixture release.' >"$repo/CHANGELOG.md"
git -C "$repo" init -q -b main
git -C "$repo" config user.name Fixture
git -C "$repo" config user.email fixture@example.invalid
git -C "$repo" add .
git -C "$repo" commit -qm "fixture source"

targets=(
  x86_64-unknown-linux-gnu.tar.gz
  x86_64-apple-darwin.tar.gz
  aarch64-apple-darwin.tar.gz
  x86_64-pc-windows-msvc.zip
)
for target in "${targets[@]}"; do
  archive="gitserious-${target}"
  printf '%s\n' "$archive" >"$binary_dir/$archive"
  (
    cd "$binary_dir"
    shasum -a 256 "$archive" >"${archive}.sha256"
  )
done

(
  cd "$repo"
  env PATH="$fake_bin:$PATH" RELEASE_TAG=dry-run RELEASE_MODE=dry-run \
    BINARY_ARTIFACT_DIR="$binary_dir" bash "$builder" >/dev/null
)

artifact_dir="$repo/target/release-artifacts"
for artifact in CHANGELOG.md package-files.txt release-notes.md release-manifest.json \
  SHA256SUMS gitserious-0.1.0-source.tar.gz; do
  [[ -f "$artifact_dir/$artifact" ]] || {
    echo "Missing release artifact: ${artifact}" >&2
    exit 1
  }
done
for target in "${targets[@]}"; do
  [[ -f "$artifact_dir/gitserious-0.1.0-${target}" ]] || exit 1
  [[ -f "$artifact_dir/gitserious-0.1.0-${target}.sha256" ]] || exit 1
done

[[ "$(jq -r '.release_tag' "$artifact_dir/release-manifest.json")" == dry-run ]] || exit 1
[[ "$(jq -r '.workspace_version' "$artifact_dir/release-manifest.json")" == 0.1.0 ]] || exit 1
[[ "$(jq -r '.rust_toolchain' "$artifact_dir/release-manifest.json")" == 1.96.0 ]] || exit 1
[[ "$(jq -r '.targets | length' "$artifact_dir/release-manifest.json")" == 4 ]] || exit 1
[[ "$(jq -r '.source_commit' "$artifact_dir/release-manifest.json")" == "$(git -C "$repo" rev-parse HEAD)" ]] || exit 1
(
  cd "$artifact_dir"
  shasum -a 256 -c SHA256SUMS >/dev/null
)

(
  cd "$repo"
  env PATH="$fake_bin:$PATH" RELEASE_TAG=v0.1.0-rc1 RELEASE_MODE=publish \
    BINARY_ARTIFACT_DIR="$binary_dir" bash "$builder" >/dev/null
)
for target in "${targets[@]}"; do
  [[ -f "$artifact_dir/gitserious-0.1.0-rc1-${target}" ]] || exit 1
  [[ -f "$artifact_dir/gitserious-0.1.0-rc1-${target}.sha256" ]] || exit 1
done
[[ -f "$artifact_dir/gitserious-0.1.0-rc1-source.tar.gz" ]] || exit 1
[[ ! -e "$artifact_dir/gitserious-0.1.0-source.tar.gz" ]] || exit 1
[[ "$(jq -r '.targets[0].filename' "$artifact_dir/release-manifest.json")" == \
  gitserious-0.1.0-rc1-x86_64-unknown-linux-gnu.tar.gz ]] || exit 1
[[ "$(jq -r '.source_archive.filename' "$artifact_dir/release-manifest.json")" == \
  gitserious-0.1.0-rc1-source.tar.gz ]] || exit 1

printf '%s\n' corrupt >>"$binary_dir/gitserious-x86_64-unknown-linux-gnu.tar.gz"
if (
  cd "$repo"
  env PATH="$fake_bin:$PATH" RELEASE_TAG=dry-run RELEASE_MODE=dry-run \
    BINARY_ARTIFACT_DIR="$binary_dir" bash "$builder" >/dev/null 2>&1
); then
  echo "Bundle assembler accepted checksum corruption." >&2
  exit 1
fi
printf '%s\n' gitserious-x86_64-unknown-linux-gnu.tar.gz \
  >"$binary_dir/gitserious-x86_64-unknown-linux-gnu.tar.gz"
(
  cd "$binary_dir"
  shasum -a 256 gitserious-x86_64-unknown-linux-gnu.tar.gz \
    >gitserious-x86_64-unknown-linux-gnu.tar.gz.sha256
)

mkdir -p "$binary_dir/duplicate"
cp "$binary_dir/gitserious-x86_64-apple-darwin.tar.gz" \
  "$binary_dir/duplicate/gitserious-x86_64-apple-darwin.tar.gz"
if (
  cd "$repo"
  env PATH="$fake_bin:$PATH" RELEASE_TAG=dry-run RELEASE_MODE=dry-run \
    BINARY_ARTIFACT_DIR="$binary_dir" bash "$builder" >/dev/null 2>&1
); then
  echo "Bundle assembler accepted a duplicate target archive." >&2
  exit 1
fi
rm -f "$binary_dir/duplicate/gitserious-x86_64-apple-darwin.tar.gz"
rm -f "$binary_dir/gitserious-x86_64-pc-windows-msvc.zip"
if (
  cd "$repo"
  env PATH="$fake_bin:$PATH" RELEASE_TAG=dry-run RELEASE_MODE=dry-run \
    BINARY_ARTIFACT_DIR="$binary_dir" bash "$builder" >/dev/null 2>&1
); then
  echo "Bundle assembler accepted a missing target archive." >&2
  exit 1
fi

if (
  cd "$repo"
  env PATH="$fake_bin:$PATH" RELEASE_TAG=dry-run RELEASE_MODE=publish \
    BINARY_ARTIFACT_DIR="$binary_dir" bash "$builder" >/dev/null 2>&1
); then
  echo "Bundle assembler accepted publish mode for tag=dry-run." >&2
  exit 1
fi

echo "Release bundle fixtures passed."
