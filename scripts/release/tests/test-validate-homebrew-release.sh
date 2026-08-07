#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
validator="$script_dir/validate-homebrew-release.sh"
fixture_dir="$(mktemp -d)"
trap 'rm -rf "$fixture_dir"' EXIT

manifest="$fixture_dir/release-manifest.json"
metadata="$fixture_dir/release-metadata.json"
tag_commit=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa

write_valid_fixture() {
  python3 - "$manifest" "$metadata" "$tag_commit" <<'PY'
import json
import pathlib
import sys


manifest_path = pathlib.Path(sys.argv[1])
metadata_path = pathlib.Path(sys.argv[2])
tag_commit = sys.argv[3]
manifest_path.write_text(
    json.dumps(
        {
            "release_tag": "v0.1.0",
            "release_mode": "publish",
            "source_commit": tag_commit,
            "workspace_version": "0.1.0",
        }
    ),
    encoding="utf-8",
)
metadata_path.write_text(
    json.dumps(
        {
            "tagName": "v0.1.0",
            "isDraft": False,
            "isPrerelease": False,
            "url": "https://github.com/markyjordan/gitserious/releases/tag/v0.1.0",
        }
    ),
    encoding="utf-8",
)
PY
}

run_validator() {
  RELEASE_TAG=v0.1.0 \
    SOURCE_REPOSITORY=markyjordan/gitserious \
    MANIFEST_FILE="$manifest" \
    RELEASE_METADATA_FILE="$metadata" \
    TAG_COMMIT="$tag_commit" \
    bash "$validator" >/dev/null
}

expect_fail() {
  local reason="$1"
  if run_validator 2>/dev/null; then
    echo "Stable release validator accepted ${reason}." >&2
    exit 1
  fi
}

mutate_json() {
  local path="$1"
  local key="$2"
  local value="$3"
  python3 - "$path" "$key" "$value" <<'PY'
import json
import pathlib
import sys


path = pathlib.Path(sys.argv[1])
key = sys.argv[2]
raw_value = sys.argv[3]
if raw_value == "true":
    value = True
elif raw_value == "false":
    value = False
else:
    value = raw_value
data = json.loads(path.read_text(encoding="utf-8"))
data[key] = value
path.write_text(json.dumps(data), encoding="utf-8")
PY
}

write_valid_fixture
run_validator

mutate_json "$metadata" isDraft true
expect_fail "a draft GitHub Release"

write_valid_fixture
mutate_json "$metadata" isPrerelease true
expect_fail "a GitHub prerelease"

write_valid_fixture
mutate_json "$metadata" tagName v0.1.1
expect_fail "a different GitHub Release tag"

write_valid_fixture
mutate_json "$manifest" release_mode dry-run
expect_fail "a dry-run release manifest"

write_valid_fixture
mutate_json "$manifest" source_commit bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
expect_fail "a tag and manifest commit mismatch"

write_valid_fixture
mutate_json "$metadata" url https://github.com/other/project/releases/tag/v0.1.0
expect_fail "a release URL from another repository"

echo "Stable Homebrew release identity fixtures passed."
