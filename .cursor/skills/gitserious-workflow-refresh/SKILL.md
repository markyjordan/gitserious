---
name: gitserious-workflow-refresh
description: >-
  Refresh local gitserious worktrees after a marky/* topic PR merges into
  origin/dev. Use when asked to refresh DEV, repurpose gitserious-feat/fix/chore
  worktrees, sync after merge, or run /gitserious-workflow-refresh.
disable-model-invocation: true
---

# gitserious worktree refresh

Run this after a `marky/<type>/<name>` PR merges into `origin/dev`. Bring
`gitserious-DEV` up to `origin/dev`, then reset each sibling topic pool
worktree onto the same tip so the directory keeps its name for the next topic.

## Layout

Sibling directories on the local machine (not inside the Cloud Agent checkout):

| Directory | Role | Branch |
| --- | --- | --- |
| `gitserious-DEV/` | Integration / default | `dev` tracks `origin/dev` |
| `gitserious-feat/` | Reusable feat pool | next: `marky/feat/<name>` |
| `gitserious-fix/` | Reusable fix pool | next: `marky/fix/<name>` |
| `gitserious-chore/` | Reusable chore pool | next: `marky/chore/<name>` |
| `gitserious-<type>/` | Other type pools | next: `marky/<type>/<name>` |

Discover paths with `git worktree list`. Never invent worktree paths by hand.

## When to use

- A topic PR just merged into `dev` / `origin/dev`.
- The user asks to refresh DEV, refresh worktrees, or repurpose a pool dir.
- Slash invoke: `/gitserious-workflow-refresh`.

## When not to use

- Creating the first worktree set (use `git worktree add` once; this skill only refreshes).
- Deleting unused worktrees to free disk (that is a different cleanup task).
- Cloud Agent VMs that have only `/workspace` and no sibling pools. Say the skill needs the local sibling layout and stop.

## Instructions

1. Confirm the merge landed on `origin/dev` (`git fetch origin` and inspect the tip, or the merged PR).
2. Prefer GitHub "delete branch on merge" so squash merges are detectable after `fetch --prune`.
3. From any registered worktree of this repo, run the lever:

```bash
bash .cursor/skills/gitserious-workflow-refresh/scripts/refresh-worktrees.sh
```

Useful flags:

```bash
# Preview only
bash .cursor/skills/gitserious-workflow-refresh/scripts/refresh-worktrees.sh --dry-run

# Sibling parent is not auto-detected
bash .cursor/skills/gitserious-workflow-refresh/scripts/refresh-worktrees.sh --parent /path/to/parent

# Dirty tree or unmerged topic (destructive)
bash .cursor/skills/gitserious-workflow-refresh/scripts/refresh-worktrees.sh --force
```

4. The script must exit 0. If it refuses a dirty or unmerged topic worktree, stop and report the path. Do not hand-roll a reset unless the user asks.
5. After success, each topic pool is detached at `origin/dev`. Start the next topic with:

```bash
git -C /path/to/gitserious-<type> switch -c marky/<type>/<name>
```

## Safety

- Default mode never discards uncommitted work.
- Default mode never resets a `marky/*` branch that is still open (tip not in `origin/dev`, remote branch still present, and no merged PR via `gh`).
- `--force` is irreversible for local dirty state. Pause and get an explicit ask before using it unless the user already said to force.
- Do not delete remote branches from this skill. PR merge settings own that.

## Expected output

Report, in order:

1. `origin/dev` SHA after fetch.
2. DEV path and whether it fast-forwarded or hard-reset.
3. Each topic path refreshed, the old `marky/*` branch deleted (or kept), and the next `git switch -c` hint.
4. Any skipped path with the concrete reason.
