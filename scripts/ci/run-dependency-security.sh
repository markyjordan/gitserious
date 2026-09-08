#!/usr/bin/env bash
set -euo pipefail

# shellcheck source=scripts/shared/rust-toolchain.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/shared/rust-toolchain.sh"
require_pinned_toolchain

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

component="${1:-}"
case "$component" in
  cargo-audit)
    command -v cargo-audit >/dev/null
    "$CARGO" audit --file Cargo.lock
    ;;
  *)
    echo "Usage: $0 <cargo-audit>" >&2
    exit 2
    ;;
esac
