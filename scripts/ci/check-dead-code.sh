#!/usr/bin/env bash
# Fails when crate sources hide or hold dead code: an #[allow(dead_code)] or
# #[allow(unused...)] attribute under crates/*/src, or a public item in a
# scanned crate that no workspace target uses (find-unused-pub.py). Entries in
# dead-code-allowlist.txt are known offenders; an entry that stops offending
# is an error, so each fix must shrink the list.
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

allowlist=scripts/ci/dead-code-allowlist.txt
scanned_crates=(gitserious-tui gitserious-app gitserious-core)

if ! command -v python3 >/dev/null 2>&1; then
  echo "error: missing python3; install it and add it to PATH" >&2
  exit 1
fi

# Prints "<path>\tallow(<lint>) <the item it covers>" for every suppression.
suppressions() {
  { git grep --untracked -nE '^[[:space:]]*#!?\[allow\((dead_code|unused[a-z_]*)' -- 'crates/*/src/**' || true; } |
    while IFS=: read -r path line _; do
      awk -v path="$path" -v line="$line" '
        NR == line { match($0, /allow\([a-z_]+/); lint = substr($0, RSTART, RLENGTH) ")" }
        NR > line && !/^[[:space:]]*#/ {
          sub(/^[[:space:]]+/, ""); sub(/[[:space:]]*\{?[[:space:]]*$/, "")
          print path "\t" lint " " $0; exit
        }
      ' "$path"
    done
}

found="$({ suppressions; python3 "$script_dir/find-unused-pub.py" "${scanned_crates[@]}"; } | sort -u)"
listed="$({ grep -vE '^[[:space:]]*(#|$)' "$allowlist" || true; } | sort -u)"
status=0

while IFS=$'\t' read -r path item; do
  [[ -n "$path" ]] || continue
  if [[ "$item" == allow\(* ]]; then
    echo "error: $path suppresses a dead-code warning ($item); delete the dead code or list it in $allowlist" >&2
  else
    echo "error: $path declares $item, which no workspace target uses; delete it or list it in $allowlist" >&2
  fi
  status=1
done < <(comm -23 <(printf '%s\n' "$found") <(printf '%s\n' "$listed"))

while IFS=$'\t' read -r path item; do
  [[ -n "$path" ]] || continue
  echo "error: $allowlist lists $item in $path, which no longer offends; remove the entry" >&2
  status=1
done < <(comm -13 <(printf '%s\n' "$found") <(printf '%s\n' "$listed"))

if ((status == 0)); then
  echo "No dead-code suppressions or unused public items outside $allowlist."
fi
exit "$status"
