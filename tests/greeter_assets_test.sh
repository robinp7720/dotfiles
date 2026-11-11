#!/usr/bin/env bash
# Real decoding and publication in an isolated cache, including failed generations.
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
work="$(mktemp -d)"
trap 'rm -rf -- "$work"' EXIT
mkdir "$work/assets"
python3 - "$ROOT" "$work" <<'PY'
from pathlib import Path
import re, sys, gi

gi.require_version('GdkPixbuf', '2.0')
from gi.repository import GdkPixbuf
root, work = map(Path, sys.argv[1:])
css = (root / 'tools/greeter/src/style.css').read_text()
(work / 'theme.css').write_text('\n'.join(re.findall(r'^@define-color.*;', css, re.M)) + '\n')
pixbuf = GdkPixbuf.Pixbuf.new(GdkPixbuf.Colorspace.RGB, False, 8, 2, 2)
pixbuf.fill(0x8ecbd6ff)
pixbuf.savev(str(work / 'wallpaper.png'), 'png', [], [])
PY
publish() { "$ROOT/scripts/publish_greeter_theme.sh" "$work/wallpaper.png" "$work/theme.css" "$work/assets"; }
publish
first="$(readlink "$work/assets/current")"
cmp "$work/wallpaper.png" "$work/assets/current/wallpaper"
cmp "$work/theme.css" "$work/assets/current/theme.css"
cp "$work/theme.css" "$work/good.css"
printf '{{unrendered}}\n' > "$work/theme.css"
if publish >/dev/null 2>&1; then
  printf 'Invalid template must not publish.\n' >&2; exit 1
fi
[[ "$(readlink "$work/assets/current")" == "$first" ]]
cp "$work/good.css" "$work/theme.css"
cp "$work/wallpaper.png" "$work/good.png"
printf 'broken image' > "$work/wallpaper.png"
if publish >/dev/null 2>&1; then
  printf 'Corrupt image must not publish.\n' >&2; exit 1
fi
[[ "$(readlink "$work/assets/current")" == "$first" ]]
cp "$work/good.png" "$work/wallpaper.png"
publish
[[ "$(readlink "$work/assets/current")" != "$first" ]]
[[ -d "$work/assets/$first" ]]
publish
[[ ! -d "$work/assets/$first" ]]
[[ "$(stat -c '%a' "$work/assets/current/wallpaper")" == 644 ]]
[[ "$(stat -Lc '%a' "$work/assets/current")" == 755 ]]
printf 'Greeter asset validation, atomic publication, and previous-generation retention passed.\n'
