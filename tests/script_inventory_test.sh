#!/usr/bin/env bash
# Keep runtime references reproducible from the files recorded in Git.
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
python3 - "$ROOT" <<'PY'
from pathlib import Path
import re
import subprocess
import sys

root = Path(sys.argv[1])
entries = subprocess.check_output(
    ['git', '-C', str(root), 'ls-files', '--stage', '-z'], text=True
).split('\0')
tracked = {}
for entry in filter(None, entries):
    metadata, path = entry.split('\t', 1)
    tracked[path] = metadata.split()[0]

errors = []
checked = set()

def require(path, origin, executable=True):
    checked.add(path)
    if path not in tracked:
        errors.append(f'{origin}: required file is not tracked: {path}')
    elif not (root / path).is_file():
        errors.append(f'{origin}: required file is missing: {path}')
    elif executable and path.endswith('.sh') and tracked[path] != '100755':
        errors.append(f'{origin}: required script is not executable in Git: {path}')

for path in tracked:
    if not path.startswith(('hypr/', 'niri/', 'scripts/', 'systemd/', 'matugen/')):
        continue
    source = root / path
    if not source.is_file():
        continue
    try:
        text = source.read_text()
    except UnicodeError:
        continue
    for reference in re.findall(r'(?:~|%h|/home/robin)/\.dotfiles/([\w./-]+)', text):
        require(reference, path)
    for match in re.finditer(r'(?m)^.*?\$SCRIPT_DIR/([\w./-]+\.sh).*$', text):
        resolved = (source.parent / match.group(1)).resolve().relative_to(root)
        sourced = re.match(r'\s*(?:source|\.)\s', match.group())
        require(str(resolved), path, executable=not sourced)

for script in re.findall(r'"(scripts/[^":]+\.sh):[^"\n]+"', (root / 'setup.sh').read_text()):
    require(script, 'setup.sh')
for template in re.findall(r"input_path = '~/.config/matugen/([^']+)'", (root / 'matugen/config.toml').read_text()):
    require('matugen/' + template, 'matugen/config.toml')

if errors:
    raise SystemExit('\n'.join(errors))
print(f'Runtime inventory: {len(checked)} tracked scripts, templates, and assets verified.')
PY
