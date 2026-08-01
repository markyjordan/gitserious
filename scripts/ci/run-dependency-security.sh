#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

component="${1:-}"
case "$component" in
  cargo-audit)
    command -v cargo-audit >/dev/null
    cargo audit --file Cargo.lock
    ;;
  *)
    echo "Usage: $0 <cargo-audit>" >&2
    exit 2
    ;;
esac
