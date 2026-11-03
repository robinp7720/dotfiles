#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
python3 - "$ROOT" <<'PY'
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

root = Path(sys.argv[1])
services = {
    'vigil.service', 'awww-daemon.service', 'awww-wallpaper.service',
    'hypridle.service', 'cliphist.service',
}
mock = '''#!/usr/bin/env python3
import json
import os
from pathlib import Path
import sys
import time

state_path = Path(os.environ['STARTUP_TEST_STATE'])
state = json.loads(state_path.read_text())
args = sys.argv[1:]
if Path(sys.argv[0]).name == 'dbus-update-activation-environment':
    assert args == ['--all'], args
    action = 'dbus'
else:
    assert args.pop(0) == '--user', args
    action = args.pop(0)
state['calls'].append(action)
state_path.write_text(json.dumps(state))
if action == os.environ.get('STARTUP_TEST_FAIL'):
    sys.exit(1)
if action == 'dbus':
    # The import must finish before any service can use its environment.
    time.sleep(0.05)
    state['dbus'] = True
elif action == 'import-environment':
    assert state.get('dbus'), 'systemd imported before D-Bus finished'
    state['environment'] = {key: os.environ[key] for key in args}
elif action == 'reset-failed':
    assert state.get('environment'), 'reset before session import'
    state['rate_limited'] = False
    state['reset_units'] = args
elif action == 'restart':
    assert not state['rate_limited'], 'service still start-limit-hit'
    assert set(args) == set(state['reset_units']), 'unreset service restarted'
    state['started_units'] = args
else:
    raise AssertionError(action)
state_path.write_text(json.dumps(state))
'''

with tempfile.TemporaryDirectory(prefix='desktop-startup-test-') as temp:
    temp = Path(temp)
    bin_dir = temp / 'bin'
    bin_dir.mkdir()
    for name in ('systemctl', 'dbus-update-activation-environment'):
        executable = bin_dir / name
        executable.write_text(mock)
        executable.chmod(0o755)

    def run(compositor, fail='', missing=''):
        state_path = temp / 'state.json'
        state_path.write_text(json.dumps({'calls': [], 'rate_limited': True}))
        env = dict(os.environ)
        env.pop('DISPLAY', None)  # Optional, absent in some Wayland sessions.
        env.update(
            PATH=f'{bin_dir}:{env["PATH"]}',
            STARTUP_TEST_STATE=str(state_path), STARTUP_TEST_FAIL=fail,
            WAYLAND_DISPLAY='wayland-new', XDG_CURRENT_DESKTOP=compositor,
            HYPRLAND_INSTANCE_SIGNATURE='hyprland-new', NIRI_SOCKET='/run/niri-new',
        )
        if missing:
            env.pop(missing)
        result = subprocess.run(
            ['bash', str(root / 'scripts/start_desktop_services.sh'), compositor],
            env=env, text=True, capture_output=True,
        )
        state = json.loads(state_path.read_text())
        if fail or missing:
            assert result.returncode != 0, (compositor, fail, missing)
            assert 'started_units' not in state, state
            return state
        assert result.returncode == 0, result.stderr
        assert state['calls'] == ['dbus', 'import-environment', 'reset-failed', 'restart'], state
        assert set(state['started_units']) == services, state
        imported = state['environment']
        assert imported['WAYLAND_DISPLAY'] == 'wayland-new', imported
        assert 'DISPLAY' not in imported, imported
        if compositor == 'hyprland':
            assert imported['HYPRLAND_INSTANCE_SIGNATURE'] == 'hyprland-new', imported
            assert imported['NIRI_SOCKET'] == '', imported
        else:
            assert imported['NIRI_SOCKET'] == '/run/niri-new', imported
            assert imported['HYPRLAND_INSTANCE_SIGNATURE'] == '', imported
        return state

    for compositor, socket in (
        ('hyprland', 'HYPRLAND_INSTANCE_SIGNATURE'), ('niri', 'NIRI_SOCKET'),
    ):
        run(compositor)
        for fail in ('dbus', 'import-environment', 'reset-failed'):
            run(compositor, fail=fail)
        for missing in ('WAYLAND_DISPLAY', socket):
            assert run(compositor, missing=missing)['calls'] == []

print('Desktop startup: ordered imports, start-limit recovery, compositor switching, and failure handling verified.')
PY
