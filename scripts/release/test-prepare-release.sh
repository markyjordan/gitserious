#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
validator="$script_dir/prepare-release.sh"
fixture_dir="$(mktemp -d)"
trap 'rm -rf "$fixture_dir"' EXIT

fake_bin="$fixture_dir/bin"
api_log="$fixture_dir/api.log"
mkdir -p "$fake_bin"

cat >"$fake_bin/git" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
case "${1:-}" in
  fetch)
    exit 0
    ;;
  rev-parse)
    printf '%s\n' cccccccccccccccccccccccccccccccccccccccc
    exit 0
    ;;
esac
exit 2
EOF

cat >"$fake_bin/gh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"${API_LOG:?API_LOG is required}"
if [[ "$*" == *"git/ref/heads/release/"* ]]; then
  exit "${BRANCH_LOOKUP_STATUS:-1}"
fi
exit 0
EOF
chmod +x "$fake_bin/git" "$fake_bin/gh"

run_fixture() {
  env -i PATH="$fake_bin:/usr/bin:/bin" HOME="${HOME:-/tmp}" \
    API_LOG="$api_log" GH_TOKEN=fixture-token \
    GITHUB_REPOSITORY=markyjordan/gitserious GITHUB_ACTOR=maintainer \
    VERSION_FAMILY="${VERSION_FAMILY:-0.1}" BASE_REF="${BASE_REF:-main}" \
    BRANCH_LOOKUP_STATUS="${BRANCH_LOOKUP_STATUS:-1}" \
    bash "$validator"
}

if VERSION_FAMILY=0.1 BASE_REF=dev run_fixture >/dev/null 2>&1; then
  echo "Release preparation accepted a non-main base." >&2
  exit 1
fi

if VERSION_FAMILY=v0.1 BASE_REF=main run_fixture >/dev/null 2>&1; then
  echo "Release preparation accepted an invalid version family." >&2
  exit 1
fi

: >"$api_log"
VERSION_FAMILY=0.1 BASE_REF=main run_fixture >/dev/null
if ! grep -F -- '--method POST repos/markyjordan/gitserious/git/refs -f ref=refs/heads/release/0.1 -f sha=cccccccccccccccccccccccccccccccccccccccc' "$api_log" >/dev/null; then
  echo "Release preparation did not request the expected immutable ref." >&2
  exit 1
fi

if VERSION_FAMILY=0.1 BASE_REF=main BRANCH_LOOKUP_STATUS=0 run_fixture >/dev/null 2>&1; then
  echo "Release preparation accepted an existing release branch." >&2
  exit 1
fi

echo "Release preparation fixtures passed."
