#!/usr/bin/env bash
set -euo pipefail

maintainer_registry_repo_root() {
  if [[ -n "${MAINTAINER_REGISTRY_ROOT:-}" ]]; then
    printf '%s\n' "$MAINTAINER_REGISTRY_ROOT"
  else
    git rev-parse --show-toplevel
  fi
}

maintainer_registry_file() {
  local root
  root="$(maintainer_registry_repo_root)"
  printf '%s\n' "${MAINTAINER_REGISTRY_FILE:-$root/.github/maintainers/registry.tsv}"
}

maintainer_registry_rows() {
  local registry
  registry="$(maintainer_registry_file)"
  awk 'NF && $0 !~ /^[[:space:]]*#/' "$registry"
}

validate_maintainer_registry() {
  local registry root keys_prefix entries row field_count login fingerprint key_path
  local key_file primary_fingerprint existing_login existing_fingerprint

  registry="$(maintainer_registry_file)"
  root="$(maintainer_registry_repo_root)"
  keys_prefix="${MAINTAINER_KEYS_PREFIX:-.github/maintainers/keys/}"
  entries=""

  [[ -f "$registry" ]] || {
    echo "Maintainer registry is missing: $registry" >&2
    return 1
  }
  command -v gpg >/dev/null || {
    echo "gpg is required to validate maintainer keys." >&2
    return 1
  }

  while IFS= read -r row; do
    field_count="$(awk -F '\t' '{print NF}' <<<"$row")"
    if [[ "$field_count" != 3 ]]; then
      echo "Maintainer registry rows must contain exactly three tab-separated fields." >&2
      return 1
    fi

    IFS=$'\t' read -r login fingerprint key_path <<<"$row"
    if [[ ! "$login" =~ ^[a-z0-9]([a-z0-9-]{0,37}[a-z0-9])?$ ]]; then
      echo "Invalid lowercase GitHub login in maintainer registry: $login" >&2
      return 1
    fi
    if [[ ! "$fingerprint" =~ ^[0-9A-F]{40}$ ]]; then
      echo "Invalid OpenPGP fingerprint for $login: $fingerprint" >&2
      return 1
    fi
    if [[ "$key_path" == /* || "$key_path" == *..* || "$key_path" != "$keys_prefix"* ]]; then
      echo "Maintainer key path must stay under ${keys_prefix}: $key_path" >&2
      return 1
    fi

    key_file="$root/$key_path"
    [[ -f "$key_file" ]] || {
      echo "Maintainer public key is missing: $key_path" >&2
      return 1
    }
    primary_fingerprint="$(
      gpg --batch --with-colons --show-keys "$key_file" 2>/dev/null |
        awk -F: '$1 == "fpr" {print $10; exit}'
    )"
    if [[ "$primary_fingerprint" != "$fingerprint" ]]; then
      echo "Maintainer key fingerprint mismatch for $login: expected $fingerprint, got ${primary_fingerprint:-<none>}" >&2
      return 1
    fi

    while IFS=$'\t' read -r existing_login existing_fingerprint; do
      [[ -n "$existing_login" ]] || continue
      if [[ "$existing_login" == "$login" && "$existing_fingerprint" == "$fingerprint" ]]; then
        echo "Duplicate maintainer registry entry for $login and $fingerprint." >&2
        return 1
      fi
      if [[ "$existing_login" != "$login" && "$existing_fingerprint" == "$fingerprint" ]]; then
        echo "OpenPGP fingerprint $fingerprint is assigned to multiple maintainers." >&2
        return 1
      fi
    done <<<"$entries"
    entries="${entries}${login}"$'\t'"${fingerprint}"$'\n'
  done < <(maintainer_registry_rows)

  [[ -n "$entries" ]] || {
    echo "Maintainer registry must contain at least one key." >&2
    return 1
  }
}

registered_maintainer_logins() {
  maintainer_registry_rows | awk -F '\t' '{print $1}' | sort -fu
}

maintainer_fingerprints_for_login() {
  local login="$1"
  maintainer_registry_rows | awk -F '\t' -v login="$login" '$1 == login {print $2}'
}

maintainer_key_paths_for_login() {
  local login="$1"
  maintainer_registry_rows | awk -F '\t' -v login="$login" '$1 == login {print $3}'
}

maintainer_login_for_fingerprint() {
  local fingerprint="$1"
  maintainer_registry_rows | awk -F '\t' -v fingerprint="$fingerprint" \
    '$2 == fingerprint {print $1; exit}'
}

maintainer_permission_rows() {
  local token
  if [[ -n "${MAINTAINER_PERMISSION_ROWS_DATA:-}" ]]; then
    printf '%s\n' "$MAINTAINER_PERMISSION_ROWS_DATA"
    return
  fi
  if [[ -n "${MAINTAINER_PERMISSION_ROWS_FILE:-}" ]]; then
    cat "$MAINTAINER_PERMISSION_ROWS_FILE"
    return
  fi

  token="${GH_TOKEN:-${GITHUB_TOKEN:-}}"
  [[ -n "$token" ]] || {
    echo "GH_TOKEN or GITHUB_TOKEN is required to validate maintainer access." >&2
    return 1
  }
  command -v gh >/dev/null || {
    echo "gh is required to validate maintainer access." >&2
    return 1
  }

  GH_TOKEN="$token" gh api --paginate \
    "repos/${GITHUB_REPOSITORY:?GITHUB_REPOSITORY is required}/collaborators?affiliation=all&per_page=100" \
    --jq '.[] | [
      .login,
      (if .permissions.admin then "admin" elif .permissions.push then "write" else "none" end),
      .role_name
    ] | @tsv'
}

maintainer_is_active() {
  local login="$1" permission_rows permission
  permission_rows="$(maintainer_permission_rows)" || return 1
  permission="$(
    awk -F '\t' -v login="$login" \
      'tolower($1) == tolower(login) {print $2; exit}' <<<"$permission_rows"
  )"
  case "$permission" in
    admin | write | push) return 0 ;;
    *) return 1 ;;
  esac
}

active_maintainer_logins() {
  local login permission_rows
  permission_rows="$(maintainer_permission_rows)" || return 1
  MAINTAINER_PERMISSION_ROWS_DATA="$permission_rows"
  while IFS= read -r login; do
    [[ -n "$login" ]] || continue
    if maintainer_is_active "$login"; then
      printf '%s\n' "$login"
    fi
  done < <(registered_maintainer_logins)
}
