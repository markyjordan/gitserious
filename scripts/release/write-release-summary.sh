#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
artifact_dir="${ARTIFACT_DIR:-target/release-artifacts}"
source_ref="${RELEASE_SOURCE_REF:?RELEASE_SOURCE_REF is required}"
summary_file="${GITHUB_STEP_SUMMARY:?GITHUB_STEP_SUMMARY is required}"
cargo_command="${CARGO:-cargo}"

for required_file in SHA256SUMS release-manifest.json; do
  [[ -f "$artifact_dir/$required_file" ]] || {
    echo "${artifact_dir}/${required_file} is required for the release summary." >&2
    exit 1
  }
done

command -v python3 >/dev/null || {
  echo "python3 is required to render the release summary." >&2
  exit 1
}

(
  cd "$artifact_dir"
  shasum -a 256 -c SHA256SUMS >/dev/null
)

metadata_file="$(mktemp)"
package_order_file="$(mktemp)"
trap 'rm -f "$metadata_file" "$package_order_file"' EXIT
"$cargo_command" metadata --locked --format-version 1 >"$metadata_file"
python3 "$script_dir/list-publishable-packages.py" "$metadata_file" >"$package_order_file"

[[ -s "$package_order_file" ]] || {
  echo "No publishable workspace packages found for the release summary." >&2
  exit 1
}

manifest_digest="$(shasum -a 256 "$artifact_dir/release-manifest.json" | awk '{print $1}')"
python3 - \
  "$artifact_dir/release-manifest.json" \
  "$package_order_file" \
  "$source_ref" \
  "$manifest_digest" \
  "$summary_file" <<'PY'
import json
import pathlib
import re
import sys

manifest_path = pathlib.Path(sys.argv[1])
package_order_path = pathlib.Path(sys.argv[2])
source_ref = sys.argv[3]
manifest_digest = sys.argv[4]
summary_path = pathlib.Path(sys.argv[5])

manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
tag = manifest["release_tag"]
source_commit = manifest["source_commit"]

if tag == "dry-run":
    classification = "dry run"
    release_branch = source_ref.removeprefix("refs/heads/")
    tag_commit = "not applicable"
elif re.fullmatch(r"v[0-9]+\.[0-9]+\.[0-9]+-rc[1-9][0-9]*", tag):
    classification = "release candidate"
    major, minor = tag.removeprefix("v").split(".")[:2]
    release_branch = f"release/{major}.{minor}"
    tag_commit = source_commit
elif re.fullmatch(r"v[0-9]+\.[0-9]+\.[0-9]+", tag):
    classification = "stable release"
    major, minor = tag.removeprefix("v").split(".")[:2]
    release_branch = f"release/{major}.{minor}"
    tag_commit = source_commit
else:
    raise SystemExit(f"unsupported release tag in manifest: {tag}")

packages = []
for line in package_order_path.read_text(encoding="utf-8").splitlines():
    name, version = line.split("\t", 1)
    packages.append((name, version))

lines = [
    "# Release authorization state",
    "",
    "| Field | Value |",
    "| --- | --- |",
    f"| Tag | `{tag}` |",
    f"| Mode | `{manifest['release_mode']}` |",
    f"| Classification | {classification} |",
    f"| Requested ref | `{source_ref}` |",
    f"| Release branch | `{release_branch}` |",
    f"| Release-branch head | `{source_commit}` |",
    f"| Tag commit | `{tag_commit}` |",
    f"| Workspace version | `{manifest['workspace_version']}` |",
    f"| Rust toolchain | `{manifest['rust_toolchain']}` |",
    f"| `release-manifest.json` SHA256 | `{manifest_digest}` |",
    "",
    "## Required native targets",
    "",
]
lines.extend(f"- `{target['target']}`" for target in manifest["targets"])
lines.extend(["", "## Stable crate publication order", ""])
lines.extend(
    f"{position}. `{name}` `{version}`"
    for position, (name, version) in enumerate(packages, start=1)
)
lines.extend(
    [
        "",
        "> The checksum index was verified before this summary was written. "
        "Protected publication jobs consume this exact run's assembled bundle.",
        "",
    ]
)

with summary_path.open("a", encoding="utf-8") as handle:
    handle.write("\n".join(lines))
PY

echo "Wrote release authorization state to ${summary_file}."
