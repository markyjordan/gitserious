#!/usr/bin/env bash
set -euo pipefail

tag="${RELEASE_TAG:?RELEASE_TAG is required}"
repository="${SOURCE_REPOSITORY:?SOURCE_REPOSITORY is required}"
manifest="${MANIFEST_FILE:?MANIFEST_FILE is required}"
release_metadata="${RELEASE_METADATA_FILE:?RELEASE_METADATA_FILE is required}"
tag_commit="${TAG_COMMIT:?TAG_COMMIT is required}"

[[ "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || {
  echo "Homebrew requires a stable vX.Y.Z release; got ${tag}." >&2
  exit 1
}
[[ "$repository" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]] || {
  echo "SOURCE_REPOSITORY must look like owner/name; got ${repository}." >&2
  exit 1
}
[[ -f "$manifest" ]] || {
  echo "Release manifest not found: ${manifest}." >&2
  exit 1
}
[[ -f "$release_metadata" ]] || {
  echo "GitHub Release metadata not found: ${release_metadata}." >&2
  exit 1
}
[[ "$tag_commit" =~ ^[0-9a-f]{40}$|^[0-9a-f]{64}$ ]] || {
  echo "TAG_COMMIT is not a full commit object ID." >&2
  exit 1
}

python_command="${PYTHON:-}"
if [[ -z "$python_command" ]]; then
  if command -v python3 >/dev/null; then
    python_command=python3
  elif command -v python >/dev/null; then
    python_command=python
  else
    echo "Python is required to validate the stable GitHub Release." >&2
    exit 1
  fi
fi

"$python_command" - \
  "$manifest" "$release_metadata" "$tag" "$repository" "$tag_commit" <<'PY'
import json
import pathlib
import re
import sys


manifest_path = pathlib.Path(sys.argv[1])
metadata_path = pathlib.Path(sys.argv[2])
expected_tag = sys.argv[3]
repository = sys.argv[4]
tag_commit = sys.argv[5]

try:
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
except (OSError, json.JSONDecodeError) as error:
    raise SystemExit(f"invalid release identity input: {error}") from error

if manifest.get("release_tag") != expected_tag:
    raise SystemExit("release manifest tag does not match the requested Homebrew handoff")
if manifest.get("release_mode") != "publish":
    raise SystemExit("Homebrew requires a bundle assembled in publish mode")
version = expected_tag.removeprefix("v")
if manifest.get("workspace_version") != version:
    raise SystemExit("release manifest workspace version does not match its stable tag")
source_commit = manifest.get("source_commit", "")
if not re.fullmatch(r"[0-9a-f]{40}|[0-9a-f]{64}", source_commit):
    raise SystemExit("release manifest source commit is invalid")
if source_commit != tag_commit:
    raise SystemExit("stable tag commit does not match the release manifest source commit")

if metadata.get("tagName") != expected_tag:
    raise SystemExit("GitHub Release tag does not match the requested Homebrew handoff")
if metadata.get("isDraft") is not False:
    raise SystemExit("Homebrew cannot consume a draft GitHub Release")
if metadata.get("isPrerelease") is not False:
    raise SystemExit("Homebrew cannot consume a GitHub prerelease")
expected_url = f"https://github.com/{repository}/releases/tag/{expected_tag}"
if metadata.get("url") != expected_url:
    raise SystemExit("GitHub Release URL does not match the source repository and tag")

print(f"Validated stable GitHub Release {expected_tag} at {tag_commit}.")
PY
