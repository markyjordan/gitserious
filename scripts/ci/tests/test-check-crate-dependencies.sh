#!/usr/bin/env bash
set -euo pipefail
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
checker="$script_dir/check-crate-dependencies.py"
fixture_dir="$(cd "$(mktemp -d)" && pwd -P)"
trap 'rm -rf "$fixture_dir"' EXIT

repo="$fixture_dir/repo"
cat_manifest() {
  local name="$1"
  shift
  printf '[package]\nname = "%s"\nversion = "0.1.0"\nedition = "2024"\n\n[dependencies]\n' "$name"
  for dependency in "$@"; do
    printf '%s = { path = "%s" }\n' "${dependency%%=*}" "${dependency#*=}"
  done
}
write_crate() {
  local name="$1"
  shift
  mkdir -p "$repo/crates/$name/src"
  cat_manifest "$name" "$@" >"$repo/crates/$name/Cargo.toml"
  [[ -f "$repo/crates/$name/src/lib.rs" ]] || : >"$repo/crates/$name/src/lib.rs"
}

mkdir -p "$repo"
printf '[workspace]\nmembers = ["crates/*"]\nresolver = "3"\n' >"$repo/Cargo.toml"
write_crate gitserious-core
write_crate gitserious-app gitserious-core=../gitserious-core
write_crate gitserious-cli gitserious-app=../gitserious-app gitserious-core=../gitserious-core
write_crate gitserious-tui gitserious-app=../gitserious-app gitserious-core=../gitserious-core
write_crate gitserious-fs gitserious-app=../gitserious-app gitserious-core=../gitserious-core
write_crate gitserious gitserious-app=../gitserious-app gitserious-cli=../gitserious-cli \
  gitserious-core=../gitserious-core gitserious-fs=../gitserious-fs gitserious-tui=../gitserious-tui
mkdir -p "$fixture_dir/ratatui/src"
cat_manifest ratatui >"$fixture_dir/ratatui/Cargo.toml"
: >"$fixture_dir/ratatui/src/lib.rs"
git -C "$repo" init -q

run_checker() {
  (cd "$repo" && cargo generate-lockfile --offline -q && python3 "$checker")
}

expect_pass() {
  if ! run_checker >/dev/null 2>"$fixture_dir/stderr"; then
    echo "Expected the crate-dependency fixture to pass: $1" >&2
    cat "$fixture_dir/stderr" >&2
    exit 1
  fi
}

expect_fail_naming() {
  local name="$1" expected="$2"
  if run_checker >/dev/null 2>"$fixture_dir/stderr"; then
    echo "Expected the crate-dependency fixture to fail: $name" >&2
    exit 1
  fi
  if ! grep -qF "$expected" "$fixture_dir/stderr"; then
    echo "Expected '$expected' in the failure for: $name" >&2
    cat "$fixture_dir/stderr" >&2
    exit 1
  fi
}

expect_pass "the permitted graph"

write_crate gitserious-cli gitserious-app=../gitserious-app gitserious-core=../gitserious-core \
  gitserious-tui=../gitserious-tui
expect_fail_naming "an adapter depending on the other adapter" \
  "gitserious-cli depends on gitserious-tui (normal), an edge the architecture forbids"
write_crate gitserious-cli gitserious-app=../gitserious-app gitserious-core=../gitserious-core
expect_pass "the forbidden edge removed"

write_crate gitserious-app gitserious-core=../gitserious-core ratatui=../../../ratatui
expect_fail_naming "the application depending on a terminal crate" \
  "gitserious-app depends on ratatui (normal), but core and app own no presentation"
write_crate gitserious-app gitserious-core=../gitserious-core
expect_pass "the terminal crate removed"

printf '#[cfg(test)]\n#[path = "../../gitserious-app/src/fixture.rs"]\nmod fixture;\n' \
  >"$repo/crates/gitserious-tui/src/lib.rs"
expect_fail_naming "a source inclusion that leaves its crate" \
  "crates/gitserious-tui/src/lib.rs:2 compiles crates/gitserious-app/src/fixture.rs, outside gitserious-tui"
printf '#[cfg(test)]\n#[path = "../tests/fixture.rs"]\nmod fixture;\n' \
  >"$repo/crates/gitserious-tui/src/lib.rs"
expect_pass "a source inclusion inside its own crate"

echo "check-crate-dependencies.py fixtures passed."
