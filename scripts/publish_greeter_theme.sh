#!/usr/bin/env bash
# Publish a validated wallpaper/palette pair without exposing partial generations.
set -euo pipefail

if [[ $# -lt 1 || $# -gt 3 ]]; then
  printf 'Usage: %s WALLPAPER [CSS [ASSET_DIRECTORY]]\n' "$0" >&2
  exit 2
fi

python3 - "$1" "${2:-/var/cache/matugen/shell-greeter.css}" "${3:-/var/cache/shell-greeter}" <<'PY'
from pathlib import Path
import fcntl
import os
import re
import shutil
import sys
import tempfile
import gi

gi.require_version('GdkPixbuf', '2.0')
from gi.repository import GdkPixbuf

image, palette, root = map(Path, sys.argv[1:])
# Installation owns provisioning; an ordinary wallpaper change never asks for sudo.
if not root.is_dir() or not os.access(root, os.W_OK) or not palette.is_file():
    sys.exit(0)
css = palette.read_text()
roles = {'surface', 'item', 'hover', 'outline', 'text', 'muted', 'primary',
         'on_primary', 'error', 'warning'}
lines = [line.strip() for line in css.splitlines() if line.strip()]
found = set()
for line in lines:
    match = re.fullmatch(r'@define-color greeter_(\w+) #[0-9a-fA-F]{6};', line)
    if not match or match[1] not in roles or match[1] in found:
        raise SystemExit('Invalid greeter palette; keeping previous assets')
    found.add(match[1])
if found != roles:
    raise SystemExit('Incomplete greeter palette; keeping previous assets')
if image.stat().st_size > 64 * 1024 * 1024:
    raise SystemExit('Wallpaper exceeds 64 MiB; keeping previous assets')
info = GdkPixbuf.Pixbuf.get_file_info(str(image))
if info[0] is None or info[1] * info[2] > 64_000_000:
    raise SystemExit('Unsupported or oversized wallpaper; keeping previous assets')
# Decode before publication so a corrupt image cannot replace a working generation.
GdkPixbuf.Pixbuf.new_from_file(str(image))
with (root / '.publish.lock').open('a') as lock:
    fcntl.flock(lock, fcntl.LOCK_EX)
    generation = Path(tempfile.mkdtemp(prefix='generation-', dir=root))
    pointer = root / '.current-new'
    try:
        generation.chmod(0o755)
        shutil.copyfile(image, generation / 'wallpaper')
        (generation / 'theme.css').write_text(css)
        for file in generation.iterdir():
            file.chmod(0o644)
            with file.open('rb') as stream:
                os.fsync(stream.fileno())
        previous = os.readlink(root / 'current') if (root / 'current').is_symlink() else None
        pointer.unlink(missing_ok=True)
        pointer.symlink_to(generation.name)
        os.replace(pointer, root / 'current')
        # Keep the previous pair for recovery; no unbounded wallpaper cache growth.
        for old in root.glob('generation-*'):
            if old.is_dir() and not old.is_symlink() and old.name not in {generation.name, previous}:
                shutil.rmtree(old)
    except BaseException:
        pointer.unlink(missing_ok=True)
        if not (root / 'current').is_symlink() or os.readlink(root / 'current') != generation.name:
            shutil.rmtree(generation)
        raise
PY
