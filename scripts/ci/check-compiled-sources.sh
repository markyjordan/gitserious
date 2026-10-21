#!/usr/bin/env bash
# Fails when a tracked Rust file under crates/ is left out of the build: no
# target compiles it, #![cfg(any())] compiles it to nothing, or it holds a
# commented-out mod. The input is the JSON message stream of
# `cargo check --workspace --all-targets --message-format=json`.
set -euo pipefail

messages="${1:?usage: check-compiled-sources.sh <cargo check JSON messages>}"
messages="$(cd "$(dirname "$messages")" && pwd)/$(basename "$messages")"

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

if ! command -v jq >/dev/null 2>&1; then
  echo "error: missing jq; install it and add it to PATH" >&2
  exit 1
fi

# Each workspace artifact's .rmeta sits next to the rustc dep-info that lists
# every source file the target read.
dep_info_files() {
  jq -r '
    select(.reason == "compiler-artifact" and (.package_id | startswith("path+file://")))
    | .filenames[]
    | select(endswith(".rmeta"))
  ' "$messages" | sed -E 's#/lib([^/]+)\.rmeta$#/\1.d#'
}

compiled_sources() {
  local dep_info found=0
  while IFS= read -r dep_info; do
    found=1
    tr ' ' '\n' <"$dep_info"
  done < <(dep_info_files)
  if ((found == 0)); then
    echo "error: $messages names no workspace .rmeta artifacts; pass cargo check JSON output" >&2
    return 1
  fi
}

normalize_paths() {
  awk -v root="$repo_root/" '
    {
      sub(/:$/, "")
      if (index($0, root) == 1) $0 = substr($0, length(root) + 1)
      n = split($0, parts, "/")
      depth = 0
      for (i = 1; i <= n; i++) {
        if (parts[i] == "..") { if (depth > 0) depth-- }
        else if (parts[i] != "." && parts[i] != "") kept[++depth] = parts[i]
      }
      path = ""
      for (i = 1; i <= depth; i++) path = path (i > 1 ? "/" : "") kept[i]
      if (path ~ /^crates\//) print path
    }
  '
}

label() {
  awk -v reason="$1" '{ print $0 "\t" reason }'
}

# Prints "<path>\t<reason>" for every offending file.
offenders() {
  git ls-files 'crates/*.rs' | sort | comm -23 - <(printf '%s\n' "$compiled") |
    label 'never compiled'
  git grep -l -F 'cfg(any())' -- 'crates/*.rs' | label 'gated by cfg(any())' || true
  git grep -l -E '^[[:space:]]*//[[:space:]]*(pub(\([a-z]+\))?[[:space:]]+)?mod[[:space:]]' -- 'crates/*.rs' |
    label 'commented-out mod' || true
}

compiled="$(compiled_sources | normalize_paths | sort -u)"
status=0

while IFS=$'\t' read -r path reason; do
  [[ -n "$path" ]] || continue
  echo "error: $path is left out of the build ($reason); compile it or delete it" >&2
  status=1
done < <(offenders | sort -u)

if ((status == 0)); then
  echo "Every Rust source under crates/ is compiled."
fi
exit "$status"
