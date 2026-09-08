#!/usr/bin/env bash
# Source this file, then call require_pinned_toolchain before Rust operations.
GITSERIOUS_TOOLCHAIN_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

read_rust_toolchain_pin() {
  local root pin
  root="$GITSERIOUS_TOOLCHAIN_ROOT"
  pin="$(awk '
    /^[[:space:]]*\[/ { section = ($0 ~ /^[[:space:]]*\[toolchain\][[:space:]]*(#.*)?$/) }
    section && /^[[:space:]]*channel[[:space:]]*=/ {
      count++; line=$0
      if (line !~ /^[[:space:]]*channel[[:space:]]*=[[:space:]]*"[0-9]+\.[0-9]+\.[0-9]+"[[:space:]]*(#.*)?$/) invalid=1
      sub(/^[^=]*=[[:space:]]*"/, "", line)
      sub(/"[[:space:]]*(#.*)?$/, "", line)
      value=line
    }
    END { if (count == 1 && !invalid) print value }
  ' "$root/rust-toolchain.toml")" || return 1
  if [[ ! "$pin" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "error: expected one exact Rust version in $root/rust-toolchain.toml" >&2
    return 1
  fi
  printf '%s\n' "$pin"
}

find_rustup() {
  local rustup_command
  if [[ -n "${CARGO_HOME:-}" && -x "$CARGO_HOME/bin/rustup" ]]; then
    rustup_command="$CARGO_HOME/bin/rustup"
  elif rustup_command="$(type -P rustup)"; then
    :
  elif [[ -x "${HOME}/.cargo/bin/rustup" ]]; then
    rustup_command="${HOME}/.cargo/bin/rustup"
  else
    echo "error: install rustup or expose it through CARGO_HOME/bin or PATH" >&2
    return 1
  fi
  printf '%s\n' "$rustup_command"
}

require_pinned_toolchain() {
  local pin rustup_command compiler tool_dir path_dir override tool version resolved
  pin="$(read_rust_toolchain_pin)" || return 1
  override="${CARGO:-}"
  if [[ -n "$override" ]]; then
    override="$(type -P "$override")" || {
      echo "error: CARGO must name one executable" >&2; return 1;
    }
    override="$(cd "$(dirname "$override")" && pwd)/$(basename "$override")"
  fi
  rustup_command="$(find_rustup)" || return 1
  compiler="$("$rustup_command" which --toolchain "$pin" rustc)" || {
    echo "error: install the pinned toolchain with rustup toolchain install $pin --component clippy --component rustfmt" >&2
    return 1
  }
  # Rustup emits native Windows paths; mixed paths work in Bash and Cargo.
  if command -v cygpath >/dev/null 2>&1; then
    compiler="$(cygpath -m "$compiler")" || return 1
    if [[ -n "$override" ]]; then
      override="$(cygpath -m "$override")" || return 1
    fi
  fi
  tool_dir="$(cd "$(dirname "$compiler")" && pwd)" || return 1
  path_dir="$tool_dir"
  if command -v cygpath >/dev/null 2>&1; then
    tool_dir="$(cygpath -m "$tool_dir")" || return 1
  fi
  for tool in cargo rustc rustdoc cargo-clippy clippy-driver cargo-fmt rustfmt; do
    if [[ ! -x "$tool_dir/$tool" && ! -x "$tool_dir/$tool.exe" ]]; then
      echo "error: missing pinned $tool; run rustup toolchain install $pin --component clippy --component rustfmt" >&2
      return 1
    fi
  done
  export PATH="$path_dir:$PATH"
  export RUSTUP_TOOLCHAIN="$pin"
  export RUSTC="$compiler"
  export RUSTDOC="$tool_dir/rustdoc"
  export CARGO="${override:-$tool_dir/cargo}"
  for tool in cargo rustc; do
    resolved="$CARGO"
    [[ "$tool" != rustc ]] || resolved="$RUSTC"
    version="$("$resolved" --version)" || return 1
    if [[ "$version" != "$tool $pin" && "$version" != "$tool $pin "* ]]; then
      echo "error: expected $tool $pin, got '$version' from $resolved" >&2
      return 1
    fi
  done
}
