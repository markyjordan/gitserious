#!/usr/bin/env bash
set -euo pipefail

tag="${RELEASE_TAG:?RELEASE_TAG is required}"
source_repository="${SOURCE_REPOSITORY:-markyjordan/gitserious}"
tap_repository="${TAP_REPOSITORY:-markyjordan/homebrew-tap}"
manifest="${MANIFEST_FILE:?MANIFEST_FILE is required}"
asset_dir="${ASSET_DIR:?ASSET_DIR is required}"
tap_dir="${TAP_DIR:?TAP_DIR is required}"

[[ "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || {
  echo "Homebrew handoff requires a stable vX.Y.Z tag; got ${tag}." >&2
  exit 1
}
[[ -d "$tap_dir/.git" ]] || {
  echo "TAP_DIR must be a Git checkout: ${tap_dir}." >&2
  exit 1
}
[[ -f "$manifest" ]] || {
  echo "Release manifest not found: ${manifest}." >&2
  exit 1
}

command -v gh >/dev/null || {
  echo "gh is required to open or update the tap pull request." >&2
  exit 1
}
[[ -n "${GH_TOKEN:-${GITHUB_TOKEN:-}}" ]] || {
  echo "GH_TOKEN or GITHUB_TOKEN is required for the tap handoff." >&2
  exit 1
}

python_command="${PYTHON:-}"
if [[ -z "$python_command" ]]; then
  if command -v python3 >/dev/null; then
    python_command=python3
  elif command -v python >/dev/null; then
    python_command=python
  else
    echo "Python is required for the tap handoff." >&2
    exit 1
  fi
fi

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
temp_dir="$(mktemp -d)"
trap 'rm -rf "$temp_dir"' EXIT
formula="$temp_dir/gitserious.rb"
pr_body="$temp_dir/pr-body.md"
"$script_dir/render-homebrew-formula.sh" "$manifest" "$formula" >/dev/null

manifest_values="$($python_command - "$manifest" "$asset_dir" "$tag" <<'PY'
import hashlib
import json
import pathlib
import re
import sys

manifest_path = pathlib.Path(sys.argv[1])
asset_dir = pathlib.Path(sys.argv[2])
expected_tag = sys.argv[3]
with manifest_path.open(encoding="utf-8") as handle:
    manifest = json.load(handle)
if manifest.get("release_tag") != expected_tag:
    raise SystemExit("release manifest tag does not match the requested handoff")
version = expected_tag.removeprefix("v")
if manifest.get("workspace_version") != version:
    raise SystemExit("release manifest workspace version does not match its tag")
if not re.fullmatch(r"[0-9a-f]{40}", manifest.get("source_commit", "")):
    raise SystemExit("release manifest source commit is invalid")

entries = manifest.get("targets", [])
if len(entries) != 4:
    raise SystemExit("release manifest must contain exactly four targets")
for entry in entries:
    path = asset_dir / entry["filename"]
    if not path.is_file():
        raise SystemExit(f"missing published asset: {entry['filename']}")
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    if digest != entry.get("sha256"):
        raise SystemExit(f"published asset checksum mismatch: {entry['filename']}")
print(version)
PY
)"
version="$(printf '%s\n' "$manifest_values" | sed -n '1p')"

branch="automation/gitserious-${tag}"
release_url="https://github.com/${source_repository}/releases/tag/${tag}"

git -C "$tap_dir" fetch --prune origin main "$branch" 2>/dev/null ||
  git -C "$tap_dir" fetch --prune origin main

main_formula="$temp_dir/main-gitserious.rb"
if git -C "$tap_dir" show origin/main:Formula/gitserious.rb >"$main_formula" 2>/dev/null; then
  main_state="$($python_command - "$main_formula" "$manifest" <<'PY'
import json
import pathlib
import re
import sys

formula = pathlib.Path(sys.argv[1]).read_text(encoding="utf-8")
manifest = json.loads(pathlib.Path(sys.argv[2]).read_text(encoding="utf-8"))
version = manifest["workspace_version"]
if f'version "{version}"' not in formula:
    print("other-version")
    raise SystemExit
expected = sorted(entry["sha256"] for entry in manifest["targets"] if "windows" not in entry["target"])
actual = sorted(re.findall(r'sha256 "([0-9a-f]{64})"', formula))
print("exact" if actual == expected else "digest-conflict")
PY
)"
  case "$main_state" in
    exact)
      echo "Tap main already contains ${tag} with the published digests."
      exit 0
      ;;
    digest-conflict)
      echo "Tap main already declares ${version} with different published digests." >&2
      exit 1
      ;;
  esac
fi

if git -C "$tap_dir" show-ref --verify --quiet "refs/remotes/origin/${branch}"; then
  git -C "$tap_dir" switch -C "$branch" "origin/${branch}"
  branch_formula="$temp_dir/branch-gitserious.rb"
  if git -C "$tap_dir" show "origin/${branch}:Formula/gitserious.rb" >"$branch_formula" 2>/dev/null; then
    branch_state="$($python_command - "$branch_formula" "$manifest" <<'PY'
import json
import pathlib
import re
import sys

formula = pathlib.Path(sys.argv[1]).read_text(encoding="utf-8")
manifest = json.loads(pathlib.Path(sys.argv[2]).read_text(encoding="utf-8"))
version = manifest["workspace_version"]
if f'version "{version}"' not in formula:
    print("replace")
    raise SystemExit
expected = sorted(entry["sha256"] for entry in manifest["targets"] if "windows" not in entry["target"])
actual = sorted(re.findall(r'sha256 "([0-9a-f]{64})"', formula))
print("same" if actual == expected else "digest-conflict")
PY
)"
    [[ "$branch_state" != digest-conflict ]] || {
      echo "Existing ${branch} declares ${version} with different published digests." >&2
      exit 1
    }
  fi
else
  git -C "$tap_dir" switch -C "$branch" origin/main
fi

mkdir -p "$tap_dir/Formula"
cp "$formula" "$tap_dir/Formula/gitserious.rb"

"$python_command" - "$tap_dir/README.md" "$tag" "$release_url" <<'PY'
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
tag = sys.argv[2]
release_url = sys.argv[3]
lines = path.read_text(encoding="utf-8").splitlines()
updated = False
for index, line in enumerate(lines):
    if line.startswith("| `gitserious` |"):
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        if len(cells) != 5:
            raise SystemExit("unexpected gitserious tap registry row")
        cells[3] = f"Stable [{tag}]({release_url}) available"
        lines[index] = "| " + " | ".join(cells) + " |"
        updated = True
        break
if not updated:
    raise SystemExit("README tap registry has no gitserious row")
path.write_text("\n".join(lines) + "\n", encoding="utf-8")
PY

"$python_command" - "$manifest" "$release_url" "$source_repository" >"$pr_body" <<'PY'
import json
import pathlib
import sys

manifest = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
release_url = sys.argv[2]
source_repository = sys.argv[3]
print(f"Updates `gitserious` to [{manifest['release_tag']}]({release_url}).")
print()
print(f"Source commit: `{manifest['source_commit']}`")
print()
print("| Target | Asset | SHA256 |")
print("| --- | --- | --- |")
for entry in manifest["targets"]:
    if "windows" not in entry["target"]:
        print(f"| `{entry['target']}` | `{entry['filename']}` | `{entry['sha256']}` |")
print()
print("Verification:")
print()
print("```sh")
print(f"gh release download {manifest['release_tag']} --repo {source_repository}")
print("shasum -a 256 -c SHA256SUMS")
print(f"gh attestation verify release-manifest.json --repo {source_repository}")
print("brew install markyjordan/tap/gitserious")
print("brew test markyjordan/tap/gitserious")
print("```")
PY

git -C "$tap_dir" config user.name "gitserious release automation"
git -C "$tap_dir" config user.email "41898282+github-actions[bot]@users.noreply.github.com"
git -C "$tap_dir" add Formula/gitserious.rb README.md
if ! git -C "$tap_dir" diff --cached --quiet; then
  git -C "$tap_dir" commit -m "gitserious ${tag}"
  git -C "$tap_dir" push origin "HEAD:refs/heads/${branch}"
fi

pr_number="$(gh pr list --repo "$tap_repository" --state open --head "$branch" \
  --json number --jq '.[0].number // empty')"
title="gitserious ${tag}"
if [[ -n "$pr_number" ]]; then
  gh pr edit "$pr_number" --repo "$tap_repository" --title "$title" --body-file "$pr_body"
  echo "Updated tap pull request #${pr_number} for ${tag}."
else
  gh pr create --repo "$tap_repository" --base main --head "$branch" \
    --title "$title" --body-file "$pr_body"
  echo "Opened tap pull request for ${tag}."
fi
