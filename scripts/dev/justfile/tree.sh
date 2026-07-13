#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
readonly REPO_ROOT

readonly COLLAPSED_DOT_DIR_GLOB=".git|.gradle|.idea"
readonly GENERATED_DIR_GLOB="node_modules|target|build|dist|coverage|out|debug"
readonly DEFAULT_IGNORE_GLOB="${COLLAPSED_DOT_DIR_GLOB}|${GENERATED_DIR_GLOB}"
readonly DEFAULT_SHOW_HIDDEN_IGNORE_GLOB="${GENERATED_DIR_GLOB}"
readonly COLLAPSED_DOT_DIR_AWK_REGEX='^(\.git|\.gradle|\.idea)$'

SHOW_HIDDEN=false
DISABLE_IGNORES=false
REQUESTED_PATH="."
RESOLVED_PATH="."
IGNORE_GLOB=""
COLLAPSE_GLOB=""
HAS_CUSTOM_IGNORE_GLOB=false

print_usage() {
  cat <<'USAGE'
usage:
  `tree.sh [-a] [--no-ignore] [path]`
  `tree.sh -h | --help`

options:
  -a            Show hidden files and directories (dotfiles)
  --no-ignore   Disable all ignore globs for this run

examples:
  `just tree`
  `just tree -a`
  `just tree --no-ignore`
  `just tree docs/eng`
  `just tree -a docs/eng`
  `TREE_NO_IGNORE=1 just tree apps`
USAGE
}

parse_cli_args() {
  local arg

  for arg in "$@"; do
    case "${arg}" in
      -h|--help)
        print_usage
        exit 0
        ;;
      -a|--all)
        SHOW_HIDDEN=true
        ;;
      --no-ignore)
        DISABLE_IGNORES=true
        ;;
      -*)
        echo "error: unknown flag \`${arg}\`" >&2
        print_usage >&2
        exit 1
        ;;
      *)
        set_requested_path_once "${arg}"
        ;;
    esac
  done

  REQUESTED_PATH="$(strip_wrapping_quotes "${REQUESTED_PATH}")"
}

set_requested_path_once() {
  local next_path="$1"

  if [[ "${REQUESTED_PATH}" != "." ]]; then
    echo "error: expected one path argument, got \`${REQUESTED_PATH}\` and \`${next_path}\`" >&2
    echo "help: use a single slash-delimited path, for example: \`just tree docs/product\`" >&2
    exit 1
  fi

  REQUESTED_PATH="${next_path}"
}

strip_wrapping_quotes() {
  local value="$1"

  value="${value#\"}"
  value="${value%\"}"
  value="${value#\'}"
  value="${value%\'}"

  printf '%s\n' "${value}"
}

apply_environment_overrides() {
  if [[ "${TREE_NO_IGNORE:-0}" == "1" || "${TREE_NO_IGNORE:-}" == "true" ]]; then
    DISABLE_IGNORES=true
  fi
}

configure_ignore_rules() {
  if [[ -n "${TREE_IGNORE_GLOB:-}" ]]; then
    IGNORE_GLOB="${TREE_IGNORE_GLOB}"
    HAS_CUSTOM_IGNORE_GLOB=true
  elif "${SHOW_HIDDEN}"; then
    IGNORE_GLOB="${DEFAULT_SHOW_HIDDEN_IGNORE_GLOB}"
    COLLAPSE_GLOB="${COLLAPSED_DOT_DIR_GLOB}"
  else
    IGNORE_GLOB="${DEFAULT_IGNORE_GLOB}"
  fi

  if "${DISABLE_IGNORES}"; then
    IGNORE_GLOB=""
    COLLAPSE_GLOB=""
  fi
}

resolve_requested_path() {
  if [[ "${REQUESTED_PATH}" == "." ]]; then
    RESOLVED_PATH="."
    return
  fi

  RESOLVED_PATH="$(resolve_repo_path "${REQUESTED_PATH}")" || {
    echo "error: no such path \`${REQUESTED_PATH}\`" >&2
    exit 1
  }
}

resolve_repo_path() {
  local query="$1"
  local matches=()
  local candidate

  if [[ -d "${query}" || -f "${query}" ]]; then
    printf '%s\n' "${query}"
    return 0
  fi

  while IFS= read -r candidate; do
    matches+=("${candidate}")
  done < <(find_matching_directories "${query}")

  if [[ "${#matches[@]}" -eq 0 ]]; then
    return 1
  fi

  if [[ "${#matches[@]}" -gt 1 ]]; then
    echo "error: ambiguous path \`${query}\`" >&2
    echo "help: choose one of these matches:" >&2
    printf ' - %s\n' "${matches[@]}" >&2
    exit 1
  fi

  printf '%s\n' "${matches[0]}"
}

find_matching_directories() {
  local query="$1"

  if [[ "${query}" != */* ]]; then
    find . -type d -name "${query}" | sort
  else
    find . -type d -path "*${query}" | sort
  fi
}

path_matches_pipe_glob() {
  local entry="$1"
  local pipe_glob="$2"
  local relative_entry="${entry#./}"
  local entry_basename="${relative_entry##*/}"
  local pattern

  [[ -z "${pipe_glob}" ]] && return 1

  IFS='|' read -r -a patterns <<< "${pipe_glob}"
  for pattern in "${patterns[@]}"; do
    [[ -z "${pattern}" ]] && continue
    # These variables intentionally carry user-supplied glob patterns.
    # shellcheck disable=SC2053
    if [[ "${entry}" == ${pattern} ||
      "${relative_entry}" == ${pattern} ||
      "${relative_entry}" == ${pattern}/* ||
      "${relative_entry}" == */${pattern} ||
      "${relative_entry}" == */${pattern}/* ||
      "${entry_basename}" == ${pattern} ]]; then
      return 0
    fi
  done

  return 1
}

path_is_descendant_of_pipe_glob() {
  local entry="$1"
  local pipe_glob="$2"
  local relative_entry="${entry#./}"
  local pattern

  [[ -z "${pipe_glob}" ]] && return 1

  IFS='|' read -r -a patterns <<< "${pipe_glob}"
  for pattern in "${patterns[@]}"; do
    [[ -z "${pattern}" ]] && continue
    if [[ "${relative_entry}" == ${pattern}/* || "${relative_entry}" == */${pattern}/* ]]; then
      return 0
    fi
  done

  return 1
}

should_use_eza_collapsed_dot_dir_mode() {
  "${SHOW_HIDDEN}" &&
    ! "${DISABLE_IGNORES}" &&
    ! "${HAS_CUSTOM_IGNORE_GLOB}" &&
    command -v eza >/dev/null 2>&1
}

render_tree() {
  if should_use_eza_collapsed_dot_dir_mode; then
    render_eza_tree_with_collapsed_dot_dirs
  elif command -v eza >/dev/null 2>&1; then
    render_with_eza
  elif command -v tree >/dev/null 2>&1; then
    render_with_tree
  else
    render_with_find
  fi
}

render_eza_tree_with_collapsed_dot_dirs() {
  eza --tree --all --color=always --ignore-glob "${IGNORE_GLOB}" "${RESOLVED_PATH}" |
    collapse_eza_tree_stream "${COLLAPSED_DOT_DIR_AWK_REGEX}"
}

collapse_eza_tree_stream() {
  local collapse_name_regex="$1"

  awk -v collapse_name_regex="${collapse_name_regex}" '
    BEGIN {
      skip_descendants_below_depth = -1
    }

    function strip_ansi(value) {
      gsub(/\033\[[0-9;]*m/, "", value)
      return value
    }

    function tree_line_depth(value, plain_value, prefix) {
      plain_value = strip_ansi(value)
      if (plain_value !~ /^((│   |    )*)(├── |└── )/) {
        return -1
      }
      prefix = plain_value
      sub(/(├── |└── ).*$/, "", prefix)
      return length(prefix) / 4
    }

    function tree_line_name(value, plain_value) {
      plain_value = strip_ansi(value)
      sub(/^((│   |    )*)(├── |└── )/, "", plain_value)
      sub(/\/$/, "", plain_value)
      return plain_value
    }

    {
      current_depth = tree_line_depth($0)

      if (skip_descendants_below_depth >= 0 && current_depth > skip_descendants_below_depth) {
        next
      }
      if (skip_descendants_below_depth >= 0 && current_depth <= skip_descendants_below_depth) {
        skip_descendants_below_depth = -1
      }

      print

      if (current_depth >= 0 && tree_line_name($0) ~ collapse_name_regex) {
        skip_descendants_below_depth = current_depth
      }
    }
  '
}

render_with_eza() {
  local cmd=(eza --tree)

  if "${SHOW_HIDDEN}"; then
    cmd+=(--all)
  fi
  if [[ -n "${IGNORE_GLOB}" ]]; then
    cmd+=(--ignore-glob "${IGNORE_GLOB}")
  fi

  "${cmd[@]}" "${RESOLVED_PATH}"
}

render_with_tree() {
  local cmd=(tree)

  if "${SHOW_HIDDEN}"; then
    cmd+=(-a)
  fi
  if [[ -n "${IGNORE_GLOB}" ]]; then
    cmd+=(-I "${IGNORE_GLOB}")
  fi

  "${cmd[@]}" "${RESOLVED_PATH}"
}

render_with_find() {
  local entry

  while IFS= read -r entry; do
    if should_skip_find_entry "${entry}"; then
      continue
    fi
    printf '%s\n' "${entry}"
  done < <(find "${RESOLVED_PATH}" -print)
}

should_skip_find_entry() {
  local entry="$1"

  if ! "${SHOW_HIDDEN}" && [[ "${entry}" == */.* || "${entry}" == .* ]]; then
    return 0
  fi

  if path_is_descendant_of_pipe_glob "${entry}" "${COLLAPSE_GLOB}"; then
    return 0
  fi

  path_matches_pipe_glob "${entry}" "${IGNORE_GLOB}"
}

main() {
  cd "${REPO_ROOT}"

  parse_cli_args "$@"
  apply_environment_overrides
  configure_ignore_rules
  resolve_requested_path
  render_tree
}

main "$@"
