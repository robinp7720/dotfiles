#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
SCRIPT_UNDER_TEST="$ROOT_DIR/scripts/codex_self_improve.sh"

fail() {
  printf 'not ok - %s\n' "$*" >&2
  exit 1
}

assert_file_contains() {
  local file="$1"
  local expected="$2"

  [[ -f "$file" ]] || fail "missing file: $file"
  grep -Fqx "$expected" "$file" || {
    printf 'Expected %s to contain exact line: %s\n' "$file" "$expected" >&2
    printf 'Actual contents:\n' >&2
    sed -n '1,120p' "$file" >&2
    exit 1
  }
}

new_repo() {
  local repo="$1"

  mkdir -p "$repo"
  git -C "$repo" init -q
  git -C "$repo" config user.name "Self Improve Test"
  git -C "$repo" config user.email "self-improve-test@localhost"
  printf 'initial\n' >"$repo/README.md"
  git -C "$repo" add README.md
  git -C "$repo" commit -q -m "test: initial"
}

run_wrapper() {
  local workdir="$1"
  local codex_bin="$2"
  local repo="$workdir/repo"
  local state_home="$workdir/state"
  local runtime_dir="$workdir/runtime"
  local prompt_file="$workdir/prompt.md"
  local output_file="$workdir/output.log"

  mkdir -p "$state_home" "$runtime_dir"
  printf 'test prompt\n' >"$prompt_file"

  set +e
  HOME="$workdir/home" \
  XDG_STATE_HOME="$state_home" \
  XDG_RUNTIME_DIR="$runtime_dir" \
  CODEX_SELF_IMPROVE_REPO_DIR="$repo" \
  CODEX_SELF_IMPROVE_PROMPT_FILE="$prompt_file" \
  CODEX_SELF_IMPROVE_CODEX_BIN="$codex_bin" \
  CODEX_SELF_IMPROVE_COOLDOWN_HOURS=0 \
  CODEX_SELF_IMPROVE_NOTIFY_FAILURES=0 \
    "$SCRIPT_UNDER_TEST" --force --no-commit >"$output_file" 2>&1
  local exit_code=$?
  set -e

  printf '%s' "$exit_code"
}

test_failed_codex_marks_run_failed() {
  local workdir="$1"
  local repo="$workdir/repo"
  local fake_codex="$workdir/fake-codex"

  new_repo "$repo"

  cat >"$fake_codex" <<'FAKE_CODEX'
#!/usr/bin/env bash
printf 'simulated codex failure\n' >&2
exit 42
FAKE_CODEX
  chmod +x "$fake_codex"

  exit_code="$(run_wrapper "$workdir" "$fake_codex")"

  [[ "$exit_code" != "0" ]] || fail "failed codex command should make wrapper fail"
  assert_file_contains "$workdir/state/codex-self-improve/last-summary.txt" "status=failed"
  [[ ! -f "$workdir/state/codex-self-improve/last-run-epoch" ]] || {
    fail "failed runs must not update cooldown state"
  }
}

test_successful_no_change_updates_cooldown() {
  local workdir="$1"
  local repo="$workdir/repo"
  local fake_codex="$workdir/fake-codex"

  new_repo "$repo"

  cat >"$fake_codex" <<'FAKE_CODEX'
#!/usr/bin/env bash
report_file=""
while (($# > 0)); do
  if [[ "$1" == "-o" || "$1" == "--output-last-message" ]]; then
    shift
    report_file="$1"
  fi
  shift || true
done
cat >"$report_file" <<'REPORT'
STATUS: no_change
TITLE: No change
SUMMARY: Nothing worth changing.
FILES: none
VALIDATION: none
REPORT
FAKE_CODEX
  chmod +x "$fake_codex"

  exit_code="$(run_wrapper "$workdir" "$fake_codex")"

  [[ "$exit_code" == "0" ]] || fail "successful codex command should make wrapper succeed"
  assert_file_contains "$workdir/state/codex-self-improve/last-summary.txt" "status=no_change"
  [[ -f "$workdir/state/codex-self-improve/last-run-epoch" ]] || {
    fail "successful runs should update cooldown state"
  }
}

main() {
  local tmpdir
  tmpdir="$(mktemp -d)"
  trap "rm -rf '$tmpdir'" EXIT

  test_failed_codex_marks_run_failed "$tmpdir/failed"
  test_successful_no_change_updates_cooldown "$tmpdir/no-change"

  printf 'ok - codex self-improve wrapper tests passed\n'
}

main "$@"
