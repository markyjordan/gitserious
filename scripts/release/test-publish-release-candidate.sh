#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
publisher="$script_dir/publish-release-candidate.sh"
fixture_dir="$(mktemp -d)"
trap 'rm -rf "$fixture_dir"' EXIT

fake_bin="$fixture_dir/bin"
artifact_dir="$fixture_dir/artifacts"
publish_log="$fixture_dir/publish.log"
mkdir -p "$fake_bin" "$artifact_dir"
: >"$publish_log"

cat >"$fake_bin/gh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf 'gh %s\n' "$*" >>"${PUBLISH_LOG:?PUBLISH_LOG is required}"
if [[ "${1:-}" == release && "${2:-}" == view ]]; then
  exit "${VIEW_STATUS:-1}"
fi
EOF
chmod +x "$fake_bin/gh"

printf '%s\n' '## [0.1.0]' >"$artifact_dir/release-notes.md"
printf '%s\n' artifact >"$artifact_dir/v0.1.0-rc1-gitserious"
(
  cd "$artifact_dir"
  shasum -a 256 release-notes.md v0.1.0-rc1-gitserious >SHA256SUMS
)

env PATH="$fake_bin:$PATH" RELEASE_TAG=v0.1.0-rc1 RELEASE_MODE=dry-run \
  ARTIFACT_DIR="$artifact_dir" bash "$publisher" >/dev/null
if [[ -s "$publish_log" ]]; then
  echo "Dry-run prerelease publication invoked GitHub CLI." >&2
  exit 1
fi

if env PATH="$fake_bin:$PATH" RELEASE_TAG=v0.1.0 RELEASE_MODE=publish \
  ARTIFACT_DIR="$artifact_dir" GH_TOKEN=fixture GITHUB_REPOSITORY=markyjordan/gitserious \
  PUBLISH_LOG="$publish_log" bash "$publisher" >/dev/null 2>&1; then
  echo "Prerelease publisher accepted a stable tag." >&2
  exit 1
fi

printf '%s\n' tampered >>"$artifact_dir/release-notes.md"
if env PATH="$fake_bin:$PATH" RELEASE_TAG=v0.1.0-rc1 RELEASE_MODE=publish \
  ARTIFACT_DIR="$artifact_dir" GH_TOKEN=fixture GITHUB_REPOSITORY=markyjordan/gitserious \
  PUBLISH_LOG="$publish_log" bash "$publisher" >/dev/null 2>&1; then
  echo "Prerelease publisher accepted modified artifacts." >&2
  exit 1
fi
printf '%s\n' '## [0.1.0]' >"$artifact_dir/release-notes.md"

: >"$publish_log"
env PATH="$fake_bin:$PATH" RELEASE_TAG=v0.1.0-rc1 RELEASE_MODE=publish \
  ARTIFACT_DIR="$artifact_dir" GH_TOKEN=fixture GITHUB_REPOSITORY=markyjordan/gitserious \
  PUBLISH_LOG="$publish_log" bash "$publisher" >/dev/null

if ! grep -F 'gh release create v0.1.0-rc1 ' "$publish_log" |
  grep -F -- '--repo markyjordan/gitserious --verify-tag --prerelease' >/dev/null; then
  echo "Prerelease publisher did not create the release in the selected repository." >&2
  exit 1
fi
if grep -E 'release upload|--clobber' "$publish_log" >/dev/null; then
  echo "Prerelease publisher retained a mutable asset upload path." >&2
  exit 1
fi

: >"$publish_log"
if env PATH="$fake_bin:$PATH" RELEASE_TAG=v0.1.0-rc1 RELEASE_MODE=publish \
  ARTIFACT_DIR="$artifact_dir" GH_TOKEN=fixture GITHUB_REPOSITORY=markyjordan/gitserious \
  PUBLISH_LOG="$publish_log" VIEW_STATUS=0 bash "$publisher" >/dev/null 2>&1; then
  echo "Prerelease publisher accepted an existing release." >&2
  exit 1
fi

echo "Release-candidate publication fixtures passed."
