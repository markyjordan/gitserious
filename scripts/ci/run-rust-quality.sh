#!/usr/bin/env bash
set -euo pipefail

# shellcheck source=scripts/shared/rust-toolchain.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/shared/rust-toolchain.sh"
require_pinned_toolchain

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

if [[ ! -f Cargo.lock ]]; then
  echo "Cargo.lock is required for locked CI dependency resolution." >&2
  exit 1
fi

run_check() {
  "$CARGO" metadata --locked --no-deps --format-version 1 >/dev/null
  "$CARGO" check --locked --workspace --all-targets --all-features
}

run_fmt() {
  "$CARGO" fmt --all --check
}

run_lint() {
  "$CARGO" clippy --locked --workspace --all-targets --all-features -- -D warnings
}

run_test() {
  "$CARGO" test --locked --workspace --all-targets --all-features
  "$CARGO" test --locked --workspace --doc
}

run_release() {
  "$CARGO" build --locked --workspace --all-targets --all-features --release
  "$CARGO" package --locked --workspace --list
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
