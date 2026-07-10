#!/usr/bin/env zsh

emulate -L zsh
setopt errexit nounset pipefail

SCRIPT_DIR=${0:A:h}
REPO_ROOT=${SCRIPT_DIR:h:h}

source "$REPO_ROOT/bar/shell-integration.zsh"

fail() {
  print -u2 -- "FAIL: $*"
  exit 1
}

assert_eq() {
  local actual=$1
  local expected=$2
  local message=$3
  [[ "$actual" == "$expected" ]] || fail "$message (expected '$expected', got '$actual')"
}

assert_no_match() {
  local command=$1
  local output
  if output=$(classify_activity "$command"); then
    fail "expected '$command' to be ignored, got '$output'"
  fi
}

assert_file_contains() {
  local path=$1
  local needle=$2
  local content
  content=$(<"$path")
  [[ "$content" == *"$needle"* ]] || fail "expected '$needle' in $path"
}

assert_file_lacks() {
  local path=$1
  local needle=$2
  local content
  content=$(<"$path")
  if [[ "$content" == *"$needle"* ]]; then
    fail "did not expect '$needle' in $path"
  fi
}

wait_for_lines() {
  local path=$1
  local expected=$2
  local attempt
  local content
  local -a lines

  for attempt in {1..100}; do
    if [[ -f "$path" ]]; then
      content=$(<"$path")
      lines=("${(@f)content}")
      if (( ${#lines} >= expected )); then
        return 0
      fi
    fi
    /usr/bin/sleep 0.05
  done

  fail "timed out waiting for $expected lines in $path"
}

assert_eq "$(classify_activity 'cargo test -- --nocapture')" "Cargo test" "cargo test should map to Cargo test"
assert_eq "$(classify_activity 'pytest -k secret')" "Pytest" "pytest should map to Pytest"
assert_no_match "git status"

tmpdir=$(mktemp -d)
trap 'rm -rf "$tmpdir"' EXIT

log_file="$tmpdir/cockpit-bar.log"
mkdir -p "$tmpdir/bin"

cat >"$tmpdir/bin/cockpit-bar" <<'EOF'
#!/usr/bin/env zsh
print -r -- "$*" >>"$COCKPIT_BAR_LOG_FILE"
EOF
chmod +x "$tmpdir/bin/cockpit-bar"

export PATH="$tmpdir/bin:$PATH"
export COCKPIT_BAR_LOG_FILE="$log_file"

cd "$tmpdir"
__cockpit_bar_preexec "pytest -k secret"
setopt noerrexit
command sh -c 'exit 23'
__cockpit_bar_precmd
setopt errexit

wait_for_lines "$log_file" 2

log_lines=("${(@f)$(<"$log_file")}")
start_line=${log_lines[1]}
finish_line=${log_lines[2]}

assert_file_contains "$log_file" "activity start"
assert_file_contains "$log_file" "activity finish"
assert_file_contains "$log_file" "Pytest"
assert_file_contains "$log_file" "$tmpdir"
assert_file_contains "$log_file" "23"
assert_file_lacks "$log_file" "pytest -k secret"
assert_file_lacks "$log_file" "secret"

start_id=${${(z)start_line}[4]}
finish_id=${${(z)finish_line}[4]}
[[ -n "$start_id" ]] || fail "expected a generated start id"
assert_eq "$start_id" "$finish_id" "start and finish should use the same activity id"

print -- "PASS"
