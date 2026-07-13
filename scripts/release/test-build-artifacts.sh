#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
builder="$script_dir/build-artifacts.sh"
fixture_dir="$(mktemp -d)"
trap 'rm -rf "$fixture_dir"' EXIT

fake_bin="$fixture_dir/bin"
repo="$fixture_dir/repo"
mkdir -p "$fake_bin" "$repo/target/release"

cat >"$fake_bin/cargo" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
case "${1:-}" in
  package)
    printf '%s\n' Cargo.toml Cargo.lock src/main.rs
    ;;
  build)
    mkdir -p target/release
    printf '%s\n' '#!/usr/bin/env bash' 'echo gitserious' >target/release/gitserious
    chmod +x target/release/gitserious
    ;;
  *)
    exit 2
    ;;
esac
EOF
chmod +x "$fake_bin/cargo"

printf '%s\n' '# Changelog' '## [0.1.0]' >"$repo/CHANGELOG.md"
(
  cd "$repo"
  env PATH="$fake_bin:$PATH" RELEASE_TAG=v0.1.0 RELEASE_MODE=dry-run \
    bash "$builder" >/dev/null
)

artifact_dir="$repo/target/release-artifacts"
for artifact in CHANGELOG.md package-files.txt release-notes.md release-plan.json SHA256SUMS v0.1.0-gitserious; do
  if [[ ! -f "$artifact_dir/$artifact" ]]; then
    echo "Missing release artifact: ${artifact}" >&2
    exit 1
  fi
done

if [[ "$(jq -r '.release_mode' "$artifact_dir/release-plan.json")" != "dry-run" ]]; then
  echo "Release manifest did not record dry-run mode." >&2
  exit 1
fi
if ! grep -F '## [0.1.0]' "$artifact_dir/release-notes.md" >/dev/null; then
  echo "Release notes did not contain the versioned changelog section." >&2
  exit 1
fi
if [[ "$(jq -r '.publish_operations_enabled' "$artifact_dir/release-plan.json")" != "false" ]]; then
  echo "Dry-run manifest enabled publish operations." >&2
  exit 1
fi
(
  cd "$artifact_dir"
  shasum -a 256 -c SHA256SUMS >/dev/null
)

if (
  cd "$repo"
  env PATH="$fake_bin:$PATH" RELEASE_TAG=v0.1.0-rc0 RELEASE_MODE=dry-run \
    bash "$builder" >/dev/null 2>&1
); then
  echo "Artifact builder accepted an invalid release candidate tag." >&2
  exit 1
fi

echo "Release artifact fixtures passed."
