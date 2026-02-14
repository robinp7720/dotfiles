#!/usr/bin/env bash
# Keep awww wallpapers applied on all outputs (reapply on output changes).

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
CONFIG_FILE_DEFAULT="$SCRIPT_DIR/awww_wallpapers.conf"
CONFIG_FILE="${AWWW_WALLPAPER_CONFIG:-$CONFIG_FILE_DEFAULT}"
DEFAULT_WALLPAPER_FALLBACK="${AWWW_DEFAULT_WALLPAPER:-$HOME/.wallpaper.png}"
THEME_WALLPAPER_OVERRIDE="${AWWW_THEME_WALLPAPER:-}"
RUN_MATUGEN=true
WATCH=true
WAYLAND_DISPLAY_NAME="${WAYLAND_DISPLAY:-wayland-0}"
LOCK_FILE="${XDG_RUNTIME_DIR:-/tmp}/awww-wallpaper-${WAYLAND_DISPLAY_NAME}.lock"

usage() {
  cat <<'USAGE'
Usage: awww_wallpaper_watcher.sh [--once] [--watch] [--config path]
                                 [--wallpaper path] [--theme path] [--no-theme]

Defaults:
  --watch                Run continuously and reapply on output changes.
  --config               Uses ~/.dotfiles/scripts/awww_wallpapers.conf if present.
  --wallpaper            Falls back to ~/.wallpaper.png when no default is set.
USAGE
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --once)
      WATCH=false
      ;;
    --watch)
      WATCH=true
      ;;
    --config)
      CONFIG_FILE="$2"
      shift
      ;;
    --wallpaper)
      DEFAULT_WALLPAPER_FALLBACK="$2"
      shift
      ;;
    --theme)
      THEME_WALLPAPER_OVERRIDE="$2"
      shift
      ;;
    --no-theme)
      RUN_MATUGEN=false
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      DEFAULT_WALLPAPER_FALLBACK="$1"
      ;;
  esac
  shift
done

if ! command -v awww >/dev/null 2>&1; then
  exit 0
fi

# Ensure only one instance is running.
exec 9>"$LOCK_FILE"
if command -v flock >/dev/null 2>&1; then
  if ! flock -n 9; then
    exit 0
  fi
fi

expand_path() {
  case "$1" in
    "~"|"~/"*)
      printf '%s' "$HOME${1:1}"
      ;;
    *)
      printf '%s' "$1"
      ;;
  esac
}

declare -A OUTPUT_WALLPAPERS=()
DEFAULT_WALLPAPER=""
THEME_WALLPAPER=""

load_config() {
  if [ ! -f "$CONFIG_FILE" ]; then
    return 0
  fi

  while IFS= read -r line || [ -n "$line" ]; do
    line="${line%%#*}"
    line="${line%"${line##*[![:space:]]}"}"
    line="${line#"${line%%[![:space:]]*}"}"
    if [ -z "$line" ]; then
      continue
    fi

    if [[ "$line" == default=* ]]; then
      DEFAULT_WALLPAPER="$(expand_path "${line#default=}")"
      continue
    fi

    if [[ "$line" == theme=* ]]; then
      THEME_WALLPAPER="$(expand_path "${line#theme=}")"
      continue
    fi

    if [[ "$line" == *=* ]]; then
      local key
      local value
      key="${line%%=*}"
      value="$(expand_path "${line#*=}")"
      if [ -n "$key" ] && [ -n "$value" ]; then
        OUTPUT_WALLPAPERS["$key"]="$value"
      fi
    fi
  done < "$CONFIG_FILE"
}

resolve_wallpapers() {
  if [ -z "$DEFAULT_WALLPAPER" ]; then
    DEFAULT_WALLPAPER="$(expand_path "$DEFAULT_WALLPAPER_FALLBACK")"
  fi

  if [ ! -f "$DEFAULT_WALLPAPER" ]; then
    DEFAULT_WALLPAPER=""
  fi

  if [ -n "$THEME_WALLPAPER_OVERRIDE" ]; then
    THEME_WALLPAPER="$(expand_path "$THEME_WALLPAPER_OVERRIDE")"
  fi

  if [ -z "$THEME_WALLPAPER" ] && [ -n "$DEFAULT_WALLPAPER" ]; then
    THEME_WALLPAPER="$DEFAULT_WALLPAPER"
  fi
}

ensure_daemon() {
  if awww query >/dev/null 2>&1; then
    return 0
  fi

  local runtime_dir
  local wayland_display
  runtime_dir="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
  wayland_display="${WAYLAND_DISPLAY:-wayland-0}"
  rm -f \
    "$runtime_dir/${wayland_display}-awww-daemon.sock" \
    "$runtime_dir/${wayland_display}-awww-daemon.socket" \
    "$runtime_dir/${wayland_display}-awww-daemon..sock" \
    "$runtime_dir/${wayland_display}-awww-daemon..socket"
  awww-daemon -q >"$runtime_dir/awww-daemon-${wayland_display}.log" 2>&1 &

  local i
  for i in {1..20}; do
    if awww query >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.25
  done

  return 1
}

apply_wallpaper() {
  if ! ensure_daemon; then
    return 1
  fi

  local args=(--resize crop --transition-type simple --transition-step 255)
  local connected_outputs
  connected_outputs="$(get_connected_outputs)"

  if [ -n "$connected_outputs" ]; then
    # Apply the correct wallpaper to each output directly (no default-then-override).
    while IFS= read -r output; do
      [ -z "$output" ] && continue
      local path="${OUTPUT_WALLPAPERS[$output]:-$DEFAULT_WALLPAPER}"
      if [ -n "$path" ] && [ -f "$path" ]; then
        awww img --outputs "$output" "${args[@]}" "$path" >/dev/null 2>&1 || true
      fi
    done <<< "$connected_outputs"
  elif [ -n "$DEFAULT_WALLPAPER" ]; then
    # Fallback when we can't enumerate outputs (no compositor IPC available).
    awww img "${args[@]}" "$DEFAULT_WALLPAPER" >/dev/null 2>&1 || true
  fi
}

run_matugen() {
  if ! "$RUN_MATUGEN"; then
    return 0
  fi

  if [ -z "$THEME_WALLPAPER" ]; then
    return 0
  fi

  if ! command -v matugen >/dev/null 2>&1; then
    return 0
  fi

  if [ ! -f "$THEME_WALLPAPER" ]; then
    return 0
  fi

  matugen image "$THEME_WALLPAPER" -c "$HOME/.config/matugen/config.toml" >/dev/null 2>&1 || true
}

get_niri_outputs_sorted() {
  niri msg outputs 2>/dev/null | awk -F'[()]' '/^Output /{print $2}' | sort -u
}

get_hypr_outputs_sorted() {
  hyprctl monitors 2>/dev/null | awk '/^Monitor /{print $2}' | sort -u
}

get_connected_outputs() {
  if command -v hyprctl >/dev/null 2>&1 && hyprctl monitors >/dev/null 2>&1; then
    get_hypr_outputs_sorted
  elif command -v niri >/dev/null 2>&1 && niri msg outputs >/dev/null 2>&1; then
    get_niri_outputs_sorted
  fi
}

outputs_from_workspaces_line() {
  printf '%s\n' "$1" \
    | awk '{
        line = $0
        needle = "output: Some(\""
        while (1) {
          start = index(line, needle)
          if (start == 0) {
            break
          }
          line = substr(line, start + length(needle))
          finish = index(line, "\")")
          if (finish == 0) {
            break
          }
          output = substr(line, 1, finish - 1)
          if (output != "") {
            print output
          }
          line = substr(line, finish + 2)
        }
      }' \
    | sort -u
}

watch_niri() {
  local known_outputs
  local current_outputs
  local new_outputs

  known_outputs="$(get_niri_outputs_sorted || true)"

  niri msg event-stream 2>/dev/null | while IFS= read -r line; do
    case "$line" in
      Outputs\ changed:*)
        current_outputs="$(get_niri_outputs_sorted || true)"
        ;;
      Workspaces\ changed:*)
        current_outputs="$(outputs_from_workspaces_line "$line")"
        if [ -z "$current_outputs" ]; then
          current_outputs="$(get_niri_outputs_sorted || true)"
        fi
        ;;
      *)
        continue
        ;;
    esac

    if [ -n "$current_outputs" ]; then
      new_outputs="$(comm -13 <(printf '%s\n' "$known_outputs" | sort -u) <(printf '%s\n' "$current_outputs" | sort -u))"
      if [ -n "$new_outputs" ]; then
        apply_wallpaper
      fi
      known_outputs="$current_outputs"
    fi
  done
}

watch_hypr() {
  local known_outputs
  local current_outputs

  known_outputs="$(get_hypr_outputs_sorted || true)"

  while true; do
    sleep 2
    current_outputs="$(get_hypr_outputs_sorted || true)"
    if [ "$current_outputs" != "$known_outputs" ]; then
      apply_wallpaper
      known_outputs="$current_outputs"
    fi
  done
}

load_config
resolve_wallpapers

if ! ensure_daemon; then
  exit 0
fi

apply_wallpaper
run_matugen

if ! "$WATCH"; then
  exit 0
fi

if command -v niri >/dev/null 2>&1 && niri msg outputs >/dev/null 2>&1; then
  watch_niri
  exit 0
fi

if command -v hyprctl >/dev/null 2>&1 && hyprctl monitors >/dev/null 2>&1; then
  watch_hypr
  exit 0
fi

exit 0
