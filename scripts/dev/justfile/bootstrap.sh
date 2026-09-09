#!/usr/bin/env bash
set -euo pipefail

scripts_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
bash "$scripts_root/shared/setup-rust-toolchain.sh"
# Setup runs in a child process; select tools again in this process.
# shellcheck source=scripts/shared/rust-toolchain.sh
source "$scripts_root/shared/rust-toolchain.sh"
require_pinned_toolchain
cd "$GITSERIOUS_TOOLCHAIN_ROOT"
"$CARGO" fetch --locked
