#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

for component in check fmt lint test release; do
  actual="$(RUST_QUALITY_RUNNER=/bin/echo "$script_dir/run-quality.sh" "$component")"
  if [[ "$actual" != "$component" ]]; then
    echo "run-quality.sh dispatched '$component' as '$actual'" >&2
    exit 1
  fi
done

if RUST_QUALITY_RUNNER=/bin/echo "$script_dir/run-quality.sh" unknown >/dev/null 2>&1; then
  echo "run-quality.sh accepted an unknown category" >&2
  exit 1
fi

echo "run-quality.sh dispatch fixtures passed."
