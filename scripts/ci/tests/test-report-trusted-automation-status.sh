#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
reporter="$script_dir/report-trusted-automation-status.sh"
fixture_dir="$(mktemp -d)"
trap 'rm -rf "$fixture_dir"' EXIT

fake_bin="$fixture_dir/bin"
api_log="$fixture_dir/api.log"
mkdir -p "$fake_bin"

cat >"$fake_bin/gh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"${API_LOG:?API_LOG is required}"
EOF
chmod +x "$fake_bin/gh"

head_sha="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
repository="markyjordan/gitserious"
target_url="https://github.com/markyjordan/gitserious/actions/runs/17"

run_fixture() {
  env -i PATH="$fake_bin:/usr/bin:/bin" HOME="${HOME:-/tmp}" \
    API_LOG="$api_log" GH_TOKEN=fixture-token GITHUB_REPOSITORY="$repository" \
    PR_HEAD_SHA="${PR_HEAD_SHA:-$head_sha}" TRUST_STATUS_TARGET_URL="$target_url" \
    TRUST_STATUS="${TRUST_STATUS:-pending}" bash "$reporter"
}

for state in pending success failure error; do
  : >"$api_log"
  TRUST_STATUS="$state" run_fixture >/dev/null
  expected="--method POST repos/${repository}/statuses/${head_sha} -f state=${state} -f context=trusted-automation-review"
  if ! grep -F -- "$expected" "$api_log" >/dev/null; then
    echo "Status reporter did not publish ${state} to the exact PR head." >&2
    exit 1
  fi
  if ! grep -F -- "-f target_url=${target_url}" "$api_log" >/dev/null; then
    echo "Status reporter omitted the workflow run URL." >&2
    exit 1
  fi
done

if TRUST_STATUS=unknown run_fixture >/dev/null 2>&1; then
  echo "Status reporter accepted an unsupported state." >&2
  exit 1
fi

if PR_HEAD_SHA=short TRUST_STATUS=pending run_fixture >/dev/null 2>&1; then
  echo "Status reporter accepted a non-full PR head SHA." >&2
  exit 1
fi

if env -i PATH="$fake_bin:/usr/bin:/bin" HOME="${HOME:-/tmp}" \
  GITHUB_REPOSITORY="$repository" PR_HEAD_SHA="$head_sha" TRUST_STATUS=pending \
  bash "$reporter" >/dev/null 2>&1; then
  echo "Status reporter accepted a missing token." >&2
  exit 1
fi

echo "Trusted automation status fixtures passed."
