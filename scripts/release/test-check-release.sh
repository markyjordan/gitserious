#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
validator="$script_dir/check-release.sh"
validation_only="$script_dir/validate-release-readiness.sh"
fixture_dir="$(mktemp -d)"
trap 'rm -rf "$fixture_dir"' EXIT

repo="$fixture_dir/repo"
origin="$fixture_dir/origin.git"
fake_bin="$fixture_dir/bin"
quality_log="$fixture_dir/quality.log"
mkdir -p "$repo" "$fake_bin"

for file in Cargo.lock README.md LICENSE-MIT LICENSE-APACHE-2.0; do
  : >"$repo/$file"
done
cat >"$repo/Cargo.toml" <<'EOF'
[workspace]
members = []
EOF
cat >"$repo/CHANGELOG.md" <<'EOF'
# Changelog

## [0.1.0] - 2026-08-01
EOF

cat >"$fake_bin/cargo" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == "metadata" ]]; then
  printf '%s\n' '{"packages":[{"source":null,"version":"0.1.0"}]}'
  exit 0
fi
exit 2
EOF
cat >"$fake_bin/quality-runner" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "${1:?component is required}" >>"${QUALITY_LOG:?QUALITY_LOG is required}"
EOF
chmod +x "$fake_bin/cargo" "$fake_bin/quality-runner"

git -C "$repo" init -q -b release/0.1
git -C "$repo" config user.name "Release Fixture"
git -C "$repo" config user.email "release-fixture@example.invalid"
git -C "$repo" add .
git -C "$repo" commit -qm "release head"
git -C "$fixture_dir" init -q --bare "$origin"
git -C "$repo" remote add origin "$origin"
git -C "$repo" push -q origin release/0.1
git -C "$repo" tag v0.1.0
git -C "$repo" push -q origin v0.1.0

(
  cd "$repo"
  env PATH="$fake_bin:$PATH" QUALITY_RUNNER="$fake_bin/quality-runner" \
    QUALITY_LOG="$quality_log" RELEASE_TAG=v0.1.0 bash "$validator" >/dev/null
)

expected_components=$'check\nfmt\nlint\ntest\nrelease'
if [[ "$(cat "$quality_log")" != "$expected_components" ]]; then
  echo "Release readiness did not run the complete quality surface." >&2
  exit 1
fi

sed -i.bak 's/2026-08-01/TBD/' "$repo/CHANGELOG.md"
if (
  cd "$repo"
  env PATH="$fake_bin:$PATH" QUALITY_RUNNER="$fake_bin/quality-runner" \
    QUALITY_LOG="$quality_log" RELEASE_TAG=v0.1.0 bash "$validator" >/dev/null 2>&1
); then
  echo "Stable readiness accepted an unfinalized changelog date." >&2
  exit 1
fi
mv "$repo/CHANGELOG.md.bak" "$repo/CHANGELOG.md"

: >"$quality_log"
(
  cd "$repo"
  env PATH="$fake_bin:$PATH" QUALITY_RUNNER="$fake_bin/quality-runner" \
    QUALITY_LOG="$quality_log" RELEASE_REF=main GITHUB_REF_NAME=17/merge \
    bash "$validation_only" >/dev/null
)
if [[ -s "$quality_log" ]]; then
  echo "Validation-only release readiness reran the quality surface." >&2
  exit 1
fi

(
  cd "$repo"
  env PATH="$fake_bin:$PATH" QUALITY_RUNNER="$fake_bin/quality-runner" \
    QUALITY_LOG="$quality_log" RELEASE_REF=main GITHUB_REF_NAME=17/merge \
    bash "$validator" >/dev/null
)
if [[ "$(cat "$quality_log")" != "$expected_components" ]]; then
  echo "Release readiness did not accept a pull request merge ref for main." >&2
  exit 1
fi

: >"$quality_log"
(
  cd "$repo"
  env PATH="$fake_bin:$PATH" RELEASE_REF=release/0.1 \
    bash "$validation_only" >/dev/null
)
if [[ -s "$quality_log" ]]; then
  echo "Validation-only release readiness invoked a quality runner." >&2
  exit 1
fi

git -C "$repo" commit --allow-empty -qm "new release head"
git -C "$repo" push -q origin release/0.1
if (
  cd "$repo"
  env PATH="$fake_bin:$PATH" QUALITY_RUNNER="$fake_bin/quality-runner" \
    QUALITY_LOG="$quality_log" RELEASE_TAG=v0.1.0 bash "$validator" >/dev/null 2>&1
); then
  echo "Release readiness accepted a tag behind its release branch." >&2
  exit 1
fi

if (
  cd "$repo"
  env PATH="$fake_bin:$PATH" QUALITY_RUNNER="$fake_bin/quality-runner" \
    QUALITY_LOG="$quality_log" RELEASE_TAG=v0.1.0-rc0 bash "$validator" >/dev/null 2>&1
); then
  echo "Release readiness accepted an invalid RC number." >&2
  exit 1
fi

echo "Release readiness fixtures passed."
