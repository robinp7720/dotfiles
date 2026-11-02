#!/usr/bin/env bash
# Commit existing work first; improve and validate in an independent checkout.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
ASSET_DIR="$SCRIPT_DIR/../tools/self-improve"
REPO_DIR="${CODEX_SELF_IMPROVE_REPO_DIR:-$(cd -- "$SCRIPT_DIR/.." && pwd)}"
STATE_DIR="${XDG_STATE_HOME:-$HOME/.local/state}/codex-self-improve"
RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
CODEX_BIN="${CODEX_SELF_IMPROVE_CODEX_BIN:-codex}"
PROMPT_FILE="${CODEX_SELF_IMPROVE_PROMPT_FILE:-$ASSET_DIR/prompt.md}"
COOLDOWN_HOURS="${CODEX_SELF_IMPROVE_COOLDOWN_HOURS:-24}"
TIMEOUT_SECONDS="${CODEX_SELF_IMPROVE_TIMEOUT_SECONDS:-1800}"
AUTO_COMMIT="${CODEX_SELF_IMPROVE_AUTO_COMMIT:-1}"
FORCE_RUN=false

while (($# > 0)); do
  case "$1" in
    --force) FORCE_RUN=true ;;
    --no-commit) AUTO_COMMIT=0 ;;
    --help|-h)
      cat <<'USAGE'
Usage: codex_self_improve.sh [--force] [--no-commit]

Group and commit existing changes, then validate one isolated improvement.
  --force      Ignore the per-boot and cooldown gates (other checks still apply).
  --no-commit  Skip dirty repositories; retain improvements in an isolated checkout.
USAGE
      exit 0
      ;;
    *) printf 'Unknown argument: %s\n' "$1" >&2; exit 1 ;;
  esac
  shift
done

# Refuse to run without the lock; do not silently lose concurrency protection.
mkdir -p "$STATE_DIR/runs" "$RUNTIME_DIR"
exec 9>"$RUNTIME_DIR/codex-self-improve.lock"
command -v flock >/dev/null || { printf 'flock is required\n' >&2; exit 1; }
flock -n 9 || exit 0
umask 077
RUN_DIR="$(mktemp -d "$STATE_DIR/runs/$(date +%Y%m%dT%H%M%S)-XXXXXX")"
LOG_FILE="$RUN_DIR/run.log"
: >"$LOG_FILE"
: >"$RUN_DIR/report.json"
ln -sfn "$LOG_FILE" "$STATE_DIR/last-run.log"
ln -sfn "$RUN_DIR/summary.txt" "$STATE_DIR/last-summary.txt"
ln -sfn "$RUN_DIR/report.json" "$STATE_DIR/last-report.txt"
PHASE=preflight
FINISHED=false
WORK_DIR=""
COMMIT_REF=""

log() { printf '[%s] %s\n' "$(date -Is)" "$*" >>"$LOG_FILE"; }
finish() {
  local status="$1" message="$2"
  message="${message//$'\n'/ }"
  message="${message//$'\r'/ }"
  log "$status: $message"
  printf 'status=%s\nsummary=%s\nphase=%s\ncommit=%s\ncheckout=%s\nrun=%s\ntimestamp=%s\n' \
    "$status" "$message" "$PHASE" "$COMMIT_REF" "$WORK_DIR" "$RUN_DIR" "$(date -Is)" >"$RUN_DIR/summary.txt"
  FINISHED=true
  if [[ "$status" == failed && "${CODEX_SELF_IMPROVE_NOTIFY_FAILURES:-1}" == 1 ]]; then
    notify-send -a 'Codex Self Improve' -u critical 'Codex self-improve failed' "$message. See $LOG_FILE" >/dev/null 2>&1 || true
  elif [[ "$status" == changed || "$status" == pending_review ]]; then
    notify-send -a 'Codex Self Improve' 'Codex self-improve' "$message" >/dev/null 2>&1 || true
  fi
}
cleanup() {
  local rc=$?
  if ! "$FINISHED"; then
    finish failed "$PHASE stopped (exit $rc); retained artifacts in $RUN_DIR"
  fi
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
fail() { finish failed "$*"; exit 1; }
skip() { finish skipped "$*"; exit 0; }

[[ "${CODEX_SELF_IMPROVE_DISABLED:-0}" != 1 ]] || skip 'Disabled by environment'
for binary in git jq timeout "$CODEX_BIN"; do
  command -v "$binary" >/dev/null || fail "Required command unavailable: $binary"
done
[[ "$COOLDOWN_HOURS" =~ ^[0-9]{1,4}$ ]] || COOLDOWN_HOURS=24
[[ "$TIMEOUT_SECONDS" =~ ^[1-9][0-9]{0,4}$ ]] || fail 'Invalid timeout seconds'
REPO_DIR="$(git -C "$REPO_DIR" rev-parse --show-toplevel)"
[[ -f "$PROMPT_FILE" ]] || fail "Missing prompt: $PROMPT_FILE"
boot_id="$(cat /proc/sys/kernel/random/boot_id)"
now_epoch="$(date +%s)"
last_boot_id="$(cat "$STATE_DIR/last-boot-id" 2>/dev/null || true)"
last_run_epoch="$(cat "$STATE_DIR/last-run-epoch" 2>/dev/null || true)"
[[ "$last_run_epoch" =~ ^[0-9]{1,12}$ ]] || last_run_epoch=0
if ! "$FORCE_RUN"; then
  [[ "$boot_id" != "$last_boot_id" ]] || skip 'Already completed a run this boot'
  ((now_epoch - 10#$last_run_epoch >= 10#$COOLDOWN_HOURS * 3600)) || skip 'Cooldown still active'
fi

# Snapshot the policy before the agent can edit any repository files.
cp "$PROMPT_FILE" "$RUN_DIR/prompt.md"
cp "$ASSET_DIR/commit-prompt.md" "$ASSET_DIR/report.schema.json" "$ASSET_DIR/validate.sh" "$RUN_DIR/"
codex_cmd=("$CODEX_BIN" -a never exec --sandbox workspace-write --color never)
[[ -z "${CODEX_SELF_IMPROVE_PROFILE:-}" ]] || codex_cmd+=(-p "$CODEX_SELF_IMPROVE_PROFILE")
[[ -z "${CODEX_SELF_IMPROVE_MODEL:-}" ]] || codex_cmd+=(-m "$CODEX_SELF_IMPROVE_MODEL")
# Parsing help checks compatibility without sending a prompt or spending tokens.
"${codex_cmd[@]}" --output-schema "$RUN_DIR/report.schema.json" --help >>"$LOG_FILE" 2>&1 || fail 'Codex CLI is incompatible'

run_codex() {
  local directory="$1" prompt="$2" report="$3"
  shift 3
  log "Starting $PHASE in $directory"
  timeout --kill-after=15s "${TIMEOUT_SECONDS}s" "${codex_cmd[@]}" \
    -C "$directory" --output-schema "$RUN_DIR/report.schema.json" -o "$report" "$@" - \
    <"$prompt" >>"$LOG_FILE" 2>&1
}
read_report() {
  local report="$1"
  # Validate locally as well: an absent/invalid/failed model response never succeeds.
  jq -e '
    type == "object" and
    (keys == ["files", "status", "summary", "title", "validation"]) and
    (.status == "changed" or .status == "no_change") and
    (.title | type == "string" and length > 0 and (test("[\\r\\n]") | not)) and
    (.summary | type == "string" and length > 0) and
    ([.files, .validation] | all(type == "array" and all(type == "string" and length > 0)))
  ' "$report" >/dev/null || fail "Invalid or failed report during $PHASE"
}
clean() {
  local status
  status="$(git -C "$1" status --porcelain --untracked-files=all --ignore-submodules=none)" || return 1
  [[ -z "$status" ]]
}

# Compare file contents/modes using a temporary index, including untracked files.
# Gitlinks are checked separately in their own initialized repositories.
fingerprint() {
  local repo="$1" index_file
  index_file="$(mktemp "$RUN_DIR/index-XXXXXX")"
  rm -- "$index_file"
  GIT_INDEX_FILE="$index_file" git -C "$repo" read-tree HEAD || return 1
  GIT_INDEX_FILE="$index_file" git -C "$repo" add -A || return 1
  GIT_INDEX_FILE="$index_file" git -C "$repo" ls-files --stage -z |
    { grep -zv '^160000 ' || [[ $? == 1 ]]; } | sha256sum || return 1
  rm -f -- "$index_file"
}

# An explicit commit-only invocation comes BEFORE the improvement pass.
if ! clean "$REPO_DIR"; then
  [[ "$AUTO_COMMIT" == 1 ]] || skip 'Local changes present; --no-commit leaves them untouched'
  PHASE=commit_existing
  repos=("$REPO_DIR")
  git -C "$REPO_DIR" submodule foreach --quiet --recursive 'printf "%s\0" "$PWD"' >"$RUN_DIR/submodules"
  while IFS= read -r -d '' nested; do repos+=("$nested"); done <"$RUN_DIR/submodules"
  heads=() fingerprints=()
  for repo in "${repos[@]}"; do
    [[ -z "$(git -C "$repo" diff --name-only --diff-filter=U)" ]] || fail "Unresolved conflicts in $repo"
    heads+=("$(git -C "$repo" rev-parse HEAD)")
    fingerprints+=("$(fingerprint "$repo")")
  done
  git_dir="$(git -C "$REPO_DIR" rev-parse --path-format=absolute --git-common-dir)"
  run_codex "$REPO_DIR" "$RUN_DIR/commit-prompt.md" "$RUN_DIR/commit-report.json" \
    --add-dir "$git_dir" || fail 'Commit-only Codex invocation failed'
  read_report "$RUN_DIR/commit-report.json"
  for i in "${!repos[@]}"; do
    [[ "${fingerprints[$i]}" == "$(fingerprint "${repos[$i]}")" ]] || fail "Files changed during commit-only pass: ${repos[$i]}"
    git -C "${repos[$i]}" merge-base --is-ancestor "${heads[$i]}" HEAD || fail 'Commit pass rewrote history'
    clean "${repos[$i]}" || fail "Commit pass left uncommitted changes: ${repos[$i]}"
  done
fi
clean "$REPO_DIR" || fail 'Repository must be clean before improvement'
base="$(git -C "$REPO_DIR" rev-parse HEAD)"
branch="$(git -C "$REPO_DIR" symbolic-ref -q HEAD)" || fail 'Main repository has a detached HEAD'
printf '%s\n' "$base" >"$RUN_DIR/base-commit"

PHASE=improve
# Independent Git metadata and objects; the agent cannot alter the live index/refs.
WORK_DIR="$RUN_DIR/checkout"
git init -q "$WORK_DIR"
git -C "$WORK_DIR" -c core.hooksPath=/dev/null fetch -q --depth=1 "$REPO_DIR" "$base" >>"$LOG_FILE" 2>&1
git -C "$WORK_DIR" -c core.hooksPath=/dev/null checkout -q --detach FETCH_HEAD
run_codex "$WORK_DIR" "$RUN_DIR/prompt.md" "$RUN_DIR/report.json" || fail 'Improvement Codex invocation failed'
read_report "$RUN_DIR/report.json"
[[ "$(git -C "$WORK_DIR" rev-parse HEAD)" == "$base" ]] || fail 'Improvement pass created or rewrote commits'
git -C "$WORK_DIR" add -A
# Paths are always NUL-delimited, including paths supplied by the model.
git -C "$WORK_DIR" diff --cached --name-only --no-renames -z "$base" >"$RUN_DIR/changed-files"
jq -j '.files[] | ., "\u0000"' "$RUN_DIR/report.json" | sort -zu >"$RUN_DIR/reported-files"
sort -zu "$RUN_DIR/changed-files" >"$RUN_DIR/actual-files"
cmp -s "$RUN_DIR/reported-files" "$RUN_DIR/actual-files" || fail 'Report files do not match the actual changes'
status="$(jq -r .status "$RUN_DIR/report.json")"
if [[ ! -s "$RUN_DIR/changed-files" ]]; then
  [[ "$status" == no_change ]] || fail 'Report claims changes but checkout is unchanged'
else
  [[ "$status" == changed ]] || fail 'Report claims no change but checkout is modified'
  jq -e '.validation | length > 0' "$RUN_DIR/report.json" >/dev/null || fail 'No validation reported'
  PHASE=validate
  tree="$(git -C "$WORK_DIR" write-tree)"
  timeout --kill-after=15s "${TIMEOUT_SECONDS}s" bash "$RUN_DIR/validate.sh" "$WORK_DIR" "$base" \
    >>"$LOG_FILE" 2>&1 || fail 'Independent validation failed'
  git -C "$WORK_DIR" add -A
  [[ "$(git -C "$WORK_DIR" write-tree)" == "$tree" && "$(git -C "$WORK_DIR" rev-parse HEAD)" == "$base" ]] || fail 'Validation modified the candidate'
  git -C "$WORK_DIR" diff --cached --binary "$base" >"$RUN_DIR/change.patch"
  if [[ "$AUTO_COMMIT" != 1 ]]; then
    finish pending_review "Validated improvement retained in $WORK_DIR (--no-commit)"
    exit 0
  fi
  PHASE=integrate
  # Never stage live files. Git's fast-forward also refuses overwritten local edits.
  [[ "$(git -C "$REPO_DIR" rev-parse HEAD)" == "$base" && "$(git -C "$REPO_DIR" symbolic-ref -q HEAD)" == "$branch" ]] && clean "$REPO_DIR" || {
    finish pending_review "Live checkout changed; validated improvement retained in $WORK_DIR"
    exit 0
  }
  title="$(jq -r .title "$RUN_DIR/report.json")"
  [[ "$title" =~ ^(fix|feat|docs|style|refactor|perf|test|build|ci|chore)(\([^\)]+\))?:\ .+ ]] || fail 'Improvement title must be a Conventional Commit message'
  git -C "$WORK_DIR" -c core.hooksPath=/dev/null -c commit.gpgsign=false \
    -c user.name='Codex Self Improve' -c user.email=codex-self-improve@localhost \
    commit -qm "$title"
  COMMIT_REF="$(git -C "$WORK_DIR" rev-parse HEAD)"
  git -C "$REPO_DIR" -c core.hooksPath=/dev/null fetch --no-write-fetch-head "$WORK_DIR" "$COMMIT_REF" >>"$LOG_FILE" 2>&1
  [[ "$(git -C "$REPO_DIR" rev-parse HEAD)" == "$base" && "$(git -C "$REPO_DIR" symbolic-ref -q HEAD)" == "$branch" ]] && clean "$REPO_DIR" || {
    finish pending_review "Live checkout changed before integration; candidate retained in $WORK_DIR"
    exit 0
  }
  git -C "$REPO_DIR" -c core.hooksPath=/dev/null -c merge.autostash=false merge --ff-only "$COMMIT_REF" >>"$LOG_FILE" 2>&1 || fail 'Fast-forward refused; candidate retained'
fi

printf '%s\n' "$boot_id" >"$STATE_DIR/last-boot-id"
date +%s >"$STATE_DIR/last-run-epoch"
finish "$status" "$(jq -r .summary "$RUN_DIR/report.json")"
# Keep reports, logs and patches; successful checkouts can be recreated from Git.
rm -rf -- "$WORK_DIR"
