#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
writer="$script_dir/write-release-summary.sh"
fixture_dir="$(mktemp -d)"
trap 'rm -rf "$fixture_dir"' EXIT

fake_bin="$fixture_dir/bin"
artifact_dir="$fixture_dir/artifacts"
summary_file="$fixture_dir/summary.md"
mkdir -p "$fake_bin" "$artifact_dir"

cat >"$fake_bin/cargo" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
[[ "${1:-}" == metadata ]] || exit 2
cat <<'JSON'
{"workspace_members":["app 0.1.0","cli 0.1.0"],"packages":[{"id":"app 0.1.0","name":"gitserious-app","version":"0.1.0","source":null,"publish":null},{"id":"cli 0.1.0","name":"gitserious","version":"0.1.0","source":null,"publish":null}],"resolve":{"nodes":[{"id":"app 0.1.0","dependencies":[],"deps":[]},{"id":"cli 0.1.0","dependencies":["app 0.1.0"],"deps":[{"pkg":"app 0.1.0"}]}]}}
JSON
EOF
chmod +x "$fake_bin/cargo"

cat >"$artifact_dir/release-manifest.json" <<'JSON'
{
  "release_tag": "v0.1.0",
  "release_mode": "publish",
  "source_commit": "0123456789abcdef0123456789abcdef01234567",
  "workspace_version": "0.1.0",
  "rust_toolchain": "1.96.0",
  "targets": [
    {"target": "x86_64-unknown-linux-gnu"},
    {"target": "x86_64-apple-darwin"},
    {"target": "aarch64-apple-darwin"},
    {"target": "x86_64-pc-windows-msvc"}
  ]
}
JSON
(
  cd "$artifact_dir"
  shasum -a 256 release-manifest.json >SHA256SUMS
)

env CARGO="$fake_bin/cargo" ARTIFACT_DIR="$artifact_dir" \
  GITHUB_STEP_SUMMARY="$summary_file" RELEASE_SOURCE_REF=refs/tags/v0.1.0 \
  bash "$writer" >/dev/null

# Literal backticks are part of the rendered Markdown contract.
# shellcheck disable=SC2016
for expected in \
  '# Release authorization state' \
  '| Classification | stable release |' \
  '| Requested ref | `refs/tags/v0.1.0` |' \
  '| Release branch | `release/0.1` |' \
  '| Release-branch head | `0123456789abcdef0123456789abcdef01234567` |' \
  '| Tag commit | `0123456789abcdef0123456789abcdef01234567` |' \
  '`x86_64-pc-windows-msvc`' \
  '1. `gitserious-app` `0.1.0`' \
  '2. `gitserious` `0.1.0`'; do
  grep -F "$expected" "$summary_file" >/dev/null || {
    echo "Release summary omitted: ${expected}" >&2
    exit 1
  }
done

printf '%s\n' corrupt >>"$artifact_dir/release-manifest.json"
if env CARGO="$fake_bin/cargo" ARTIFACT_DIR="$artifact_dir" \
  GITHUB_STEP_SUMMARY="$summary_file" RELEASE_SOURCE_REF=refs/tags/v0.1.0 \
  bash "$writer" >/dev/null 2>&1; then
  echo "Release summary accepted a corrupted checksum index." >&2
  exit 1
fi

echo "Release authorization summary fixtures passed."
