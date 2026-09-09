#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
fixture="$(mktemp -d)"
trap 'rm -rf "$fixture"' EXIT
fake_bin="$fixture/bin"
mkdir -p "$fake_bin"
for tool in bash dirname awk git basename find xargs; do
  ln -s "$(command -v "$tool")" "$fake_bin/$tool"
done
# Reuse isolated, version-matched fixture tools, never the host installation.
script_dir="$root/scripts/release"
# shellcheck source=scripts/release/tests/toolchain-fixture.sh
source "$script_dir/tests/toolchain-fixture.sh"
cp "$FIXTURE_RUST_TOOLS/cargo" "$fake_bin/cargo"
for tool in actionlint shellcheck cargo-audit; do
  runner="$root/scripts/ci/run-automation-quality.sh"
  [[ "$tool" != cargo-audit ]] || runner="$root/scripts/ci/run-dependency-security.sh"
  if PATH="$fake_bin" bash "$runner" "$tool" >"$fixture/error" 2>&1; then exit 1; fi
  grep -q "missing $tool" "$fixture/error"
  printf '#!/usr/bin/env bash\nexit 37\n' >"$fake_bin/$tool"
  chmod +x "$fake_bin/$tool"
  if [[ "$tool" == cargo-audit ]]; then
    cat >"$fake_bin/cargo" <<'SH'
#!/usr/bin/env bash
if [[ "$1" == --version ]]; then echo "cargo $FIXTURE_RUST_PIN"; else exec cargo-audit; fi
SH
  fi
  if PATH="$fake_bin" bash "$runner" "$tool" >"$fixture/error" 2>&1; then exit 1; fi
  if grep -q 'missing ' "$fixture/error"; then exit 1; fi
done
echo 'Prerequisite error fixtures passed.'
