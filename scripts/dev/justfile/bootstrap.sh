#!/usr/bin/env bash

set -euo pipefail

REQUIRED_COMMANDS=(
  rustc
  cargo
)

# Fails fast when a bootstrap prerequisite is not available on PATH.
require_command() {
  local command_name="$1"

  if ! command -v "${command_name}" >/dev/null 2>&1; then
    echo "error: missing required command \`${command_name}\`" >&2
    echo "help: a working Rust toolchain is required; install Rust or make \`${command_name}\` available on PATH, then rerun \`just bootstrap\`" >&2
    exit 1
  fi
}

for command_name in "${REQUIRED_COMMANDS[@]}"; do
  require_command "${command_name}"
done

rustc --version
cargo --version

if command -v rustup >/dev/null 2>&1; then
  rustup show active-toolchain
fi

cargo fetch
