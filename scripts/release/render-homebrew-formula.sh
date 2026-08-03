#!/usr/bin/env bash
set -euo pipefail

manifest="${1:-${MANIFEST_FILE:-}}"
output="${2:-${FORMULA_OUTPUT:-}}"

[[ -n "$manifest" && -f "$manifest" ]] || {
  echo "A release manifest path is required." >&2
  exit 1
}
[[ -n "$output" ]] || {
  echo "A formula output path is required." >&2
  exit 1
}

python_command="${PYTHON:-}"
if [[ -z "$python_command" ]]; then
  if command -v python3 >/dev/null; then
    python_command=python3
  elif command -v python >/dev/null; then
    python_command=python
  else
    echo "Python is required to render the Homebrew formula." >&2
    exit 1
  fi
fi

mkdir -p "$(dirname "$output")"
"$python_command" - "$manifest" "$output" <<'PY'
import json
import pathlib
import re
import sys

manifest_path = pathlib.Path(sys.argv[1])
output_path = pathlib.Path(sys.argv[2])
with manifest_path.open(encoding="utf-8") as handle:
    manifest = json.load(handle)

tag = manifest.get("release_tag", "")
match = re.fullmatch(r"v(\d+\.\d+\.\d+)", tag)
if not match:
    raise SystemExit(f"Homebrew formulas require a stable vX.Y.Z manifest; got {tag!r}")
version = match.group(1)
if manifest.get("workspace_version") != version:
    raise SystemExit("manifest workspace version does not match its stable tag")

expected = {
    "aarch64-apple-darwin": "tar.gz",
    "x86_64-apple-darwin": "tar.gz",
    "x86_64-unknown-linux-gnu": "tar.gz",
    "x86_64-pc-windows-msvc": "zip",
}
targets = {}
for entry in manifest.get("targets", []):
    target = entry.get("target")
    if target in targets:
        raise SystemExit(f"duplicate target in manifest: {target}")
    if target not in expected:
        raise SystemExit(f"unexpected target in manifest: {target}")
    expected_filename = f"gitserious-{target}.{expected[target]}"
    digest = entry.get("sha256", "")
    if entry.get("filename") != expected_filename:
        raise SystemExit(f"unexpected filename for {target}")
    if not re.fullmatch(r"[0-9a-f]{64}", digest):
        raise SystemExit(f"invalid SHA256 for {target}")
    targets[target] = entry
if set(targets) != set(expected):
    missing = sorted(set(expected) - set(targets))
    raise SystemExit(f"manifest is missing required targets: {missing}")

repository = "https://github.com/markyjordan/gitserious"

def url(target):
    return f"{repository}/releases/download/{tag}/{targets[target]['filename']}"

formula = f'''class Gitserious < Formula
  desc "Semantic Git policy CLI for humans and agents"
  homepage "{repository}"
  version "{version}"
  license any_of: ["MIT", "Apache-2.0"]

  on_macos do
    on_arm do
      url "{url('aarch64-apple-darwin')}"
      sha256 "{targets['aarch64-apple-darwin']['sha256']}"
    end

    on_intel do
      url "{url('x86_64-apple-darwin')}"
      sha256 "{targets['x86_64-apple-darwin']['sha256']}"
    end
  end

  on_linux do
    on_intel do
      url "{url('x86_64-unknown-linux-gnu')}"
      sha256 "{targets['x86_64-unknown-linux-gnu']['sha256']}"
    end
  end

  def install
    bin.install "gitserious"
  end

  test do
    system bin/"gitserious"
  end
end
'''
output_path.write_text(formula, encoding="utf-8")
PY

echo "Rendered ${output} from ${manifest}."
