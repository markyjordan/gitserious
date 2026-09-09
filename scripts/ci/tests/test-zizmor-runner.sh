#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
fixture="$(mktemp -d)"
trap 'rm -rf "$fixture"' EXIT
mkdir -p "$fixture/bin"
cat >"$fixture/bin/zizmor" <<'SH'
#!/usr/bin/env bash
if [[ "$*" == --version ]]; then echo "zizmor ${FAKE_VERSION:-1.30.0}"; exit 0; fi
printf '%s\n' "$*" >>"$LOG"
exit "${FAIL_AUDIT:-0}"
SH
chmod +x "$fixture/bin/zizmor"
export PATH="$fixture/bin:$PATH" LOG="$fixture/log"
unset GH_TOKEN GITHUB_TOKEN
runner="$root/scripts/ci/run-zizmor.sh"
if bash "$runner" >"$fixture/error" 2>&1; then exit 1; fi
grep -q 'requires GH_TOKEN' "$fixture/error"
[[ ! -e "$LOG" ]]
if FAKE_VERSION=1.29.0 bash "$runner" --offline >"$fixture/error" 2>&1; then exit 1; fi
grep -q 'expected zizmor 1.30.0' "$fixture/error"
[[ ! -e "$LOG" ]]
bash "$runner" --offline >"$fixture/error" 2>&1
grep -q 'OFFLINE / PARTIAL' "$fixture/error"
grep -Fx -- '--pedantic --min-confidence medium --min-severity medium --collect all --offline --config .github/zizmor.yml .' "$LOG"
: >"$LOG"
GH_TOKEN=fixture bash "$runner" >"$fixture/error" 2>&1
grep -q 'authenticated ONLINE' "$fixture/error"
grep -Fx -- '--pedantic --min-confidence medium --min-severity medium --collect all --config .github/zizmor.yml .' "$LOG"
if GH_TOKEN=fixture FAIL_AUDIT=12 bash "$runner" >/dev/null 2>&1; then exit 1; fi
grep -F 'version: "1.30.0"' "$root/.github/workflows/automation-quality.yml" >/dev/null
echo 'Zizmor runner fixtures passed.'
