#!/usr/bin/env zsh

autoload -Uz add-zsh-hook

typeset -g __vigil_activity_counter=0
typeset -g __vigil_current_activity_id=""
typeset -g __vigil_activity_rules_config_path=""
typeset -ga __vigil_activity_rules=()

__vigil_config_path() {
  if [[ -n "${XDG_CONFIG_HOME:-}" ]]; then
    print -r -- "${XDG_CONFIG_HOME}/vigil/config.toml"
  else
    print -r -- "${HOME}/.config/vigil/config.toml"
  fi
}

__vigil_load_activity_rules() {
  emulate -L zsh

  local config_path=$1
  local rules_file
  local label prefix
  local -a rules=()

  rules_file=$(mktemp) || return 1
  if ! command vigil --config "$config_path" activity shell-rules >"$rules_file" 2>/dev/null; then
    rm -f "$rules_file"
    return 1
  fi

  exec {rules_fd}<"$rules_file" || {
    rm -f "$rules_file"
    return 1
  }

  while IFS= read -r -d $'\0' -u $rules_fd label; do
    if ! IFS= read -r -d $'\0' -u $rules_fd prefix; then
      exec {rules_fd}<&-
      rm -f "$rules_file"
      return 1
    fi
    rules+=("$label" "$prefix")
  done

  exec {rules_fd}<&-
  rm -f "$rules_file"

  __vigil_activity_rules=("${rules[@]}")
  __vigil_activity_rules_config_path="$config_path"
}

__vigil_ensure_activity_rules() {
  emulate -L zsh

  local config_path
  config_path=$(__vigil_config_path)

  if [[ "$__vigil_activity_rules_config_path" != "$config_path" ]]; then
    __vigil_load_activity_rules "$config_path" || return 1
  fi
}

classify_activity() {
  emulate -L zsh
  setopt extendedglob

  local command=${1##[[:space:]]##}
  local label prefix
  local index=1

  __vigil_ensure_activity_rules || return 1

  while (( index <= ${#__vigil_activity_rules[@]} )); do
    label=${__vigil_activity_rules[index]}
    prefix=${__vigil_activity_rules[index + 1]}
    if [[ "$command" == "$prefix" || "$command" == ${prefix}\ * ]]; then
      print -r -- "$label"
      return 0
    fi
    (( index += 2 ))
  done

  return 1
}

__vigil_preexec() {
  emulate -L zsh

  local label
  label=$(classify_activity "$1") || return 0

  (( __vigil_activity_counter += 1 ))
  __vigil_current_activity_id="${$}-${__vigil_activity_counter}"

  (
    command vigil activity start \
      --id "$__vigil_current_activity_id" \
      --label "$label" \
      --cwd "$PWD" \
      </dev/null >/dev/null 2>&1 &
  )
}

__vigil_precmd() {
  local exit_code=$?
  emulate -L zsh
  local activity_id=$__vigil_current_activity_id

  [[ -n "$activity_id" ]] || return 0
  __vigil_current_activity_id=""

  (
    command vigil activity finish \
      --id "$activity_id" \
      --exit-code "$exit_code" \
      </dev/null >/dev/null 2>&1 &
  )
}

if [[ -o interactive ]] && command -v vigil >/dev/null 2>&1; then
  add-zsh-hook preexec __vigil_preexec
  add-zsh-hook precmd __vigil_precmd
fi
