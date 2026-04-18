#!/usr/bin/env bash
# Shared helpers for detecting the current desktop session.

current_desktop_tokens() {
  local raw="${XDG_CURRENT_DESKTOP:-${XDG_SESSION_DESKTOP:-${DESKTOP_SESSION:-}}}"
  local token

  raw="${raw//:/$'\n'}"
  raw="${raw//;/$'\n'}"

  while IFS= read -r token; do
    [[ -n "$token" ]] || continue
    printf '%s\n' "${token,,}"
  done <<< "$raw"
}

desktop_matches() {
  local wanted="${1,,}"
  local token

  while IFS= read -r token; do
    [[ "$token" == "$wanted" ]] && return 0
  done < <(current_desktop_tokens)

  return 1
}

is_hyprland_session() {
  [[ -n "${HYPRLAND_INSTANCE_SIGNATURE:-}" ]] || desktop_matches "hyprland"
}

is_niri_session() {
  [[ -n "${NIRI_SOCKET:-}" ]] || desktop_matches "niri"
}

is_bspwm_session() {
  [[ -n "${BSPWM_SOCKET:-}" ]] || desktop_matches "bspwm"
}

current_logind_session_id() {
  local session_id=""
  local login_user="${USER:-$(id -un)}"

  if [[ -n "${XDG_SESSION_ID:-}" ]]; then
    printf '%s\n' "$XDG_SESSION_ID"
    return 0
  fi

  if ! command -v loginctl >/dev/null 2>&1; then
    return 1
  fi

  session_id="$(loginctl show-user "$login_user" --property=Display --value 2>/dev/null || true)"
  if [[ -z "$session_id" || "$session_id" == "n/a" ]]; then
    return 1
  fi

  printf '%s\n' "$session_id"
}

lock_current_logind_session() {
  local session_id=""

  if ! command -v loginctl >/dev/null 2>&1; then
    return 1
  fi

  session_id="$(current_logind_session_id || true)"
  if [[ -n "$session_id" ]]; then
    loginctl lock-session "$session_id"
  else
    loginctl lock-session
  fi
}

terminate_current_logind_session() {
  local session_id=""

  if ! command -v loginctl >/dev/null 2>&1; then
    return 1
  fi

  session_id="$(current_logind_session_id || true)"
  [[ -n "$session_id" ]] || return 1

  loginctl terminate-session "$session_id"
}
