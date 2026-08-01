#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
rust_quality_runner="${RUST_QUALITY_RUNNER:-$script_dir/run-rust-quality.sh}"

component="${1:-}"
case "$component" in
  check | fmt | lint | test | release)
    "$rust_quality_runner" "$component"
    ;;
  *)
    echo "Usage: $0 <check|fmt|lint|test|release>" >&2
    exit 2
    ;;
esac
