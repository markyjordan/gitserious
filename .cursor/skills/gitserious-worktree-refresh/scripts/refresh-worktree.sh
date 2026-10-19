#!/usr/bin/env bash
# Refresh gitserious-DEV from origin/dev, then create or reattach a sibling worktree.
set -euo pipefail

DEFAULT_DEV="/Users/marky/Documents/Chia/marky++/projects/oss/cli/gitserious-DEV"
PRESERVE_UNTRACKED=(.gitserious gitserious.toml gitserious.lock)

usage() {
  cat <<'EOF'
Usage: refresh-worktree.sh [options] <destination> <branch>

Refresh gitserious-DEV to origin/dev (ff-only when possible), then create or
reattach the sibling worktree at <destination> on <branch>.

Options:
  --dev PATH                 DEV worktree root (default: local gitserious-DEV path)
  --dry-run                  Print actions without mutating
  --allow-equivalent-reset   When HEAD diverges from origin/dev but tracked trees
                             match, archive the current tip and reset --keep
  --skip-live-check          Skip git ls-remote confirmation before creating a
                             new branch (not recommended)
  -h, --help                 Show this help
EOF
}

DRY_RUN=0
ALLOW_EQUIV_RESET=0
SKIP_LIVE=0
DEV_ROOT=""
DEST=""
BRANCH=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dev)
      DEV_ROOT="${2:?}"
      shift 2
      ;;
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    --allow-equivalent-reset)
      ALLOW_EQUIV_RESET=1
      shift
      ;;
    --skip-live-check)
      SKIP_LIVE=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    --)
      shift
      break
      ;;
    -*)
      echo "error: unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
    *)
      if [[ -z "$DEST" ]]; then
        DEST="$1"
      elif [[ -z "$BRANCH" ]]; then
        BRANCH="$1"
      else
        echo "error: unexpected argument: $1" >&2
        usage >&2
        exit 2
      fi
      shift
      ;;
  esac
done

if [[ -z "$DEST" || -z "$BRANCH" ]]; then
  echo "error: destination and branch are required" >&2
  usage >&2
  exit 2
fi

if [[ "$BRANCH" == codex/* ]]; then
  echo "error: refusing to create or attach a codex/... branch" >&2
  exit 2
fi

log() { printf '%s\n' "$*"; }
run() {
  if [[ "$DRY_RUN" -eq 1 ]]; then
    log "dry-run: $*"
  else
    "$@"
  fi
}

resolve_dev() {
  if [[ -n "$DEV_ROOT" ]]; then
    printf '%s\n' "$DEV_ROOT"
    return
  fi
  if [[ -d "$DEFAULT_DEV/.git" || -f "$DEFAULT_DEV/.git" ]]; then
    printf '%s\n' "$DEFAULT_DEV"
    return
  fi
  if git rev-parse --show-toplevel >/dev/null 2>&1; then
    local root common
    root="$(git rev-parse --show-toplevel)"
    while IFS= read -r line; do
      case "$line" in
        worktree\ *)
          wt="${line#worktree }"
          if [[ "$(basename "$wt")" == "gitserious-DEV" ]]; then
            printf '%s\n' "$wt"
            return
          fi
          ;;
      esac
    done < <(git -C "$root" worktree list --porcelain)
  fi
  echo "error: gitserious-DEV not found; pass --dev <path>" >&2
  exit 1
}

is_preserved_untracked() {
  local path="$1"
  local name
  name="$(basename "$path")"
  local item
  for item in "${PRESERVE_UNTRACKED[@]}"; do
    if [[ "$path" == "$item" || "$path" == "$item"/* || "$name" == "$item" ]]; then
      return 0
    fi
  done
  return 1
}

# Exit 0 when porcelain shows only understood non-blocking untracked paths.
source_is_acceptable() {
  local path="$1"
  local line status rel
  while IFS= read -r line; do
    [[ -z "$line" ]] && continue
    status="${line:0:2}"
    rel="${line:3}"
    # Rename lines look like "R  old -> new"; treat as blocking.
    if [[ "$status" != "??" ]]; then
      return 1
    fi
    if ! is_preserved_untracked "$rel"; then
      return 1
    fi
  done < <(git -C "$path" status --porcelain)
  return 0
}

hash_preserved_untracked() {
  local path="$1"
  local line rel
  while IFS= read -r line; do
    [[ -z "$line" ]] && continue
    rel="${line:3}"
    if [[ "${line:0:2}" == "??" ]] && is_preserved_untracked "$rel"; then
      if [[ -d "$path/$rel" ]]; then
        log "preserve-dir: $rel"
      elif [[ -f "$path/$rel" ]]; then
        log "preserve-hash: $rel $(git -C "$path" hash-object "$rel")"
      fi
    fi
  done < <(git -C "$path" status --porcelain)
}

destination_prunable() {
  local dest="$1"
  local abs parent
  parent="$(dirname "$dest")"
  if [[ -d "$parent" ]]; then
    abs="$(cd "$parent" && pwd)/$(basename "$dest")"
  else
    abs="$dest"
  fi
  local in_entry=0 wt="" prunable=0
  while IFS= read -r line || [[ -n "$line" ]]; do
    case "$line" in
      worktree\ *)
        if [[ "$in_entry" -eq 1 && "$prunable" -eq 1 && "$wt" == "$abs" ]]; then
          return 0
        fi
        wt="${line#worktree }"
        in_entry=1
        prunable=0
        ;;
      prunable*)
        if [[ "$in_entry" -eq 1 ]]; then
          prunable=1
        fi
        ;;
      "")
        if [[ "$in_entry" -eq 1 && "$prunable" -eq 1 && "$wt" == "$abs" ]]; then
          return 0
        fi
        in_entry=0
        wt=""
        prunable=0
        ;;
    esac
  done < <(git -C "$DEV" worktree list --porcelain)
  if [[ "$in_entry" -eq 1 && "$prunable" -eq 1 && "$wt" == "$abs" ]]; then
    return 0
  fi
  return 1
}

DEV="$(cd "$(resolve_dev)" && pwd)"
log "dev: $DEV"
log "destination: $DEST"
log "branch: $BRANCH"

log ""
log "== inspect =="
git -C "$DEV" status --short --branch
git -C "$DEV" worktree list --porcelain
git -C "$DEV" branch -a --list "$BRANCH" "origin/$BRANCH" || true

if ! source_is_acceptable "$DEV"; then
  echo "error: gitserious-DEV has tracked changes or unexpected untracked files" >&2
  git -C "$DEV" status --short --branch >&2
  exit 1
fi

log ""
log "== fetch =="
run git -C "$DEV" fetch origin --prune

LEFT_RIGHT="$(git -C "$DEV" rev-list --left-right --count HEAD...origin/dev)"
# Format is "<ahead><TAB><behind>".
LEFT="${LEFT_RIGHT%%[$'\t\n']*}"
RIGHT="${LEFT_RIGHT#*$'\t'}"
log "HEAD...origin/dev left-right: $LEFT_RIGHT (ahead=$LEFT behind=$RIGHT)"

HEAD_SHA="$(git -C "$DEV" rev-parse HEAD)"
ORIGIN_DEV="$(git -C "$DEV" rev-parse origin/dev)"

if [[ "$HEAD_SHA" == "$ORIGIN_DEV" ]]; then
  log "DEV already at origin/dev ($ORIGIN_DEV)"
elif [[ "$LEFT" == "0" && "$RIGHT" != "0" ]]; then
  log "DEV behind origin/dev; fast-forward"
  run git -C "$DEV" merge --ff-only origin/dev
elif [[ "$LEFT" != "0" ]]; then
  log "DEV diverges from origin/dev (ahead=$LEFT behind=$RIGHT)"
  if git -C "$DEV" diff --quiet HEAD origin/dev; then
    log "tracked trees match (squash or rewritten history likely)"
    if [[ "$ALLOW_EQUIV_RESET" -ne 1 ]]; then
      echo "error: equivalent tracked tree but diverged history; re-run with --allow-equivalent-reset or choose an integration strategy" >&2
      exit 1
    fi
    ARCHIVE="archive/dev-$(git -C "$DEV" rev-parse --short HEAD)"
    log "archiving current tip as $ARCHIVE"
    hash_preserved_untracked "$DEV"
    run git -C "$DEV" branch "$ARCHIVE" HEAD
    run git -C "$DEV" reset --keep origin/dev
  else
    echo "error: DEV diverges from origin/dev with tracked differences; stop for user direction" >&2
    git -C "$DEV" status --short --branch >&2
    exit 1
  fi
else
  echo "error: unexpected ahead/behind state: $LEFT_RIGHT" >&2
  exit 1
fi

if [[ "$DRY_RUN" -eq 0 ]]; then
  HEAD_SHA="$(git -C "$DEV" rev-parse HEAD)"
  ORIGIN_DEV="$(git -C "$DEV" rev-parse origin/dev)"
  if [[ "$HEAD_SHA" != "$ORIGIN_DEV" ]]; then
    echo "error: DEV HEAD $HEAD_SHA does not match origin/dev $ORIGIN_DEV" >&2
    exit 1
  fi
  if ! git -C "$DEV" diff --quiet HEAD origin/dev; then
    echo "error: tracked tree differs from origin/dev after refresh" >&2
    exit 1
  fi
fi

log ""
log "== worktree registration =="
if destination_prunable "$DEST"; then
  log "destination is prunable; pruning"
  run git -C "$DEV" worktree prune
  git -C "$DEV" worktree list --porcelain
fi

if [[ -e "$DEST" ]]; then
  echo "error: destination already exists: $DEST" >&2
  exit 1
fi

LOCAL_BRANCH_EXISTS=0
if git -C "$DEV" show-ref --verify --quiet "refs/heads/$BRANCH"; then
  LOCAL_BRANCH_EXISTS=1
fi

log ""
log "== create or reattach =="
if [[ "$LOCAL_BRANCH_EXISTS" -eq 1 ]]; then
  log "local branch exists; reattaching"
  run git -C "$DEV" worktree add "$DEST" "$BRANCH"
else
  if [[ "$SKIP_LIVE" -eq 0 ]]; then
    LIVE="$(git -C "$DEV" ls-remote --heads origin refs/heads/dev | awk '{print $1}')"
    ORIGIN_DEV="$(git -C "$DEV" rev-parse origin/dev)"
    HEAD_SHA="$(git -C "$DEV" rev-parse HEAD)"
    log "live origin/dev: ${LIVE:-<empty>}"
    log "cached origin/dev: $ORIGIN_DEV"
    log "DEV HEAD: $HEAD_SHA"
    if [[ -z "$LIVE" || "$LIVE" != "$ORIGIN_DEV" || "$HEAD_SHA" != "$ORIGIN_DEV" ]]; then
      echo "error: local DEV, origin/dev, and live refs/heads/dev do not all match" >&2
      exit 1
    fi
  fi
  log "creating branch $BRANCH from origin/dev at $DEST"
  run git -C "$DEV" worktree add -b "$BRANCH" "$DEST" origin/dev
fi

log ""
log "== verify =="
if [[ "$DRY_RUN" -eq 1 ]]; then
  log "dry-run complete; no worktree created"
  exit 0
fi

git -C "$DEV" status --short --branch
git -C "$DEST" status --short --branch
git -C "$DEV" worktree list --porcelain
log "DEV HEAD: $(git -C "$DEV" rev-parse HEAD)"
log "target HEAD: $(git -C "$DEST" rev-parse HEAD)"
log "target branch: $(git -C "$DEST" branch --show-current)"
log "tests intentionally not run (Git-only worktree operation)"
log "done"
