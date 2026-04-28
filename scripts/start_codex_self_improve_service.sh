#!/usr/bin/env bash

set -euo pipefail

if ! command -v systemctl >/dev/null 2>&1; then
  exit 0
fi

unit_source="$HOME/.dotfiles/systemd/user/codex-self-improve.service"
unit_target="$HOME/.config/systemd/user/codex-self-improve.service"

if [[ -f "$unit_source" ]]; then
  mkdir -p "$HOME/.config/systemd/user"

  if [[ ! -L "$unit_target" || "$(readlink "$unit_target" 2>/dev/null || true)" != "$unit_source" ]]; then
    ln -sfn "$unit_source" "$unit_target"
    systemctl --user daemon-reload >/dev/null 2>&1 || true
  fi
fi

vars=(
  DBUS_SESSION_BUS_ADDRESS
  DESKTOP_SESSION
  DISPLAY
  HYPRLAND_INSTANCE_SIGNATURE
  NIRI_SOCKET
  BSPWM_SOCKET
  WAYLAND_DISPLAY
  XAUTHORITY
  XDG_CURRENT_DESKTOP
  XDG_RUNTIME_DIR
  XDG_SESSION_DESKTOP
  XDG_SESSION_ID
)

imported=()

for name in "${vars[@]}"; do
  if [[ -n "${!name:-}" ]]; then
    imported+=("$name")
  fi
done

if ((${#imported[@]} > 0)); then
  systemctl --user import-environment "${imported[@]}" >/dev/null 2>&1 || true
fi

systemctl --user start --no-block codex-self-improve.service >/dev/null 2>&1 || true
