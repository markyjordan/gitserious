#!/usr/bin/env bash
set -euo pipefail

die() {
  echo "error: $*" >&2
  exit 1
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || die "$1 is required"
}

write_checksums() {
  local directory="$1"
  local checksum_file
  checksum_file="$(mktemp)"
  (
    trap 'rm -f "$checksum_file"' EXIT
    cd "$directory"
    find . -type f ! -name SHA256SUMS -print | LC_ALL=C sort |
      while IFS= read -r path; do
        shasum -a 256 "${path#./}"
      done >"$checksum_file"
    mv "$checksum_file" SHA256SUMS
    trap - EXIT
  )
}

base_ref="${BASE_REF:-dev}"
head_ref="${HEAD_REF:-}"
out_dir="${OUT_DIR:-}"
repository="${GITHUB_REPOSITORY:-local}"
actor="${GITHUB_ACTOR:-${USER:-unknown}}"
pr_number="${PR_NUMBER:-}"
mode=github

while (($#)); do
  case "$1" in
    --base-ref)
      base_ref="${2:?--base-ref requires a value}"
      shift 2
      ;;
    --head-ref)
      head_ref="${2:?--head-ref requires a value}"
      mode=local
      shift 2
      ;;
    --out-dir)
      out_dir="${2:?--out-dir requires a value}"
      shift 2
      ;;
    --pr-number)
      pr_number="${2:?--pr-number requires a value}"
      shift 2
      ;;
    *)
      die "unknown argument: $1"
      ;;
  esac
done

require_command git
require_command jq

pull_request_url=""
if [[ "$mode" == github ]]; then
  [[ "$pr_number" =~ ^[0-9]+$ ]] || die "PR_NUMBER must be numeric"
  [[ -n "${GH_TOKEN:-${GITHUB_TOKEN:-}}" ]] || die "GitHub token is required"
  require_command gh

  metadata="$(gh api "repos/${repository}/pulls/${pr_number}")"
  base_ref="$(jq -r '.base.ref' <<<"$metadata")"
  head_ref="$(jq -r '.head.ref' <<<"$metadata")"
  api_head_sha="$(jq -r '.head.sha' <<<"$metadata")"
  head_repository="$(jq -r '.head.repo.full_name' <<<"$metadata")"
  pull_request_url="$(jq -r '.html_url' <<<"$metadata")"

  [[ "$base_ref" == dev ]] || die "PR #${pr_number} must target dev"
  [[ "$api_head_sha" =~ ^[0-9a-fA-F]{40}$ ]] || die "PR head SHA is invalid"

  git fetch --no-tags origin "+refs/heads/dev:refs/remotes/origin/dev"
  git fetch --no-tags origin "pull/${pr_number}/head:refs/remotes/pr/${pr_number}/head"
  base_rev=refs/remotes/origin/dev
  head_rev="refs/remotes/pr/${pr_number}/head"
  head_sha="$(git rev-parse "${head_rev}^{commit}")"
  [[ "$head_sha" == "$api_head_sha" ]] || die "fetched PR head differs from GitHub metadata"
else
  [[ -n "$head_ref" ]] || die "--head-ref is required in local mode"
  pr_number="${pr_number:-local}"
  head_repository="$repository"
  base_rev="$base_ref"
  head_rev="$head_ref"
  head_sha="$(git rev-parse "${head_rev}^{commit}")"
fi

base_sha="$(git rev-parse "${base_rev}^{commit}")"
merge_base_sha="$(git merge-base "$base_sha" "$head_sha")"
commit_count="$(git rev-list --count "${merge_base_sha}..${head_sha}")"
[[ "$commit_count" != 0 ]] || die "topic head has no commits after its merge base"

short_sha="${head_sha:0:12}"
artifact_name="topic-archive-pr-${pr_number}-${short_sha}"
out_dir="${out_dir:-target/topic-archives/${artifact_name}}"
case "$out_dir" in
  "" | / | . | ./) die "refusing unsafe output directory: ${out_dir}" ;;
esac

rm -rf "$out_dir"
mkdir -p "$out_dir"

git bundle create "$out_dir/topic.bundle" "$head_rev" "^${merge_base_sha}"
git log --reverse --date=iso-strict --pretty=format:'%H%x09%aI%x09%an%x09%s' \
  "${merge_base_sha}..${head_sha}" >"$out_dir/commit-log.txt"
git diff --binary "$merge_base_sha" "$head_sha" >"$out_dir/diff.patch"
git diff --stat "$merge_base_sha" "$head_sha" >"$out_dir/stat.txt"
git diff --name-status "$merge_base_sha" "$head_sha" >"$out_dir/changed-files.txt"

created_at="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
changed_files="$(jq -Rs 'split("\n") | map(select(length > 0))' <"$out_dir/changed-files.txt")"
bundle_heads="$(git bundle list-heads "$out_dir/topic.bundle")"
bundle_head_ref="$(awk 'NR == 1 {print $2}' <<<"$bundle_heads")"
restore_branch="restored-topic-${pr_number}-${short_sha}"

jq -n \
  --arg repository "$repository" --arg pr_number "$pr_number" \
  --arg pull_request_url "$pull_request_url" --arg base_ref "$base_ref" \
  --arg base_sha "$base_sha" --arg merge_base_sha "$merge_base_sha" \
  --arg head_ref "$head_ref" --arg head_sha "$head_sha" \
  --arg head_repository "$head_repository" --arg actor "$actor" \
  --arg created_at "$created_at" --arg artifact_name "$artifact_name" \
  --argjson commit_count "$commit_count" --argjson changed_files "$changed_files" \
  '{schema_version:"archive-manifest.v1",type:"topic_branch_archive",repository:$repository,
    pr_number:$pr_number,pull_request_url:$pull_request_url,
    base:{ref:$base_ref,sha:$base_sha},merge_base_sha:$merge_base_sha,
    head:{ref:$head_ref,sha:$head_sha,repository:$head_repository},
    artifact:{name:$artifact_name,created_at:$created_at,created_by:$actor},
    topic_summary:{commit_count:$commit_count,changed_files:$changed_files},
    privacy_note:"This archive may contain source code, diffs, commit messages, and project context. Review before sharing."}' \
  >"$out_dir/archive-manifest.v1.json"

jq -n \
  --arg id "git-topic-archive:${repository}:pr-${pr_number}:${short_sha}" \
  --arg repository "$repository" --arg pr_number "$pr_number" \
  --arg base_ref "$base_ref" --arg base_sha "$base_sha" \
  --arg merge_base_sha "$merge_base_sha" --arg head_ref "$head_ref" \
  --arg head_sha "$head_sha" --arg actor "$actor" --arg created_at "$created_at" \
  --argjson commit_count "$commit_count" \
  '{schema_version:"jibsa-artifact.v1",artifact_type:"git_topic_branch_archive",id:$id,status:"candidate",
    provenance:{source:"gitserious archive-topic-branch workflow",repository:$repository,pr_number:$pr_number,actor:$actor,created_at:$created_at},
    inputs:[{type:"git_ref",role:"base",ref:$base_ref,sha:$base_sha},{type:"git_ref",role:"merge_base",sha:$merge_base_sha},{type:"git_ref",role:"head",ref:$head_ref,sha:$head_sha}],
    outputs:[{type:"git_bundle",path:"topic.bundle"},{type:"git_diff",path:"diff.patch"},{type:"commit_log",path:"commit-log.txt"},{type:"human_prompt_context",path:"prompt-context.md"},{type:"restore_instructions",path:"RESTORE.md"}],
    evaluation_context:{intended_use:"Optional human-owned topic provenance for review and workflow evaluation.",commit_count:$commit_count,merge_target:$base_ref}}' \
  >"$out_dir/jibsa-artifact.v1.json"

cat >"$out_dir/prompt-context.md" <<EOF
# Topic Branch Prompt Context

Repository: \`${repository}\`
Pull request: \`${pr_number}\`
Base: \`${base_ref}\` at \`${base_sha}\`
Head: \`${head_ref}\` at \`${head_sha}\`
Merge base: \`${merge_base_sha}\`
Created by: \`${actor}\` at \`${created_at}\`

This optional archive preserves human-owned topic history for later review.
Review it before sharing because it may contain code, diffs, and commit messages.
EOF

cat >"$out_dir/RESTORE.md" <<EOF
# Restore Topic Branch Archive

Verify and fetch the archived head:

\`\`\`sh
git bundle verify topic.bundle
git fetch ./topic.bundle '${bundle_head_ref}:refs/heads/${restore_branch}'
git switch ${restore_branch}
\`\`\`

This export is not a project branch, release artifact, or merge requirement.
EOF

write_checksums "$out_dir"

if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
  printf 'artifact-name=%s\nartifact-path=%s\n' "$artifact_name" "$out_dir" >>"$GITHUB_OUTPUT"
fi

echo "Built topic archive artifact: ${artifact_name}"
