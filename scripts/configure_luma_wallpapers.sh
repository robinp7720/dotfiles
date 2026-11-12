#!/usr/bin/env bash
# Install this desktop's adapter settings without overwriting user integrations.
set -euo pipefail
python3 - <<'PY'
from pathlib import Path
import json, os, sys, tempfile
path = Path(os.environ.get('XDG_CONFIG_HOME', str(Path.home() / '.config'))) / 'Luma/config.json'
try:
    config = json.loads(path.read_text()) if path.exists() else {}
except json.JSONDecodeError:
    print('Keeping unreadable Luma config; configure the wallpaper adapter manually.', file=sys.stderr)
    raise SystemExit(0)
if not isinstance(config, dict) or not isinstance(config.get('integrations', {}), dict):
    print('Keeping unrecognized Luma config; configure the wallpaper adapter manually.', file=sys.stderr)
    raise SystemExit(0)
integrations = config.setdefault('integrations', {})
wallpapers = integrations.get('wallpapers')
if wallpapers is not None and not isinstance(wallpapers, dict):
    raise SystemExit(0)
# Luma serializes empty defaults; those are also eligible for initial setup.
if wallpapers and (wallpapers.get('directories') or wallpapers.get('backend', {}).get('command') or wallpapers.get('theme', {}).get('command')):
    raise SystemExit(0)
integrations['wallpapers'] = {
    'directories': ['~/.dotfiles/wallpapers'], 'recursive': False,
    'backend': {'command': ['~/.local/bin/luma-wallpaper-dotfiles']},
    'theme': {'command': ['~/.local/bin/luma-wallpaper-dotfiles', 'theme'], 'update_on_selection': True},
}
path.parent.mkdir(parents=True, exist_ok=True)
if path.exists():
    backup = path.with_name('config.json.before-wallpaper-adapters')
    if not backup.exists():
        backup.write_bytes(path.read_bytes())
        backup.chmod(0o600)
fd, temporary = tempfile.mkstemp(prefix='.wallpaper-config-', dir=path.parent)
try:
    with os.fdopen(fd, 'w') as stream:
        json.dump(config, stream, indent=2)
        stream.write('\n')
        stream.flush()
        os.fsync(stream.fileno())
    os.replace(temporary, path)
finally:
    Path(temporary).unlink(missing_ok=True)
PY
