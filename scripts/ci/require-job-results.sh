#!/usr/bin/env bash
set -euo pipefail

needs_json="${NEEDS_JSON:-}"
if [[ -z "$needs_json" ]]; then
  echo "NEEDS_JSON must contain the caller's needs context." >&2
  exit 2
fi

if ! jq -e 'type == "object" and length > 0' >/dev/null <<<"$needs_json"; then
  echo "NEEDS_JSON must be a non-empty JSON object." >&2
  exit 2
fi

for allowed_job in "$@"; do
  if ! jq -e --arg job "$allowed_job" 'has($job)' >/dev/null <<<"$needs_json"; then
    echo "Allowed skipped job is absent from needs: $allowed_job" >&2
    exit 2
  fi
done

skip_is_allowed() {
  local candidate="$1"
  local allowed_job
  shift

  for allowed_job in "$@"; do
    if [[ "$candidate" == "$allowed_job" ]]; then
      return 0
    fi
  done

  return 1
}

while IFS=$'\t' read -r job result; do
  if [[ "$result" == "success" ]]; then
    continue
  fi

  if [[ "$result" == "skipped" ]] && skip_is_allowed "$job" "$@"; then
    continue
  fi

  echo "$job did not pass: ${result:-missing result}" >&2
  exit 1
done < <(jq -r 'to_entries[] | [.key, (.value.result // "")] | @tsv' <<<"$needs_json")

echo "All required job results passed."
