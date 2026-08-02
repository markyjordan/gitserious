#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
updater="$script_dir/update-homebrew-tap.sh"
fixture_dir="$(mktemp -d)"
trap 'rm -rf "$fixture_dir"' EXIT

fake_bin="$fixture_dir/bin"
tap_repo="$fixture_dir/tap"
tap_origin="$fixture_dir/tap-origin.git"
assets="$fixture_dir/assets"
manifest="$assets/release-manifest.json"
gh_log="$fixture_dir/gh.log"
mkdir -p "$fake_bin" "$tap_repo" "$assets"
: >"$gh_log"

cat >"$fake_bin/gh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf 'gh %s\n' "$*" >>"${GH_LOG:?GH_LOG is required}"
if [[ "${1:-}" == pr && "${2:-}" == list ]]; then
  printf '%s\n' "${PR_NUMBER:-}"
fi
EOF
chmod +x "$fake_bin/gh"

cat >"$tap_repo/README.md" <<'EOF'
# Homebrew Tap

## Tap Registry Table

| Formula | Description | Source | Status | Install |
| --- | --- | --- | --- | --- |
| `gitserious` | Semantic Git policy CLI for humans and agents | source | Candidate only | install |
EOF
git -C "$tap_repo" init -q -b main
git -C "$tap_repo" config user.name Fixture
git -C "$tap_repo" config user.email fixture@example.invalid
git -C "$tap_repo" add README.md
git -C "$tap_repo" commit -qm "tap main"
git -C "$fixture_dir" init -q --bare "$tap_origin"
git -C "$tap_repo" remote add origin "$tap_origin"
git -C "$tap_repo" push -q -u origin main

targets=(
  x86_64-unknown-linux-gnu.tar.gz
  x86_64-apple-darwin.tar.gz
  aarch64-apple-darwin.tar.gz
  x86_64-pc-windows-msvc.zip
)
for target in "${targets[@]}"; do
  printf '%s\n' "$target" >"$assets/gitserious-${target}"
done

python3 - "$assets" "$manifest" <<'PY'
import hashlib
import json
import pathlib
import sys

assets = pathlib.Path(sys.argv[1])
manifest = {
    "release_tag": "v0.1.0",
    "workspace_version": "0.1.0",
    "source_commit": "a" * 40,
    "targets": [],
}
for path in sorted(assets.glob("gitserious-*")):
    target, extension = path.name.removeprefix("gitserious-").rsplit(".", 1)
    if path.name.endswith(".tar.gz"):
        target = path.name.removeprefix("gitserious-").removesuffix(".tar.gz")
    else:
        target = path.name.removeprefix("gitserious-").removesuffix(".zip")
    manifest["targets"].append({
        "target": target,
        "filename": path.name,
        "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
    })
pathlib.Path(sys.argv[2]).write_text(json.dumps(manifest), encoding="utf-8")
PY

run_updater() {
  env PATH="$fake_bin:$PATH" GH_TOKEN=fixture GH_LOG="$gh_log" \
    RELEASE_TAG=v0.1.0 SOURCE_REPOSITORY=markyjordan/gitserious \
    TAP_REPOSITORY=markyjordan/homebrew-tap MANIFEST_FILE="$manifest" \
    ASSET_DIR="$assets" TAP_DIR="$tap_repo" "$@" bash "$updater" >/dev/null
}

run_updater
git -C "$tap_origin" show-ref --verify --quiet \
  refs/heads/automation/gitserious-v0.1.0
if git -C "$tap_origin" show-ref --verify --quiet refs/heads/main \
  && git --git-dir="$tap_origin" show main:Formula/gitserious.rb >/dev/null 2>&1; then
  echo "Tap updater wrote the formula directly to main." >&2
  exit 1
fi
grep -F 'gh pr create' "$gh_log" >/dev/null

: >"$gh_log"
run_updater PR_NUMBER=7
grep -F 'gh pr edit 7' "$gh_log" >/dev/null
if grep -F 'gh pr create' "$gh_log" >/dev/null; then
  echo "Tap updater opened a duplicate pull request." >&2
  exit 1
fi

python3 - "$tap_repo/Formula/gitserious.rb" <<'PY'
import pathlib
import re
import sys

path = pathlib.Path(sys.argv[1])
formula = path.read_text(encoding="utf-8")
formula = re.sub(r'sha256 "[0-9a-f]{64}"', 'sha256 "' + "f" * 64 + '"', formula, count=1)
path.write_text(formula, encoding="utf-8")
PY
git -C "$tap_repo" add Formula/gitserious.rb
git -C "$tap_repo" commit -qm "conflicting digest"
git -C "$tap_repo" push -q origin HEAD:refs/heads/automation/gitserious-v0.1.0
if run_updater PR_NUMBER=7 2>/dev/null; then
  echo "Tap updater overwrote a different published digest." >&2
  exit 1
fi

if env PATH="$fake_bin:$PATH" GH_TOKEN=fixture GH_LOG="$gh_log" \
  RELEASE_TAG=v0.1.0-rc1 SOURCE_REPOSITORY=markyjordan/gitserious \
  TAP_REPOSITORY=markyjordan/homebrew-tap MANIFEST_FILE="$manifest" \
  ASSET_DIR="$assets" TAP_DIR="$tap_repo" bash "$updater" >/dev/null 2>&1; then
  echo "Tap updater accepted a release candidate." >&2
  exit 1
fi

echo "Homebrew tap handoff fixtures passed."
