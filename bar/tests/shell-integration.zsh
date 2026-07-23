#!/usr/bin/env zsh

emulate -L zsh
setopt errexit nounset pipefail

SCRIPT_DIR=${0:A:h}
REPO_ROOT=${SCRIPT_DIR:h:h}

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

tmpdir=$(mktemp -d)
trap 'rm -rf "$tmpdir"' EXIT

log_file="$tmpdir/vigil.log"
mkdir -p "$tmpdir/bin"
config_home="$tmpdir/config"
config_dir="$config_home/vigil"
config_path="$config_dir/config.toml"
mkdir -p "$config_dir"

cat >"$config_path" <<'EOF'
[[command_activity.allowlist]]
label = "Cargo build"
prefixes = ["cargo build"]

[[command_activity.allowlist]]
label = "Cargo test"
prefixes = ["cargo test"]

[[command_activity.allowlist]]
label = "Cargo run"
prefixes = ["cargo run"]

[[command_activity.allowlist]]
label = "npm test"
prefixes = ["npm test"]

[[command_activity.allowlist]]
label = "pnpm test"
prefixes = ["pnpm test"]

[[command_activity.allowlist]]
label = "Pytest"
prefixes = ["pytest"]

[[command_activity.allowlist]]
label = "Make"
prefixes = ["make"]

[[command_activity.allowlist]]
label = "Cargo nextest"
prefixes = ["cargo nextest"]
EOF

cat >"$tmpdir/bin/vigil" <<'EOF'
#!/usr/bin/env zsh
if [[ "$1" == "--config" && "$3" == "activity" && "$4" == "shell-rules" ]]; then
  [[ "$2" == "$EXPECTED_VIGIL_CONFIG" ]] || exit 9
  print -rn -- $'Cargo build\0cargo build\0Cargo test\0cargo test\0Cargo run\0cargo run\0npm test\0npm test\0pnpm test\0pnpm test\0Pytest\0pytest\0Make\0make\0Cargo nextest\0cargo nextest\0'
  exit 0
fi
print -r -- "$*" >>"$VIGIL_LOG_FILE"
EOF
chmod +x "$tmpdir/bin/vigil"

export PATH="$tmpdir/bin:$PATH"
export VIGIL_LOG_FILE="$log_file"
export XDG_CONFIG_HOME="$config_home"
export EXPECTED_VIGIL_CONFIG="$config_path"

source "$REPO_ROOT/bar/shell-integration.zsh"

assert_eq "$(classify_activity 'cargo nextest run')" "Cargo nextest" "cargo nextest should come from configured allowlist"
assert_eq "$(classify_activity 'cargo test -- --nocapture')" "Cargo test" "cargo test should map to Cargo test"
assert_eq "$(classify_activity 'pytest -k secret')" "Pytest" "pytest should map to Pytest"
assert_no_match "git status"

cd "$tmpdir"
__vigil_preexec "pytest -k secret"
setopt noerrexit
command sh -c 'exit 23'
__vigil_precmd
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
