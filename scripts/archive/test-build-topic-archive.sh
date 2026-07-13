#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
builder="$script_dir/build-topic-archive.sh"
fixture_dir="$(mktemp -d)"
trap 'rm -rf "$fixture_dir"' EXIT

repo="$fixture_dir/repo"
mkdir -p "$repo"
git -C "$repo" init -q -b dev
git -C "$repo" config user.name "Archive Fixture"
git -C "$repo" config user.email "archive-fixture@example.invalid"
touch "$repo/base.txt"
git -C "$repo" add base.txt
git -C "$repo" commit -qm base
git -C "$repo" switch -qc topic
printf '%s\n' one >"$repo/topic.txt"
git -C "$repo" add topic.txt
git -C "$repo" commit -qm one
printf '%s\n' two >>"$repo/topic.txt"
git -C "$repo" commit -qam two

(
  cd "$repo"
  GITHUB_REPOSITORY=markyjordan/gitserious GITHUB_ACTOR=maintainer \
    bash "$builder" --base-ref dev --head-ref topic --pr-number 42 --out-dir archive >/dev/null
)

archive="$repo/archive"
for file in topic.bundle archive-manifest.v1.json jibsa-artifact.v1.json commit-log.txt \
  diff.patch stat.txt changed-files.txt prompt-context.md RESTORE.md SHA256SUMS; do
  [[ -f "$archive/$file" ]] || {
    echo "Missing topic archive file: ${file}" >&2
    exit 1
  }
done
(
  cd "$archive"
  git bundle verify topic.bundle >/dev/null
  shasum -a 256 -c SHA256SUMS >/dev/null
)
git clone -q --branch dev --single-branch "$repo" "$fixture_dir/restored"
git -C "$fixture_dir/restored" fetch -q "$archive/topic.bundle" \
  'refs/heads/topic:refs/heads/restored-topic'
[[ "$(git -C "$fixture_dir/restored" rev-parse restored-topic)" == "$(git -C "$repo" rev-parse topic)" ]] || {
  echo "Restored archive head did not match the topic head." >&2
  exit 1
}
[[ "$(jq -r '.topic_summary.commit_count' "$archive/archive-manifest.v1.json")" == 2 ]] || {
  echo "Topic archive recorded the wrong commit count." >&2
  exit 1
}

if (
  cd "$repo"
  bash "$builder" --base-ref dev --head-ref dev --out-dir empty >/dev/null 2>&1
); then
  echo "Topic archive accepted a head with no topic commits." >&2
  exit 1
fi

echo "Topic archive fixtures passed."
