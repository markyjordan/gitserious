#!/usr/bin/env bash

set -euo pipefail

# shellcheck source=scripts/shared/rust-toolchain.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)/shared/rust-toolchain.sh"
require_pinned_toolchain

readonly CARGO_RECIPE="${1:-}"

if [[ -z "${CARGO_RECIPE}" ]]; then
  echo "error: expected a Cargo recipe: build or run" >&2
  exit 1
fi

shift

case "${CARGO_RECIPE}" in
  build)
    "$CARGO" build --locked --workspace "$@"
    ;;
  run)
    "$CARGO" run --locked --package gitserious --bin gitserious -- "$@"
    ;;
  *)
    echo "error: unsupported Cargo recipe \`${CARGO_RECIPE}\`" >&2
    echo "help: expected build or run" >&2
    exit 1
    ;;
esac
