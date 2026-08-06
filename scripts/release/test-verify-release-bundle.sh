#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
verifier="$script_dir/verify-release-bundle.sh"
fixture_dir="$(mktemp -d)"
trap 'rm -rf "$fixture_dir"' EXIT

artifact_dir="$fixture_dir/artifacts"
output_file="$fixture_dir/output"
source_commit=cccccccccccccccccccccccccccccccccccccccc

build_fixture() {
  rm -rf "$artifact_dir"
  mkdir -p "$artifact_dir"
  python3 - "$artifact_dir" "$source_commit" <<'PY'
import hashlib
import io
import json
import pathlib
import sys
import tarfile
import zipfile


root = pathlib.Path(sys.argv[1])
source_commit = sys.argv[2]
version = "0.1.0"


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def add_tar_file(archive, name, content, mode=0o644):
    payload = content.encode()
    info = tarfile.TarInfo(name)
    info.size = len(payload)
    info.mode = mode
    archive.addfile(info, io.BytesIO(payload))


targets = [
    ("x86_64-unknown-linux-gnu", "gitserious-0.1.0-rc1-x86_64-unknown-linux-gnu.tar.gz"),
    ("x86_64-apple-darwin", "gitserious-0.1.0-rc1-x86_64-apple-darwin.tar.gz"),
    ("aarch64-apple-darwin", "gitserious-0.1.0-rc1-aarch64-apple-darwin.tar.gz"),
    ("x86_64-pc-windows-msvc", "gitserious-0.1.0-rc1-x86_64-pc-windows-msvc.zip"),
]
manifest_targets = []
for target, filename in targets:
    archive_path = root / filename
    members = {
        "LICENSE-APACHE-2.0": "Apache fixture\n",
        "LICENSE-MIT": "MIT fixture\n",
        "README.md": "# gitserious fixture\n",
    }
    if filename.endswith(".zip"):
        members["gitserious.exe"] = "windows fixture binary\n"
        with zipfile.ZipFile(archive_path, "w", zipfile.ZIP_DEFLATED) as archive:
            for name, content in members.items():
                archive.writestr(name, content)
    else:
        members["gitserious"] = "native fixture binary\n"
        with tarfile.open(archive_path, "w:gz") as archive:
            for name, content in members.items():
                add_tar_file(archive, name, content, 0o755 if name == "gitserious" else 0o644)

    digest = sha256(archive_path)
    (root / f"{filename}.sha256").write_text(
        f"{digest}  {filename}\n", encoding="utf-8"
    )
    manifest_targets.append(
        {"target": target, "filename": filename, "sha256": digest}
    )

artifact_version = f"{version}-rc1"
source_archive = root / f"gitserious-{artifact_version}-source.tar.gz"
with tarfile.open(source_archive, "w:gz") as archive:
    add_tar_file(archive, f"gitserious-{artifact_version}/Cargo.toml", "[workspace]\n")
    add_tar_file(archive, f"gitserious-{artifact_version}/README.md", "# fixture\n")

(root / "CHANGELOG.md").write_text(
    "# Changelog\n\n## [0.1.0] - TBD\n\n- Fixture release.\n", encoding="utf-8"
)
(root / "release-notes.md").write_text(
    "## [0.1.0] - TBD\n\n- Fixture release.\n", encoding="utf-8"
)
(root / "package-files.txt").write_text("Cargo.toml\nsrc/main.rs\n", encoding="utf-8")

manifest = {
    "release_tag": "v0.1.0-rc1",
    "release_mode": "publish",
    "source_commit": source_commit,
    "workspace_version": version,
    "rust_toolchain": "1.96.0",
    "created_at": "2026-08-04T00:00:00Z",
    "targets": manifest_targets,
    "source_archive": {
        "filename": source_archive.name,
        "sha256": sha256(source_archive),
    },
}
(root / "release-manifest.json").write_text(
    json.dumps(manifest, indent=2) + "\n", encoding="utf-8"
)

records = []
for path in sorted(root.iterdir(), key=lambda item: item.name):
    if path.is_file() and path.name != "SHA256SUMS":
        records.append(f"{sha256(path)}  {path.name}")
(root / "SHA256SUMS").write_text("\n".join(records) + "\n", encoding="utf-8")
PY
}

refresh_index() {
  python3 - "$artifact_dir" <<'PY'
import hashlib
import pathlib
import sys


root = pathlib.Path(sys.argv[1])
records = []
for path in sorted(root.iterdir(), key=lambda item: item.name):
    if path.is_file() and path.name != "SHA256SUMS":
        records.append(f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}")
(root / "SHA256SUMS").write_text("\n".join(records) + "\n", encoding="utf-8")
PY
}

expect_fail() {
  local reason="$1"
  if RELEASE_TAG=v0.1.0-rc1 RELEASE_MODE=publish \
    RELEASE_SOURCE_COMMIT="$source_commit" ARTIFACT_DIR="$artifact_dir" \
    bash "$verifier" >/dev/null 2>&1; then
    echo "Release bundle verifier accepted ${reason}." >&2
    exit 1
  fi
}

build_fixture
: >"$output_file"
RELEASE_TAG=v0.1.0-rc1 RELEASE_MODE=publish \
  RELEASE_SOURCE_COMMIT="$source_commit" ARTIFACT_DIR="$artifact_dir" \
  GITHUB_OUTPUT="$output_file" bash "$verifier" >/dev/null
grep -Fx 'source_archive=gitserious-0.1.0-rc1-source.tar.gz' "$output_file" >/dev/null
grep -Fx 'linux_x86_64_archive=gitserious-0.1.0-rc1-x86_64-unknown-linux-gnu.tar.gz' \
  "$output_file" >/dev/null
grep -Fx 'macos_x86_64_archive=gitserious-0.1.0-rc1-x86_64-apple-darwin.tar.gz' \
  "$output_file" >/dev/null
grep -Fx 'macos_aarch64_archive=gitserious-0.1.0-rc1-aarch64-apple-darwin.tar.gz' \
  "$output_file" >/dev/null
grep -Fx 'windows_x86_64_archive=gitserious-0.1.0-rc1-x86_64-pc-windows-msvc.zip' \
  "$output_file" >/dev/null
grep -E '^manifest_digest=[0-9a-f]{64}$' "$output_file" >/dev/null

printf '%s\n' corrupt \
  >>"$artifact_dir/gitserious-0.1.0-rc1-x86_64-unknown-linux-gnu.tar.gz"
expect_fail "checksum corruption"

build_fixture
printf '%s\n' unexpected >"$artifact_dir/unexpected.txt"
expect_fail "an extra artifact"

build_fixture
mv "$artifact_dir/gitserious-0.1.0-rc1-x86_64-pc-windows-msvc.zip" \
  "$fixture_dir/windows.zip"
expect_fail "a missing target archive"

build_fixture
printf '%s\n' "$(head -1 "$artifact_dir/SHA256SUMS")" >>"$artifact_dir/SHA256SUMS"
expect_fail "a duplicate checksum record"

build_fixture
python3 - "$artifact_dir/release-manifest.json" <<'PY'
import json
import pathlib
import sys


path = pathlib.Path(sys.argv[1])
manifest = json.loads(path.read_text(encoding="utf-8"))
manifest["release_tag"] = "v0.1.0-rc2"
path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
PY
refresh_index
expect_fail "a manifest for a different tag"

build_fixture
python3 - "$artifact_dir" <<'PY'
import hashlib
import io
import json
import pathlib
import sys
import tarfile


root = pathlib.Path(sys.argv[1])
filename = "gitserious-0.1.0-rc1-x86_64-unknown-linux-gnu.tar.gz"
archive_path = root / filename
with tarfile.open(archive_path, "w:gz") as archive:
    for name in [
        "LICENSE-APACHE-2.0",
        "LICENSE-MIT",
        "README.md",
        "gitserious",
        "unexpected.txt",
    ]:
        payload = f"{name} fixture\n".encode()
        info = tarfile.TarInfo(name)
        info.size = len(payload)
        info.mode = 0o755 if name == "gitserious" else 0o644
        archive.addfile(info, io.BytesIO(payload))

digest = hashlib.sha256(archive_path.read_bytes()).hexdigest()
(root / f"{filename}.sha256").write_text(
    f"{digest}  {filename}\n", encoding="utf-8"
)
manifest_path = root / "release-manifest.json"
manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
manifest["targets"][0]["sha256"] = digest
manifest_path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
PY
refresh_index
expect_fail "a target archive with an unexpected member"

build_fixture
if RELEASE_TAG=dry-run RELEASE_MODE=publish \
  RELEASE_SOURCE_COMMIT="$source_commit" ARTIFACT_DIR="$artifact_dir" \
  bash "$verifier" >/dev/null 2>&1; then
  echo "Release bundle verifier accepted publish mode for tag=dry-run." >&2
  exit 1
fi

echo "Release bundle verification fixtures passed."
