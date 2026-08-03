#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
renderer="$script_dir/render-homebrew-formula.sh"
fixture_dir="$(mktemp -d)"
trap 'rm -rf "$fixture_dir"' EXIT

manifest="$fixture_dir/release-manifest.json"
formula="$fixture_dir/gitserious.rb"
cat >"$manifest" <<'JSON'
{
  "release_tag": "v0.1.0",
  "workspace_version": "0.1.0",
  "targets": [
    {"target":"x86_64-unknown-linux-gnu","filename":"gitserious-x86_64-unknown-linux-gnu.tar.gz","sha256":"1111111111111111111111111111111111111111111111111111111111111111"},
    {"target":"x86_64-apple-darwin","filename":"gitserious-x86_64-apple-darwin.tar.gz","sha256":"2222222222222222222222222222222222222222222222222222222222222222"},
    {"target":"aarch64-apple-darwin","filename":"gitserious-aarch64-apple-darwin.tar.gz","sha256":"3333333333333333333333333333333333333333333333333333333333333333"},
    {"target":"x86_64-pc-windows-msvc","filename":"gitserious-x86_64-pc-windows-msvc.zip","sha256":"4444444444444444444444444444444444444444444444444444444444444444"}
  ]
}
JSON

bash "$renderer" "$manifest" "$formula" >/dev/null
grep -F 'version "0.1.0"' "$formula" >/dev/null
grep -F 'license any_of: ["MIT", "Apache-2.0"]' "$formula" >/dev/null
grep -F 'releases/download/v0.1.0/gitserious-aarch64-apple-darwin.tar.gz' "$formula" >/dev/null
grep -F 'releases/download/v0.1.0/gitserious-x86_64-apple-darwin.tar.gz' "$formula" >/dev/null
grep -F 'releases/download/v0.1.0/gitserious-x86_64-unknown-linux-gnu.tar.gz' "$formula" >/dev/null
grep -F 'system bin/"gitserious"' "$formula" >/dev/null
if grep -F 'windows' "$formula" >/dev/null; then
  echo "Rendered Homebrew formula exposed the Windows-only asset." >&2
  exit 1
fi

python3 - "$manifest" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
manifest = json.loads(path.read_text(encoding="utf-8"))
manifest["release_tag"] = "v0.1.0-rc1"
path.write_text(json.dumps(manifest), encoding="utf-8")
PY
if bash "$renderer" "$manifest" "$formula" >/dev/null 2>&1; then
  echo "Formula renderer accepted a prerelease manifest." >&2
  exit 1
fi

echo "Homebrew formula rendering fixtures passed."
