#!/usr/bin/env bash
set -euo pipefail

tag="${RELEASE_TAG:?RELEASE_TAG is required}"
release_mode="${RELEASE_MODE:-dry-run}"
artifact_dir="${ARTIFACT_DIR:-target/release-artifacts}"
source_commit="${RELEASE_SOURCE_COMMIT:-}"
github_output="${GITHUB_OUTPUT:-}"

case "$release_mode" in
  dry-run | publish) ;;
  *)
    echo "RELEASE_MODE must be dry-run or publish; got ${release_mode}." >&2
    exit 1
    ;;
esac

if [[ "$tag" == dry-run ]]; then
  [[ "$release_mode" == dry-run ]] || {
    echo "The dry-run sentinel cannot be verified in publish mode." >&2
    exit 1
  }
elif [[ ! "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-rc[1-9][0-9]*)?$ ]]; then
  echo "RELEASE_TAG must be dry-run, vX.Y.Z, or vX.Y.Z-rcN; got ${tag}." >&2
  exit 1
fi

[[ -d "$artifact_dir" ]] || {
  echo "Release artifact directory does not exist: ${artifact_dir}." >&2
  exit 1
}

if [[ -z "$source_commit" ]]; then
  source_commit="$(git rev-parse 'HEAD^{commit}')"
fi

python_command="${PYTHON:-}"
if [[ -z "$python_command" ]]; then
  if command -v python3 >/dev/null; then
    python_command=python3
  elif command -v python >/dev/null; then
    python_command=python
  else
    echo "Python is required to verify release artifacts." >&2
    exit 1
  fi
fi

"$python_command" - \
  "$artifact_dir" \
  "$tag" \
  "$release_mode" \
  "$source_commit" \
  "$github_output" <<'PY'
from __future__ import annotations

import datetime as dt
import hashlib
import json
import pathlib
import re
import sys
import tarfile
import zipfile


root = pathlib.Path(sys.argv[1])
expected_tag = sys.argv[2]
expected_mode = sys.argv[3]
expected_commit = sys.argv[4]
github_output = pathlib.Path(sys.argv[5]) if sys.argv[5] else None

archive_members = {
    "LICENSE-APACHE-2.0",
    "LICENSE-MIT",
    "README.md",
}
digest_pattern = re.compile(r"^[0-9a-f]{64}$")
checksum_pattern = re.compile(r"^([0-9a-f]{64}) [ *]([^/]+)$")
tag_pattern = re.compile(r"^v([0-9]+)\.([0-9]+)\.([0-9]+)(?:-rc[1-9][0-9]*)?$")


def fail(message: str) -> None:
    raise SystemExit(message)


def sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def parse_checksum_records(path: pathlib.Path) -> dict[str, str]:
    records: dict[str, str] = {}
    lines = path.read_text(encoding="utf-8").splitlines()
    if not lines:
        fail(f"{path.name} must not be empty")
    for line in lines:
        match = checksum_pattern.fullmatch(line)
        if not match:
            fail(f"invalid checksum record in {path.name}: {line}")
        digest, filename = match.groups()
        if filename in records:
            fail(f"duplicate checksum record for {filename} in {path.name}")
        records[filename] = digest
    return records


manifest_path = root / "release-manifest.json"
try:
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
except (OSError, json.JSONDecodeError) as error:
    fail(f"invalid release-manifest.json: {error}")

expected_manifest_keys = {
    "release_tag",
    "release_mode",
    "source_commit",
    "workspace_version",
    "rust_toolchain",
    "created_at",
    "targets",
    "source_archive",
}
if set(manifest) != expected_manifest_keys:
    fail(
        "release-manifest.json fields do not match the release contract: "
        f"{sorted(manifest)}"
    )

if manifest["release_tag"] != expected_tag:
    fail(
        "release-manifest.json tag does not match the authorized tag: "
        f"{manifest['release_tag']} != {expected_tag}"
    )
if manifest["release_mode"] != expected_mode:
    fail(
        "release-manifest.json mode does not match the authorized mode: "
        f"{manifest['release_mode']} != {expected_mode}"
    )
if manifest["source_commit"] != expected_commit:
    fail(
        "release-manifest.json commit does not match the checked-out commit: "
        f"{manifest['source_commit']} != {expected_commit}"
    )
if not re.fullmatch(r"[0-9a-f]{40}|[0-9a-f]{64}", manifest["source_commit"]):
    fail("release-manifest.json contains an invalid source commit")

version = manifest["workspace_version"]
if not isinstance(version, str) or not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", version):
    fail("release-manifest.json contains an invalid workspace version")
if expected_tag != "dry-run":
    match = tag_pattern.fullmatch(expected_tag)
    if not match or ".".join(match.groups()) != version:
        fail(f"workspace version {version} does not match {expected_tag}")

artifact_version = version if expected_tag == "dry-run" else expected_tag.removeprefix("v")
target_contract = [
    (
        "x86_64-unknown-linux-gnu",
        f"gitserious-{artifact_version}-x86_64-unknown-linux-gnu.tar.gz",
    ),
    (
        "x86_64-apple-darwin",
        f"gitserious-{artifact_version}-x86_64-apple-darwin.tar.gz",
    ),
    (
        "aarch64-apple-darwin",
        f"gitserious-{artifact_version}-aarch64-apple-darwin.tar.gz",
    ),
    (
        "x86_64-pc-windows-msvc",
        f"gitserious-{artifact_version}-x86_64-pc-windows-msvc.zip",
    ),
]

rust_toolchain = manifest["rust_toolchain"]
if not isinstance(rust_toolchain, str) or not rust_toolchain:
    fail("release-manifest.json contains an invalid Rust toolchain")
try:
    dt.datetime.strptime(manifest["created_at"], "%Y-%m-%dT%H:%M:%SZ")
except (TypeError, ValueError):
    fail("release-manifest.json contains an invalid created_at timestamp")

source_archive = f"gitserious-{artifact_version}-source.tar.gz"
source_record = manifest["source_archive"]
if not isinstance(source_record, dict) or set(source_record) != {"filename", "sha256"}:
    fail("release-manifest.json contains an invalid source_archive record")
if source_record["filename"] != source_archive:
    fail("release-manifest.json source archive filename does not match the artifact version")
if not isinstance(source_record["sha256"], str) or not digest_pattern.fullmatch(
    source_record["sha256"]
):
    fail("release-manifest.json contains an invalid source archive digest")

expected_files = {
    "CHANGELOG.md",
    "SHA256SUMS",
    "package-files.txt",
    "release-manifest.json",
    "release-notes.md",
    source_archive,
}
for _, filename in target_contract:
    expected_files.add(filename)
    expected_files.add(f"{filename}.sha256")

actual_files: set[str] = set()
for path in root.iterdir():
    if not path.is_file():
        fail(f"release bundle contains a non-file entry: {path.name}")
    actual_files.add(path.name)

missing = sorted(expected_files - actual_files)
extra = sorted(actual_files - expected_files)
if missing or extra:
    fail(f"release bundle file set mismatch; missing={missing}, extra={extra}")

checksum_records = parse_checksum_records(root / "SHA256SUMS")
expected_checksum_files = expected_files - {"SHA256SUMS"}
if set(checksum_records) != expected_checksum_files:
    missing_checksums = sorted(expected_checksum_files - set(checksum_records))
    extra_checksums = sorted(set(checksum_records) - expected_checksum_files)
    fail(
        "SHA256SUMS file set mismatch; "
        f"missing={missing_checksums}, extra={extra_checksums}"
    )

for filename, recorded_digest in checksum_records.items():
    actual_digest = sha256(root / filename)
    if actual_digest != recorded_digest:
        fail(f"SHA256SUMS digest mismatch for {filename}")

manifest_targets = manifest["targets"]
if not isinstance(manifest_targets, list) or len(manifest_targets) != len(target_contract):
    fail("release-manifest.json must contain exactly four targets")

for position, (target, filename) in enumerate(target_contract):
    record = manifest_targets[position]
    if not isinstance(record, dict) or set(record) != {"target", "filename", "sha256"}:
        fail(f"release-manifest.json contains an invalid target record at {position}")
    if record["target"] != target or record["filename"] != filename:
        fail(f"release-manifest.json target order or filename mismatch at {position}")
    if record["sha256"] != checksum_records[filename]:
        fail(f"release-manifest.json digest mismatch for {filename}")

    sibling_records = parse_checksum_records(root / f"{filename}.sha256")
    if sibling_records != {filename: checksum_records[filename]}:
        fail(f"invalid sibling checksum for {filename}")

    expected_members = set(archive_members)
    expected_members.add("gitserious.exe" if filename.endswith(".zip") else "gitserious")
    if filename.endswith(".zip"):
        with zipfile.ZipFile(root / filename) as archive:
            members = archive.infolist()
            names = [member.filename for member in members]
            if any(member.is_dir() for member in members):
                fail(f"{filename} contains an unexpected directory")
    else:
        with tarfile.open(root / filename, "r:gz") as archive:
            members = archive.getmembers()
            names = [member.name for member in members]
            if any(not member.isfile() for member in members):
                fail(f"{filename} contains a non-file member")
            executable = next((member for member in members if member.name == "gitserious"), None)
            if executable is None or executable.mode & 0o111 == 0:
                fail(f"{filename} does not contain an executable gitserious binary")
    if len(names) != len(set(names)) or set(names) != expected_members:
        fail(f"{filename} archive layout does not match the release contract: {sorted(names)}")

if source_record["sha256"] != checksum_records[source_archive]:
    fail("release-manifest.json source archive digest does not match SHA256SUMS")

source_prefix = f"gitserious-{artifact_version}/"
with tarfile.open(root / source_archive, "r:gz") as archive:
    source_members = archive.getmembers()
    if not source_members:
        fail(f"{source_archive} must not be empty")
    for member in source_members:
        member_path = pathlib.PurePosixPath(member.name)
        if member_path.is_absolute() or ".." in member_path.parts:
            fail(f"{source_archive} contains an unsafe path: {member.name}")
        if member.name != source_prefix.rstrip("/") and not member.name.startswith(source_prefix):
            fail(f"{source_archive} contains a path outside {source_prefix}: {member.name}")
    if f"{source_prefix}Cargo.toml" not in {member.name for member in source_members}:
        fail(f"{source_archive} does not contain the workspace Cargo.toml")

heading = f"## [{version}]"
if heading not in (root / "CHANGELOG.md").read_text(encoding="utf-8"):
    fail(f"CHANGELOG.md does not contain {heading}")
if heading not in (root / "release-notes.md").read_text(encoding="utf-8"):
    fail(f"release-notes.md does not contain {heading}")
if not (root / "package-files.txt").read_text(encoding="utf-8").strip():
    fail("package-files.txt must not be empty")

manifest_digest = sha256(manifest_path)
if github_output is not None:
    with github_output.open("a", encoding="utf-8") as handle:
        for output_name, (_, filename) in zip(
            (
                "linux_x86_64_archive",
                "macos_x86_64_archive",
                "macos_aarch64_archive",
                "windows_x86_64_archive",
            ),
            target_contract,
            strict=True,
        ):
            handle.write(f"{output_name}={filename}\n")
        handle.write(f"source_archive={source_archive}\n")
        handle.write(f"manifest_digest={manifest_digest}\n")

print(
    f"Verified {expected_mode} release bundle for {expected_tag} "
    f"at {expected_commit} ({manifest_digest})."
)
PY
