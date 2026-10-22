#!/usr/bin/env bash
set -euo pipefail
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
checker="$script_dir/check-compiled-sources.sh"
fixture_dir="$(cd "$(mktemp -d)" && pwd -P)"
trap 'rm -rf "$fixture_dir"' EXIT

repo="$fixture_dir/repo"
deps="$fixture_dir/target/debug/deps"
messages="$fixture_dir/messages.jsonl"
mkdir -p "$repo/crates/demo/src" "$repo/crates/demo/tests/unit" "$deps"

printf 'mod live;\n// mod dead;\n' >"$repo/crates/demo/src/lib.rs"
printf 'pub fn live() {}\n' >"$repo/crates/demo/src/live.rs"
printf 'pub fn dead() {}\n' >"$repo/crates/demo/src/dead.rs"
printf '#![cfg(any())]\n' >"$repo/crates/demo/tests/gated.rs"
printf '#[test]\nfn unit() {}\n' >"$repo/crates/demo/tests/unit/mod.rs"
git -C "$repo" init -q
git -C "$repo" add .

cat >"$deps/demo-1.d" <<EOF
$deps/libdemo-1.rmeta: crates/demo/src/lib.rs crates/demo/src/live.rs crates/demo/src/../tests/unit/mod.rs

crates/demo/src/lib.rs:
crates/demo/src/live.rs:
crates/demo/src/../tests/unit/mod.rs:
EOF
cat >"$deps/gated-1.d" <<EOF
$deps/libgated-1.rmeta: $repo/crates/demo/tests/gated.rs
EOF
cat >"$deps/registry-1.d" <<EOF
$deps/libregistry-1.rmeta: crates/demo/src/dead.rs
EOF
cat >"$messages" <<EOF
{"reason":"compiler-artifact","package_id":"path+file://$repo/crates/demo#0.1.0","filenames":["$deps/libdemo-1.rmeta"]}
{"reason":"compiler-artifact","package_id":"path+file://$repo/crates/demo#0.1.0","filenames":["$deps/libgated-1.rmeta"]}
{"reason":"compiler-artifact","package_id":"registry+https://github.com/rust-lang/crates.io-index#registry@1.0.0","filenames":["$deps/libregistry-1.rmeta"]}
{"reason":"build-finished","success":true}
EOF

run_checker() {
  (cd "$repo" && env -i PATH="$PATH" HOME="${HOME:-/tmp}" bash "$checker" "$messages")
}

expect_pass() {
  if ! run_checker >/dev/null 2>"$fixture_dir/stderr"; then
    echo "Expected compiled-sources fixture to pass: $1" >&2
    cat "$fixture_dir/stderr" >&2
    exit 1
  fi
}

expect_fail_naming() {
  local name="$1"
  shift
  if run_checker >/dev/null 2>"$fixture_dir/stderr"; then
    echo "Expected compiled-sources fixture to fail: $name" >&2
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

expect_fail_naming "uncompiled, gated, and unregistered files" \
  "crates/demo/src/dead.rs is left out of the build (never compiled)" \
  "crates/demo/tests/gated.rs is left out of the build (gated by cfg(any()))" \
  "crates/demo/src/lib.rs is left out of the build (commented-out mod)"
if grep -qF "crates/demo/tests/unit/mod.rs" "$fixture_dir/stderr"; then
  echo "A file reached through src/../tests was reported as never compiled" >&2
  exit 1
fi

git -C "$repo" rm -q --cached crates/demo/src/dead.rs crates/demo/tests/gated.rs
printf 'mod live;\n' >"$repo/crates/demo/src/lib.rs"
expect_pass "every tracked source compiled"

printf '{"reason":"build-finished","success":true}\n' >"$messages"
expect_fail_naming "messages without workspace artifacts" "names no workspace .rmeta artifacts"

echo "check-compiled-sources.sh fixtures passed."
