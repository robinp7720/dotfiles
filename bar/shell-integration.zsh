#!/usr/bin/env zsh

autoload -Uz add-zsh-hook

typeset -g __cockpit_bar_activity_counter=0
typeset -g __cockpit_bar_current_activity_id=""

classify_activity() {
  emulate -L zsh
  setopt extendedglob

  local command=${1##[[:space:]]##}
  local label prefix
  local -a rules=(
    "Cargo build:cargo build"
    "Cargo test:cargo test"
    "Cargo run:cargo run"
    "npm test:npm test"
    "pnpm test:pnpm test"
    "Pytest:pytest"
    "Make:make"
  )

  for rule in "${rules[@]}"; do
    label=${rule%%:*}
    prefix=${rule#*:}
    if [[ "$command" == "$prefix" || "$command" == ${prefix}\ * ]]; then
      print -r -- "$label"
      return 0
    fi
  done

  return 1
}

__cockpit_bar_preexec() {
  emulate -L zsh

  local label
  label=$(classify_activity "$1") || return 0

  (( __cockpit_bar_activity_counter += 1 ))
  __cockpit_bar_current_activity_id="${$}-${__cockpit_bar_activity_counter}"

  (
    command cockpit-bar activity start \
      --id "$__cockpit_bar_current_activity_id" \
      --label "$label" \
      --cwd "$PWD" \
      </dev/null >/dev/null 2>&1 &
  )
}

__cockpit_bar_precmd() {
  local exit_code=$?
  emulate -L zsh
  local activity_id=$__cockpit_bar_current_activity_id

  [[ -n "$activity_id" ]] || return 0
  __cockpit_bar_current_activity_id=""

  (
    command cockpit-bar activity finish \
      --id "$activity_id" \
      --exit-code "$exit_code" \
      </dev/null >/dev/null 2>&1 &
  )
}

if [[ -o interactive ]] && command -v cockpit-bar >/dev/null 2>&1; then
  add-zsh-hook preexec __cockpit_bar_preexec
  add-zsh-hook precmd __cockpit_bar_precmd
fi
