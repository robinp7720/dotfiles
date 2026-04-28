#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="${CODEX_SELF_IMPROVE_REPO_DIR:-$(cd -- "$SCRIPT_DIR/.." && pwd)}"
STATE_DIR="${XDG_STATE_HOME:-$HOME/.local/state}/codex-self-improve"
RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
LOCK_FILE="$RUNTIME_DIR/codex-self-improve.lock"
PROMPT_FILE="${CODEX_SELF_IMPROVE_PROMPT_FILE:-$REPO_DIR/tools/self-improve/prompt.md}"
CODEX_BIN="${CODEX_SELF_IMPROVE_CODEX_BIN:-codex}"
COOLDOWN_HOURS="${CODEX_SELF_IMPROVE_COOLDOWN_HOURS:-24}"
AUTO_COMMIT="${CODEX_SELF_IMPROVE_AUTO_COMMIT:-1}"
NOTIFY_FAILURES="${CODEX_SELF_IMPROVE_NOTIFY_FAILURES:-1}"
FORCE_RUN=false

REPORT_FILE="$STATE_DIR/last-report.txt"
SUMMARY_FILE="$STATE_DIR/last-summary.txt"
LOG_FILE="$STATE_DIR/last-run.log"
LAST_BOOT_FILE="$STATE_DIR/last-boot-id"
LAST_RUN_FILE="$STATE_DIR/last-run-epoch"

usage() {
  cat <<'USAGE'
Usage: codex_self_improve.sh [--force] [--no-commit]

Runs a guarded Codex pass against this dotfiles repository.

Options:
  --force      Ignore the per-boot and cooldown gates.
  --no-commit  Keep successful changes in the working tree.
  --help       Show this message.
USAGE
}

while (($# > 0)); do
  case "$1" in
    --force)
      FORCE_RUN=true
      ;;
    --no-commit)
      AUTO_COMMIT=0
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      printf 'Unknown argument: %s\n' "$1" >&2
      exit 1
      ;;
  esac
  shift
done

mkdir -p "$STATE_DIR"

if [[ ! "$COOLDOWN_HOURS" =~ ^[0-9]+$ ]]; then
  COOLDOWN_HOURS=24
fi

timestamp() {
  date +"%Y-%m-%d %H:%M:%S"
}

log() {
  printf '[%s] %s\n' "$(timestamp)" "$*" >>"$LOG_FILE"
}

send_notification() {
  local urgency="$1"
  local title="$2"
  local body="$3"

  if command -v dunstify >/dev/null 2>&1; then
    dunstify \
      -a "Codex Self Improve" \
      -u "$urgency" \
      -h string:x-dunst-stack-tag:codex-self-improve \
      "$title" \
      "$body" >/dev/null 2>&1 || true
    return 0
  fi

  if command -v notify-send >/dev/null 2>&1; then
    notify-send -a "Codex Self Improve" -u "$urgency" "$title" "$body" >/dev/null 2>&1 || true
  fi
}

trim() {
  local value="${1:-}"
  value="${value#"${value%%[![:space:]]*}"}"
  value="${value%"${value##*[![:space:]]}"}"
  printf '%s' "$value"
}

report_field() {
  local key="$1"
  local value=""

  if [[ -f "$REPORT_FILE" ]]; then
    value="$(sed -n "s/^${key}: //p" "$REPORT_FILE" | head -n1)"
  fi

  trim "$value"
}

current_boot_id() {
  if [[ -r /proc/sys/kernel/random/boot_id ]]; then
    cat /proc/sys/kernel/random/boot_id
  fi
}

collect_changed_files() {
  (
    cd "$REPO_DIR"
    {
      git diff --name-only --relative HEAD --
      git ls-files --others --exclude-standard
    } | sed '/^$/d' | sort -u
  )
}

format_changed_files() {
  local -a files=("$@")
  local joined=""
  local shown=0
  local file

  for file in "${files[@]}"; do
    if ((shown == 3)); then
      joined+=", +$(( ${#files[@]} - shown )) more"
      break
    fi

    if [[ -n "$joined" ]]; then
      joined+=", "
    fi
    joined+="$file"
    shown=$((shown + 1))
  done

  printf '%s' "$joined"
}

write_summary() {
  local status="$1"
  local title="$2"
  local summary="$3"
  local files="$4"
  local validation="$5"
  local commit_ref="${6:-}"

  cat >"$SUMMARY_FILE" <<EOF
status=$status
title=$title
summary=$summary
files=$files
validation=$validation
commit=$commit_ref
timestamp=$(timestamp)
EOF
}

commit_changes() {
  local title="$1"
  local commit_message="chore(self-improve): $title"
  local commit_output

  commit_output="$(mktemp)"
  trap 'rm -f "$commit_output"' RETURN

  (
    cd "$REPO_DIR"
    git add -A
    GIT_AUTHOR_NAME="Codex Self Improve" \
    GIT_AUTHOR_EMAIL="codex-self-improve@localhost" \
    GIT_COMMITTER_NAME="Codex Self Improve" \
    GIT_COMMITTER_EMAIL="codex-self-improve@localhost" \
      git commit -m "$commit_message"
  ) >"$commit_output" 2>&1 || {
    log "git commit failed"
    cat "$commit_output" >>"$LOG_FILE"
    return 1
  }

  cat "$commit_output" >>"$LOG_FILE"
  (
    cd "$REPO_DIR"
    git rev-parse --short HEAD
  )
}

exec 9>"$LOCK_FILE"
if command -v flock >/dev/null 2>&1; then
  if ! flock -n 9; then
    exit 0
  fi
fi

: >"$LOG_FILE"
log "starting codex self-improve run"

if [[ "${CODEX_SELF_IMPROVE_DISABLED:-0}" == "1" ]]; then
  log "run disabled by environment"
  exit 0
fi

if ! git -C "$REPO_DIR" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  log "repository not found or not a git worktree: $REPO_DIR"
  exit 1
fi

if [[ ! -f "$PROMPT_FILE" ]]; then
  log "prompt file missing: $PROMPT_FILE"
  exit 1
fi

if ! command -v git >/dev/null 2>&1; then
  log "git is unavailable"
  exit 1
fi

if ! command -v "$CODEX_BIN" >/dev/null 2>&1; then
  log "codex binary is unavailable: $CODEX_BIN"
  if [[ "$NOTIFY_FAILURES" == "1" ]]; then
    send_notification "critical" "Codex self-improve skipped" "The codex CLI was not found in PATH."
  fi
  exit 1
fi

boot_id="$(current_boot_id || true)"
last_boot_id=""
last_run_epoch=0

if [[ -f "$LAST_BOOT_FILE" ]]; then
  last_boot_id="$(cat "$LAST_BOOT_FILE")"
fi

if [[ -f "$LAST_RUN_FILE" ]]; then
  last_run_epoch="$(cat "$LAST_RUN_FILE")"
fi

now_epoch="$(date +%s)"
cooldown_seconds=$((COOLDOWN_HOURS * 3600))

if ! "$FORCE_RUN"; then
  if [[ -n "$boot_id" && "$boot_id" == "$last_boot_id" ]]; then
    log "skipping: already ran for this boot"
    exit 0
  fi

  if ((cooldown_seconds > 0 && now_epoch - last_run_epoch < cooldown_seconds)); then
    log "skipping: cooldown still active"
    exit 0
  fi
fi

if [[ -n "$(git -C "$REPO_DIR" status --porcelain)" ]]; then
  log "skipping: repository has local changes"
  if [[ "$NOTIFY_FAILURES" == "1" ]]; then
    send_notification "normal" \
      "Codex self-improve skipped" \
      "The dotfiles repo has local changes, so the automatic run left it untouched."
  fi
  exit 0
fi

report_tmp="$(mktemp)"
log_tmp="$(mktemp)"
trap 'rm -f "$report_tmp" "$log_tmp"' EXIT

codex_cmd=(
  "$CODEX_BIN"
  exec
  --full-auto
  --color never
  -C "$REPO_DIR"
  -o "$report_tmp"
)

if [[ -n "${CODEX_SELF_IMPROVE_PROFILE:-}" ]]; then
  codex_cmd+=(-p "$CODEX_SELF_IMPROVE_PROFILE")
fi

if [[ -n "${CODEX_SELF_IMPROVE_MODEL:-}" ]]; then
  codex_cmd+=(-m "$CODEX_SELF_IMPROVE_MODEL")
fi

log "running: ${codex_cmd[*]}"
codex_exit=0
if ! "${codex_cmd[@]}" - <"$PROMPT_FILE" >"$log_tmp" 2>&1; then
  codex_exit=$?
fi

cp "$report_tmp" "$REPORT_FILE"
cat "$log_tmp" >>"$LOG_FILE"

mapfile -t changed_files < <(collect_changed_files)
changed_files_display="$(format_changed_files "${changed_files[@]}")"

status="$(report_field STATUS)"
title="$(report_field TITLE)"
summary="$(report_field SUMMARY)"
validation="$(report_field VALIDATION)"

if [[ -z "$status" ]]; then
  if ((codex_exit == 0)); then
    status="no_change"
  else
    status="failed"
  fi
fi

if ((codex_exit != 0)); then
  status="failed"
fi

if [[ -z "$title" ]]; then
  if ((${#changed_files[@]} > 0)); then
    title="Automatic dotfiles improvement"
  else
    title="No dotfiles changes"
  fi
fi

if [[ -z "$summary" ]]; then
  if ((${#changed_files[@]} > 0)); then
    summary="Codex applied one automatic improvement to the dotfiles repository."
  elif [[ "$status" == "failed" ]]; then
    summary="The Codex run failed before it could finish cleanly."
  else
    summary="Codex inspected the repo and left it unchanged."
  fi
fi

if ((codex_exit != 0)); then
  write_summary "$status" "$title" "$summary" "$changed_files_display" "$validation"

  if [[ "$NOTIFY_FAILURES" == "1" ]]; then
    send_notification "critical" \
      "Codex self-improve failed" \
      "$summary"
  fi
  exit 1
fi

if ((${#changed_files[@]} == 0)); then
  log "run completed without changes"
  write_summary "no_change" "$title" "$summary" "" "$validation"

  printf '%s\n' "$boot_id" >"$LAST_BOOT_FILE"
  printf '%s\n' "$now_epoch" >"$LAST_RUN_FILE"
  exit 0
fi

commit_ref=""
if [[ "$AUTO_COMMIT" == "1" ]]; then
  if ! commit_ref="$(commit_changes "$title")"; then
    write_summary "failed" "$title" "$summary" "$changed_files_display" "$validation"

    if [[ "$NOTIFY_FAILURES" == "1" ]]; then
      send_notification "critical" \
        "Codex self-improve failed" \
        "Codex changed files, but the automatic commit failed. Review $REPO_DIR manually."
    fi
    exit 1
  fi
fi

write_summary "changed" "$title" "$summary" "$changed_files_display" "$validation" "$commit_ref"
log "run completed with changes"

printf '%s\n' "$boot_id" >"$LAST_BOOT_FILE"
printf '%s\n' "$now_epoch" >"$LAST_RUN_FILE"

notification_body="$summary"
if [[ -n "$changed_files_display" ]]; then
  notification_body+="
Files: $changed_files_display"
fi
if [[ -n "$commit_ref" ]]; then
  notification_body+="
Commit: $commit_ref"
fi

send_notification "normal" "$title" "$notification_body"
