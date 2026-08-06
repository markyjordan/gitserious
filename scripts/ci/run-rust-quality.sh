#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

if [[ ! -f Cargo.lock ]]; then
  echo "Cargo.lock is required for locked CI dependency resolution." >&2
  exit 1
fi

run_check() {
  cargo metadata --locked --no-deps --format-version 1 >/dev/null
  cargo check --locked --workspace --all-targets --all-features
}

run_fmt() {
  cargo fmt --all --check
}

run_lint() {
  cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
}

run_test() {
  cargo test --locked --workspace --all-targets --all-features
  cargo test --locked --workspace --doc
}

run_release() {
  cargo build --locked --workspace --all-targets --all-features --release
  cargo package --locked --workspace --list
}

component="${1:-}"
case "$component" in
  check) run_check ;;
  fmt) run_fmt ;;
  lint) run_lint ;;
  test) run_test ;;
  release) run_release ;;
  *)
    echo "Usage: $0 <check|fmt|lint|test|release>" >&2
    exit 2
    ;;
esac
