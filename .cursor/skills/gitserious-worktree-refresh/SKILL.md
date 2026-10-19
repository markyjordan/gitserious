---
name: gitserious-worktree-refresh
description: >-
  Refresh gitserious-DEV from origin/dev, then safely create or reattach a named
  sibling worktree. Use for requests mentioning origin/dev, git worktree,
  prunable registrations, or marky/... branches. Args: <destination> <branch>.
disable-model-invocation: true
---

# Gitserious worktree refresh

## When to use

Use for a requested Gitserious sibling worktree after the user has authorized
Git changes, especially when they say to update `gitserious-DEV` first. The
destination path is the sibling directory. The branch is the existing or
desired local branch (`marky/<type>/<name>`).

Do not use for a published topic branch that merely needs `origin/dev` merged
into it. Use the public-topic merge policy instead. Do not create a `codex/...`
branch.

## Default DEV root

Start in:

```text
/Users/marky/Documents/Chia/marky++/projects/oss/cli/gitserious-DEV
```

Override with `--dev <path>` when the checkout lives elsewhere (Cloud fixtures,
another machine). Discover via `git worktree list` when the default path is
absent.

## Inputs / context to gather

1. Start in the DEV root above (or the override).
2. Inspect `git status --short --branch`, `git worktree list --porcelain`, and
   `git branch -a --list "$branch" "origin/$branch"`.
3. Confirm the source is clean or has only understood non-blocking untracked
   files, the destination is absent or understood, and whether the branch
   already exists locally. Preserve unrelated untracked files such as
   `.gitserious/`, `gitserious.toml`, or `gitserious.lock`. Before integrating
   `origin/dev`, confirm they are not paths tracked by the incoming tree. Do
   not clean or stash them unless they block the requested operation.

## Procedure

Prefer the lever. From any registered worktree of this repo (or with `--dev`):

```bash
bash .cursor/skills/gitserious-worktree-refresh/scripts/refresh-worktree.sh \
  <destination> <branch>
```

The script encodes the steps below. Hand-run only when debugging the lever.

1. Run `git fetch origin --prune` and compare `HEAD...origin/dev` with
   `git rev-list --left-right --count HEAD...origin/dev`.
2. If clean and behind without divergence, run `git merge --ff-only origin/dev`.
   Do not create a merge commit or rewrite `dev`.
3. If the checkout diverges after a squash merge, do not treat it as behind-only.
   First prove `git diff --quiet HEAD origin/dev` and inspect ancestry. Only if
   the tracked tree is equivalent, preserve the old topic history under an
   explicit archive branch, hash understood untracked files, then use
   `git reset --keep origin/dev`. Otherwise stop for user direction.
4. If the destination is listed as `prunable`, run `git worktree prune` and
   inspect the registration again.
5. If the local branch exists, reattach it: `git worktree add "$dest" "$branch"`.
   Otherwise, after confirming local `dev`, cached `origin/dev`, and live
   `git ls-remote --heads origin refs/heads/dev` match, create it directly from
   the verified remote base:
   `git worktree add -b "$branch" "$dest" origin/dev`.
6. Verify the source and target `git status --short --branch`,
   `git worktree list --porcelain`, and expected refs. When the user asks for
   exact `origin/dev` freshness or challenges a prior result, also run
   `git ls-remote --heads origin refs/heads/dev` and compare it with local
   `HEAD` and `origin/dev`. For a refresh, also prove
   `git diff --quiet HEAD origin/dev` and confirm preserved untracked files
   remain. A deleted topic remote may correctly show `[gone]`.

### Flags

```bash
# Preview only
bash .cursor/skills/gitserious-worktree-refresh/scripts/refresh-worktree.sh \
  --dry-run <destination> <branch>

# Non-default DEV checkout
bash .cursor/skills/gitserious-worktree-refresh/scripts/refresh-worktree.sh \
  --dev /path/to/gitserious-DEV <destination> <branch>

# Allow equivalent-tree squash realignment (creates archive/<branch>-<shortsha>)
bash .cursor/skills/gitserious-worktree-refresh/scripts/refresh-worktree.sh \
  --allow-equivalent-reset <destination> <branch>
```

## Efficiency plan

1. Use the three initial inspection commands before attempting creation. They
   catch dirty source, collision, existing-branch, and stale-registration cases
   in one pass.
2. Do not run tests for a synchronization or worktree-only task unless the user
   requests them.
3. Stop and report if tracked changes make `dev` dirty or it diverges. Do not
   choose an integration strategy without user direction. A known unrelated
   untracked directory is not by itself a reason to alter the checkout.

## Pitfalls and fixes

- `FETCH_HEAD` or `ORIG_HEAD.lock` says `Operation not permitted`: linked-worktree
  metadata needs access. Retry the same Git command with the needed access.
  Never edit `.git/worktrees` manually.
- `git ls-remote` says `Could not resolve host: github.com`: retry the
  network-dependent command with the needed network access before reporting a
  live-ref result.
- `git worktree add` conflicts despite a missing directory: inspect for
  `prunable`, then prune the stale registration.
- An existing requested branch is passed to `-b`: do not recreate it. Use
  `git worktree add <destination> <branch>` to preserve history.
- Divergence after a squash merge: do not reset from counts alone. Prove
  tracked-tree equivalence with `git diff --quiet HEAD origin/dev`, inspect
  ancestry, preserve the old branch and understood untracked files, or stop
  for direction.

## Verification checklist

1. `gitserious-DEV` is clean and aligned with cached and, when requested, live
   `origin/dev`. The tracked tree matches and understood untracked files remain
   when that was requested.
2. The sibling path, branch name, and worktree registration exactly match the
   request.
3. No history was rewritten and no unexpected branch was created.
4. Report whether tests were intentionally not run for this Git-only operation.
