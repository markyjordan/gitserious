#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
publisher="$script_dir/publish-stable.sh"
fixture_dir="$(mktemp -d)"
trap 'rm -rf "$fixture_dir"' EXIT

fake_bin="$fixture_dir/bin"
artifact_dir="$fixture_dir/artifacts"
publish_log="$fixture_dir/publish.log"
indexed_file="$fixture_dir/indexed.txt"
mkdir -p "$fake_bin" "$artifact_dir"
: >"$publish_log"
: >"$indexed_file"

cat >"$fake_bin/cargo" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
case "${1:-}" in
  metadata)
    cat <<'JSON'
{"workspace_members":["app 0.1.0","cli 0.1.0"],"packages":[{"id":"app 0.1.0","name":"gitserious-app","version":"0.1.0","source":null,"publish":null},{"id":"cli 0.1.0","name":"gitserious","version":"0.1.0","source":null,"publish":null}],"resolve":{"nodes":[{"id":"app 0.1.0","dependencies":[],"deps":[]},{"id":"cli 0.1.0","dependencies":["app 0.1.0"],"deps":[{"pkg":"app 0.1.0"}]}]}}
JSON
    ;;
  info)
    package="${2%@*}"
    grep -Fx "$package" "${INDEXED_FILE:?INDEXED_FILE is required}" >/dev/null
    ;;
  publish)
    package=""
    dry_run=false
    for argument in "$@"; do
      [[ "$argument" == "--dry-run" ]] && dry_run=true
    done
    while (($#)); do
      if [[ "$1" == "-p" ]]; then
        package="$2"
        break
      fi
      shift
    done
    [[ -n "$package" ]] || exit 2
    if [[ "$dry_run" == true ]]; then
      printf 'dry-run %s\n' "$package" >>"${PUBLISH_LOG:?PUBLISH_LOG is required}"
    else
      printf 'publish %s\n' "$package" >>"${PUBLISH_LOG:?PUBLISH_LOG is required}"
      printf '%s\n' "$package" >>"${INDEXED_FILE:?INDEXED_FILE is required}"
    fi
    ;;
  *)
    exit 2
    ;;
esac
EOF

cat >"$fake_bin/gh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf 'gh %s\n' "$*" >>"${PUBLISH_LOG:?PUBLISH_LOG is required}"
if [[ "${1:-}" == "release" && "${2:-}" == "view" ]]; then
  exit 1
fi
EOF
chmod +x "$fake_bin/cargo" "$fake_bin/gh"

printf '%s\n' '## [0.1.0]' >"$artifact_dir/release-notes.md"
printf '%s\n' artifact >"$artifact_dir/v0.1.0-gitserious"
(
  cd "$artifact_dir"
  shasum -a 256 release-notes.md v0.1.0-gitserious >SHA256SUMS
)

env PATH="$fake_bin:$PATH" RELEASE_TAG=v0.1.0 RELEASE_MODE=dry-run \
  ARTIFACT_DIR="$artifact_dir" bash "$publisher" >/dev/null

if env PATH="$fake_bin:$PATH" RELEASE_TAG=v0.1.0-rc1 RELEASE_MODE=publish \
  ARTIFACT_DIR="$artifact_dir" CRATES_IO_TOKEN=fixture GH_TOKEN=fixture \
  bash "$publisher" >/dev/null 2>&1; then
  echo "Stable publisher accepted a release candidate tag." >&2
  exit 1
fi

env PATH="$fake_bin:$PATH" RELEASE_TAG=v0.1.0 RELEASE_MODE=publish \
  ARTIFACT_DIR="$artifact_dir" CRATES_IO_TOKEN=fixture GH_TOKEN=fixture \
  INDEXED_FILE="$indexed_file" PUBLISH_LOG="$publish_log" \
  bash "$publisher" >/dev/null

expected=$'dry-run gitserious-app\npublish gitserious-app\ndry-run gitserious\npublish gitserious'
actual="$(grep -E '^(dry-run|publish) ' "$publish_log")"
if [[ "$actual" != "$expected" ]]; then
  echo "Stable publisher did not honor dependency order." >&2
  printf 'Expected:\n%s\nActual:\n%s\n' "$expected" "$actual" >&2
  exit 1
fi
if ! grep -F 'gh release create v0.1.0' "$publish_log" >/dev/null; then
  echo "Stable publisher did not create the GitHub release after crates.io publication." >&2
  exit 1
fi

echo "Stable publication fixtures passed."
