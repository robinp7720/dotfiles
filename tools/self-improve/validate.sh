#!/usr/bin/env bash
# Fixed policy, copied before Codex runs. Never execute commands from its report.
set -euo pipefail
repo="${1:?checkout required}"
base="${2:?base commit required}"
cd "$repo"
git diff --cached --check "$base"
mapfile -d '' -t paths < <(git diff --cached --name-only --no-renames -z "$base")
declare -A checks=()
for path in "${paths[@]}"; do
  # Reject changed symlinks and gitlinks (including newly introduced ones).
  for revision in "$base:$path" ":$path"; do
    type="$(git cat-file -t "$revision" 2>/dev/null || true)"
    [[ "$type" != commit ]] || { printf 'Submodule change rejected\n' >&2; exit 1; }
  done
  if git ls-tree "$base" -- "$path" | grep -q '^120000 '; then
    printf 'Existing symlink change rejected: %s\n' "$path" >&2; exit 1
  fi
  [[ ! -L "$path" ]] || { printf 'Symlink change rejected: %s\n' "$path" >&2; exit 1; }
  # Every changed path must have an explicit validation policy.
  case "$path" in
    scripts/codex_self_improve.sh|scripts/start_codex_self_improve_service.sh|hypr/monitors.lua)
      printf 'Protected path: %s\n' "$path" >&2; exit 1 ;;
    scripts/*.sh|hypr/scripts/*.sh)
      if [[ -e "$path" ]]; then bash -n "$path"; fi
      checks[tests/script_inventory_test.sh]=1
      name="${path##*/}"; name="${name%.sh}"
      if [[ -f "tests/${name}_test.sh" ]]; then checks["tests/${name}_test.sh"]=1; fi
      case "$path" in
        *album_art*) checks[tests/album_art_watcher_test.sh]=1 ;;
        *awww*) checks[tests/awww_wallpaper_test.sh]=1 ;;
        *lock*|*session_display*) checks[tests/session_lock_test.sh]=1 ;;
        *next_event*) checks[scripts/tests/next_event_test.sh]=1 ;;
      esac
      ;;
    systemd/user/codex-self-improve.service)
      printf 'Protected automation unit\n' >&2; exit 1 ;;
    systemd/user/*.service|systemd/user/*.timer)
      [[ ! -e "$path" ]] || systemd-analyze --user verify "$path"
      checks[tests/script_inventory_test.sh]=1 ;;
    niri/*.kdl)
      checks[tests/compositor_binding_parity_test.sh]=1
      niri validate -c niri/config.kdl ;;
    hypr/hyprland-config/*.lua)
      checks[tests/hyprland_lua_test.sh]=1
      checks[tests/compositor_binding_parity_test.sh]=1 ;;
    README.md|docs/*.md) ;;
    *) printf 'No automatic validation policy for %s\n' "$path" >&2; exit 1 ;;
  esac
done
for test in "${!checks[@]}"; do
  printf 'Running independent check: %s\n' "$test"
  bash "$test"
done
