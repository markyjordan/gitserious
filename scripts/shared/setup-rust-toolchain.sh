#!/usr/bin/env bash
set -euo pipefail

target=""
if (($#)); then
  if [[ $# != 2 || "$1" != --target || ! "$2" =~ ^[a-zA-Z0-9_]+(-[a-zA-Z0-9_.]+)+$ ]]; then
    echo "Usage: $0 [--target <triple>]" >&2
    exit 2
  fi
  target="$2"
fi

# shellcheck source=scripts/shared/rust-toolchain.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/rust-toolchain.sh"
pin="$(read_rust_toolchain_pin)"
rustup_command="$(find_rustup)"
"$rustup_command" toolchain install "$pin" \
  --profile minimal --component clippy,rustfmt --no-self-update
if [[ -n "$target" ]]; then
  "$rustup_command" target add --toolchain "$pin" "$target"
fi
require_pinned_toolchain
"$CARGO" --version
"$RUSTC" --version
