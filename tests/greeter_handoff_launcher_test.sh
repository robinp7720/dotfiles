#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
python3 - "$ROOT" <<'PY'
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
root = Path(sys.argv[1])
with tempfile.TemporaryDirectory(prefix='greeter-launcher-') as directory:
    base = Path(directory)
    handoff = base / 'handoff'
    handoff.mkdir()
    (handoff / 'libaquamarine.so.15').touch()
    system = base / 'system-library'
    system.write_text('tested ABI')
    manifest = handoff / 'system.sha256'
    manifest.write_text(f'{hashlib.sha256(system.read_bytes()).hexdigest()}  {system}\n')
    executable = base / 'start-hyprland'
    executable.write_text('''#!/usr/bin/env python3
import json, os, sys
print(json.dumps({'args': sys.argv[1:], 'path': os.getenv('LD_LIBRARY_PATH'),
                  'preserve': os.getenv('AQ_PRESERVE_OUTPUTS_ON_EXIT')}))
''')
    executable.chmod(0o755)
    launcher = base / 'launcher'
    launcher.write_text((root / 'shell-greeter/compositor.sh').read_text()
        .replace('/usr/local/lib/shell-greeter/handoff', str(handoff))
        .replace('/usr/bin/start-hyprland', str(executable)))
    env = dict(os.environ, AQ_PRESERVE_OUTPUTS_ON_EXIT='1')
    env.pop('LD_LIBRARY_PATH', None)
    def run():
        result = subprocess.run(['bash', str(launcher)], env=env, text=True,
                                capture_output=True, check=True)
        data = json.loads(result.stdout)
        assert data['args'] == ['--', '--config', '/etc/shell-greeter/hyprland.lua']
        return data
    assert run()['path'] == str(handoff)
    assert run()['preserve'] == '1'
    system.write_text('upgraded ABI')
    assert run()['path'] is None and run()['preserve'] is None
    manifest.unlink()
    assert run()['path'] is None and run()['preserve'] is None
print('Greeter launcher: matching ABI uses private backend; changed/missing ABI falls back to stock.')
PY
