#!/usr/bin/env bash
# Refresh gitserious local worktrees after a topic PR merges into origin/dev.
# Idempotent: a second run with the same inputs converges to the same end state.
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: refresh-worktrees.sh [options]

Refresh the gitserious-DEV worktree to match origin/dev, then reset sibling
topic worktrees (gitserious-<type>/) onto origin/dev so the same directory
names can take a new marky/<type>/<name> branch.

Options:
  --parent DIR     Directory that contains the worktrees (default: parent of
                   the common git dir's primary worktree, or $PWD's parent
                   when run from a sibling layout)
  --repo-root DIR  Any worktree of the repo used to discover git worktrees
                   (default: walk up from $PWD)
  --dry-run        Print actions without changing refs or worktrees
  --force          Allow dirty worktrees and unmerged topic branches
  --keep-branches  Do not delete local marky/* branches after reset
  -h, --help       Show this help

Expected layout (sibling directories):
  gitserious-DEV/              branch: dev  (tracks origin/dev)
  gitserious-feat/             next: marky/feat/<name>
  gitserious-fix/              next: marky/fix/<name>
  gitserious-chore/            next: marky/chore/<name>
  gitserious-<type>/           next: marky/<type>/<name>
EOF
}

DRY_RUN=0
FORCE=0
KEEP_BRANCHES=0
PARENT=""
REPO_ROOT=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --parent)
      PARENT="${2:?}"
      shift 2
      ;;
    --repo-root)
      REPO_ROOT="${2:?}"
      shift 2
      ;;
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    --force)
      FORCE=1
      shift
      ;;
    --keep-branches)
      KEEP_BRANCHES=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

log() { printf '%s\n' "$*"; }
run() {
  if [[ "$DRY_RUN" -eq 1 ]]; then
    log "dry-run: $*"
  else
    "$@"
  fi
}

is_dev_basename() {
  [[ "$1" == "gitserious-DEV" ]]
}

is_topic_basename() {
  local base="$1"
  [[ "$base" == gitserious-* ]] || return 1
  [[ "$base" != "gitserious-DEV" ]] || return 1
  [[ "$base" != "gitserious" ]] || return 1
  local type="${base#gitserious-}"
  [[ "$type" =~ ^[a-z][a-z0-9-]*$ ]]
}

topic_type_from_basename() {
  printf '%s\n' "${1#gitserious-}"
}

find_repo_root() {
  if [[ -n "$REPO_ROOT" ]]; then
    git -C "$REPO_ROOT" rev-parse --show-toplevel
    return
  fi
  if git rev-parse --show-toplevel >/dev/null 2>&1; then
    git rev-parse --show-toplevel
    return
  fi
  echo "error: not inside a gitserious worktree; pass --repo-root" >&2
  exit 1
}

worktree_dirty() {
  local path="$1"
  [[ -n "$(git -C "$path" status --porcelain 2>/dev/null)" ]]
}

branch_merged_into() {
  local path="$1"
  local branch="$2"
  local into_ref="$3"
  git -C "$path" merge-base --is-ancestor "$branch" "$into_ref"
}

# True when the topic branch is safe to drop and reset.
# Merge commits: tip is an ancestor of origin/dev.
# Squash/rebase merges: tip is not an ancestor; treat a pruned
# origin/<branch> (after fetch --prune) as merged. Prefer GitHub's
# "delete branch on merge" so this path works without --force.
topic_branch_consumable() {
  local path="$1"
  local branch="$2"
  if branch_merged_into "$path" "$branch" origin/dev; then
    return 0
  fi
  if ! git -C "$path" show-ref --verify --quiet "refs/remotes/origin/$branch"; then
    return 0
  fi
  if command -v gh >/dev/null 2>&1; then
    local state
    state="$(
      gh pr view "$branch" --json state --jq .state 2>/dev/null || true
    )"
    if [[ "$state" == "MERGED" ]]; then
      return 0
    fi
  fi
  return 1
}

REPO_ROOT="$(find_repo_root)"
_common_raw="$(git -C "$REPO_ROOT" rev-parse --git-common-dir)"
if [[ "$_common_raw" = /* ]]; then
  COMMON_DIR="$(cd "$_common_raw" && pwd)"
else
  COMMON_DIR="$(cd "$REPO_ROOT/$_common_raw" && pwd)"
fi
unset _common_raw

if [[ -z "$PARENT" ]]; then
  while IFS= read -r line; do
    case "$line" in
      worktree\ *)
        wt="${line#worktree }"
        base="$(basename "$wt")"
        if is_dev_basename "$base"; then
          PARENT="$(dirname "$wt")"
          break
        fi
        ;;
    esac
  done < <(git -C "$REPO_ROOT" worktree list --porcelain)

  if [[ -z "$PARENT" ]]; then
    PARENT="$(dirname "$REPO_ROOT")"
  fi
fi

PARENT="$(cd "$PARENT" && pwd)"
log "repo: $REPO_ROOT"
log "common git dir: $COMMON_DIR"
log "parent: $PARENT"

run git -C "$REPO_ROOT" fetch origin --prune

DEV_PATH=""
declare -a TOPIC_PATHS=()
declare -a TOPIC_BASES=()

while IFS= read -r line; do
  case "$line" in
    worktree\ *)
      wt="${line#worktree }"
      base="$(basename "$wt")"
      if is_dev_basename "$base"; then
        DEV_PATH="$wt"
      elif is_topic_basename "$base"; then
        parent_of_wt="$(dirname "$wt")"
        if [[ "$parent_of_wt" == "$PARENT" ]]; then
          TOPIC_PATHS+=("$wt")
          TOPIC_BASES+=("$base")
        fi
      fi
      ;;
  esac
done < <(git -C "$REPO_ROOT" worktree list --porcelain)

if [[ -z "$DEV_PATH" ]]; then
  if [[ -d "$PARENT/gitserious-DEV" ]]; then
    echo "error: $PARENT/gitserious-DEV exists but is not a linked worktree of this repo" >&2
    exit 1
  fi
  echo "error: no gitserious-DEV worktree found under $PARENT" >&2
  echo "hint: create it with: git worktree add -b dev \"$PARENT/gitserious-DEV\" origin/dev" >&2
  exit 1
fi

if ! git -C "$REPO_ROOT" rev-parse --verify refs/remotes/origin/dev >/dev/null 2>&1; then
  echo "error: origin/dev missing after fetch" >&2
  exit 1
fi

ORIGIN_DEV="$(git -C "$REPO_ROOT" rev-parse origin/dev)"
log "origin/dev: $ORIGIN_DEV"

refresh_dev() {
  local path="$1"
  log ""
  log "== refresh DEV: $path =="

  if worktree_dirty "$path" && [[ "$FORCE" -eq 0 ]]; then
    echo "error: DEV worktree is dirty; commit, stash, or pass --force" >&2
    git -C "$path" status --short >&2
    exit 1
  fi

  local head_branch
  head_branch="$(git -C "$path" branch --show-current || true)"
  if [[ "$head_branch" != "dev" ]]; then
    if git -C "$path" show-ref --verify --quiet refs/heads/dev; then
      run git -C "$path" switch --discard-changes dev 2>/dev/null || run git -C "$path" switch -f dev
    else
      run git -C "$path" switch -C dev --track origin/dev
    fi
  fi

  if [[ "$FORCE" -eq 1 ]]; then
    run git -C "$path" reset --hard origin/dev
    run git -C "$path" clean -fd
  else
    run git -C "$path" merge --ff-only origin/dev
  fi

  if [[ "$DRY_RUN" -eq 1 ]]; then
    log "DEV would match origin/dev ($ORIGIN_DEV)"
    return
  fi
  local tip
  tip="$(git -C "$path" rev-parse HEAD)"
  if [[ "$tip" != "$ORIGIN_DEV" ]]; then
    echo "error: DEV HEAD $tip does not match origin/dev $ORIGIN_DEV" >&2
    exit 1
  fi
  log "DEV ok at $tip"
}

refresh_topic() {
  local path="$1"
  local base="$2"
  local type
  type="$(topic_type_from_basename "$base")"
  log ""
  log "== refresh topic: $path (type=$type) =="

  if worktree_dirty "$path" && [[ "$FORCE" -eq 0 ]]; then
    echo "error: topic worktree is dirty; commit, stash, or pass --force: $path" >&2
    git -C "$path" status --short >&2
    exit 1
  fi

  local head_branch
  head_branch="$(git -C "$path" branch --show-current || true)"

  if [[ -n "$head_branch" && "$head_branch" == marky/* ]]; then
    if ! topic_branch_consumable "$path" "$head_branch"; then
      if [[ "$FORCE" -eq 0 ]]; then
        echo "error: $head_branch is not consumable yet (not in origin/dev, origin/$head_branch still exists, no merged PR)" >&2
        echo "hint: merge the PR and delete the remote branch, or pass --force" >&2
        exit 1
      fi
      log "warning: forcing reset of unmerged branch $head_branch"
    fi
  fi

  # Detach so DEV alone can own the local `dev` branch.
  run git -C "$path" switch --detach origin/dev
  if [[ "$FORCE" -eq 1 ]]; then
    run git -C "$path" reset --hard origin/dev
    run git -C "$path" clean -fd
  fi

  if [[ "$KEEP_BRANCHES" -eq 0 && -n "$head_branch" && "$head_branch" == marky/* ]]; then
    if git -C "$path" show-ref --verify --quiet "refs/heads/$head_branch"; then
      run git -C "$path" branch -D "$head_branch"
    fi
  fi

  if [[ "$DRY_RUN" -eq 1 ]]; then
    log "topic would match origin/dev ($ORIGIN_DEV); next: git -C \"$path\" switch -c marky/$type/<name>"
    return
  fi
  local tip
  tip="$(git -C "$path" rev-parse HEAD)"
  if [[ "$tip" != "$ORIGIN_DEV" ]]; then
    echo "error: topic HEAD $tip does not match origin/dev $ORIGIN_DEV ($path)" >&2
    exit 1
  fi
  log "topic ok at $tip; next: git -C \"$path\" switch -c marky/$type/<name>"
}

refresh_dev "$DEV_PATH"

if [[ "${#TOPIC_PATHS[@]}" -eq 0 ]]; then
  log ""
  log "no topic worktrees under $PARENT matching gitserious-<type>/"
else
  for i in "${!TOPIC_PATHS[@]}"; do
    refresh_topic "${TOPIC_PATHS[$i]}" "${TOPIC_BASES[$i]}"
  done
fi

log ""
log "done. DEV and topic pools match origin/dev ($ORIGIN_DEV)."
