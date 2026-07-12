#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

for component in check fmt lint test; do
  "$script_dir/run-rust-quality.sh" "$component"
done
