#!/usr/bin/env bash
set -euo pipefail
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
checker="$script_dir/check-dead-code.sh"
fixture_dir="$(cd "$(mktemp -d)" && pwd -P)"
trap 'rm -rf "$fixture_dir"' EXIT

repo="$fixture_dir/repo"
mkdir -p "$repo/scripts/ci" "$repo/crates/gitserious/src"
for crate in gitserious-core gitserious-app gitserious-tui; do
  mkdir -p "$repo/crates/$crate/src"
done

cat >"$repo/Cargo.toml" <<'EOF'
[workspace]
members = ["crates/*"]
resolver = "3"
EOF
manifest() {
  printf '[package]\nname = "%s"\nversion = "0.1.0"\nedition = "2024"\n\n[dependencies]\n' "$1"
  shift
  for dependency in "$@"; do
    printf '%s = { path = "../%s" }\n' "$dependency" "$dependency"
  done
}
manifest gitserious-core >"$repo/crates/gitserious-core/Cargo.toml"
manifest gitserious-app gitserious-core >"$repo/crates/gitserious-app/Cargo.toml"
manifest gitserious-tui >"$repo/crates/gitserious-tui/Cargo.toml"
manifest gitserious gitserious-app gitserious-core >"$repo/crates/gitserious/Cargo.toml"

cat >"$repo/crates/gitserious-core/src/lib.rs" <<'EOF'
mod values;

pub use values::{
    Failure,
    Grouped,
    Listed,
    Unlisted,
};

macro_rules! identifier {
    ($name:ident) => {
        pub struct $name;
    };
}

identifier!(Generated);

pub fn reached() -> u8 {
    1
}

pub fn unreached() -> u8 {
    2
}

pub fn attempt() -> Result<u8, Failure<u8>> {
    Err(Failure(3))
}
EOF
cat >"$repo/crates/gitserious-core/src/values.rs" <<'EOF'
pub struct Failure<E>(pub E);
pub struct Grouped;
pub struct Listed;
pub struct Unlisted;
EOF
cat >"$repo/crates/gitserious-app/src/lib.rs" <<'EOF'
pub fn run() -> u8 {
    let _ = (gitserious_core::Grouped, gitserious_core::Listed, gitserious_core::Generated);
    gitserious_core::reached() + gitserious_core::attempt().unwrap_or(0)
}
EOF
printf '\n' >"$repo/crates/gitserious-tui/src/lib.rs"
printf 'fn main() {\n    std::process::exit(i32::from(gitserious_app::run()));\n}\n' \
  >"$repo/crates/gitserious/src/main.rs"
: >"$repo/scripts/ci/dead-code-allowlist.txt"
git -C "$repo" init -q

run_checker() {
  (cd "$repo" && CARGO_TARGET_DIR="$fixture_dir/target" bash "$checker")
}

expect_pass() {
  if ! run_checker >/dev/null 2>"$fixture_dir/stderr"; then
    echo "Expected dead-code fixture to pass: $1" >&2
    cat "$fixture_dir/stderr" >&2
    exit 1
  fi
}

expect_fail_naming() {
  local name="$1"
  shift
  if run_checker >/dev/null 2>"$fixture_dir/stderr"; then
    echo "Expected dead-code fixture to fail: $name" >&2
    exit 1
  fi
  for expected in "$@"; do
    if ! grep -qF "$expected" "$fixture_dir/stderr"; then
      echo "Expected '$expected' in the failure for: $name" >&2
      cat "$fixture_dir/stderr" >&2
      exit 1
    fi
  done
}

expect_fail_naming "a public function no target calls" \
  "crates/gitserious-core/src/lib.rs declares unreached, which no workspace target uses" \
  "crates/gitserious-core/src/values.rs declares Unlisted, which no workspace target uses"
for used in reached Grouped Listed Generated Failure attempt; do
  if grep -qF "declares $used," "$fixture_dir/stderr"; then
    echo "A used item was reported as unused: $used" >&2
    exit 1
  fi
done

printf 'crates/gitserious-core/src/%s\t%s\n' lib.rs Unlisted lib.rs unreached values.rs Unlisted \
  >"$repo/scripts/ci/dead-code-allowlist.txt"
expect_pass "the unused function is allowlisted"

printf '#[allow(dead_code)]\nfn hidden() {}\n' >>"$repo/crates/gitserious-app/src/lib.rs"
expect_fail_naming "a dead-code suppression in crate sources" \
  "crates/gitserious-app/src/lib.rs suppresses a dead-code warning (allow(dead_code) fn hidden() {})"

printf 'pub fn run() -> u8 {\n    let _ = (gitserious_core::Grouped, gitserious_core::Listed, gitserious_core::Generated, gitserious_core::Unlisted);\n    gitserious_core::reached() + gitserious_core::unreached() + gitserious_core::attempt().unwrap_or(0)\n}\n' \
  >"$repo/crates/gitserious-app/src/lib.rs"
expect_fail_naming "allowlist entries that no longer offend" \
  "lists unreached in crates/gitserious-core/src/lib.rs, which no longer offends" \
  "lists Unlisted in crates/gitserious-core/src/values.rs, which no longer offends"

echo "check-dead-code.sh fixtures passed."
