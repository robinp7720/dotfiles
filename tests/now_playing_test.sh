#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
SCRIPT_UNDER_TEST="$ROOT_DIR/scripts/now_playing.sh"

fail() {
  printf 'not ok - %s\n' "$*" >&2
  exit 1
}

test_decodes_local_album_art_uri() {
  local workdir="$1"
  local fake_bin="$workdir/bin"
  local art_dir="$workdir/album art 100%"
  local source_art="$art_dir/cover #1.jpg"
  local cached_art="$workdir/home/.cache/hyprlock/album.jpg"
  local encoded_path=""

  mkdir -p "$fake_bin" "$art_dir" "$workdir/home"
  printf 'fake album art\n' >"$source_art"

  cat >"$fake_bin/playerctl" <<'FAKE_PLAYERCTL'
#!/usr/bin/env bash
set -euo pipefail

if [[ "${1:-}" == "-l" ]]; then
  printf 'test-player\n'
  exit 0
fi

if [[ "${1:-}" == "--player=test-player" && "${2:-}" == "metadata" ]]; then
  printf 'Playing\x1fTest Artist\x1fTest Title\x1f%s\n' "$FAKE_ART_URI"
  exit 0
fi

exit 1
FAKE_PLAYERCTL
  chmod +x "$fake_bin/playerctl"

  encoded_path="${source_art//%/%25}"
  encoded_path="${encoded_path// /%20}"
  encoded_path="${encoded_path//#/%23}"

  HOME="$workdir/home" \
  PATH="$fake_bin:$PATH" \
  FAKE_ART_URI="file://$encoded_path" \
    "$SCRIPT_UNDER_TEST" --quiet

  [[ -f "$cached_art" ]] || fail "URL-encoded local album art was not cached"
  cmp -s "$source_art" "$cached_art" || fail "cached album art differs from the source"
}

test_preserves_pipe_characters_in_metadata() {
  local workdir="$1"
  local fake_bin="$workdir/bin"
  local output=""

  mkdir -p "$fake_bin" "$workdir/home"

  cat >"$fake_bin/playerctl" <<'FAKE_PLAYERCTL'
#!/usr/bin/env bash
set -euo pipefail

if [[ "${1:-}" == "-l" ]]; then
  printf 'test-player\n'
  exit 0
fi

if [[ "${1:-}" == "--player=test-player" && "${2:-}" == "metadata" ]]; then
  printf 'Playing\x1fOne | Two\x1fLeft | Right\x1f\n'
  exit 0
fi

exit 1
FAKE_PLAYERCTL
  chmod +x "$fake_bin/playerctl"

  output="$(HOME="$workdir/home" PATH="$fake_bin:$PATH" "$SCRIPT_UNDER_TEST")"
  [[ "$output" == '🎵 One | Two — Left | Right' ]] \
    || fail "pipe characters in media metadata were not preserved: $output"
}

main() {
  local tmpdir
  tmpdir="$(mktemp -d)"
  trap "rm -rf '$tmpdir'" EXIT

  test_decodes_local_album_art_uri "$tmpdir/encoded-art"
  test_preserves_pipe_characters_in_metadata "$tmpdir/pipe-metadata"

  printf 'ok - now playing tests passed\n'
}

main "$@"
