#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cargo build --quiet --locked --manifest-path "$ROOT/tools/wallpaper-adapter/Cargo.toml"
python3 - "$ROOT" <<'PY'
from pathlib import Path
import json, os, shutil, subprocess, sys, tempfile
root = Path(sys.argv[1])
binary = root / 'tools/wallpaper-adapter/target/debug/luma-wallpaper-dotfiles'
with tempfile.TemporaryDirectory(prefix='wallpaper adapter ') as temporary:
    base = Path(temporary)
    home, bin_dir = base / 'home', base / 'bin'
    scripts = home / '.dotfiles/scripts'
    for directory in (home, bin_dir, scripts, base / 'runtime'):
        directory.mkdir(parents=True, exist_ok=True)
    for name in ('awww_wallpaper_watcher.sh', 'publish_greeter_theme.sh'):
        shutil.copy2(root / 'scripts' / name, scripts / name)
    (scripts / 'publish_greeter_theme.sh').write_text('#!/bin/sh\nexit 0\n')
    config = scripts / 'awww_wallpapers.conf'
    image = base / "a 'quoted' $(literal).png"
    image.touch()
    config.write_text(f'# retained\ndefault={image}\ntheme={image}\nDP-2=/unchanged.png\n')
    stub = '''#!/usr/bin/python3
import json, os, sys
from pathlib import Path
name = Path(sys.argv[0]).name
with Path(os.environ['CALLS']).open('a') as stream: stream.write(json.dumps([name] + sys.argv[1:]) + '\\n')
if os.environ.get('FAIL') == name:
    print('intentional failure', file=sys.stderr); sys.exit(1)
if name == 'hyprctl':
    if '-j' in sys.argv: print('[{"name":"DP-1","focused":true,"description":"Main"},{"name":"DP-2","focused":false}]')
    else: print('Monitor DP-1 (ID 0):\\nMonitor DP-2 (ID 1):')
elif name == 'niri':
    if 'focused-output' in sys.argv: print('{"name":"HDMI-A-1"}')
    else: print('{"HDMI-A-1":{"current_mode":0},"eDP-1":{"current_mode":null}}')
'''
    for name in ('awww', 'hyprctl', 'niri', 'matugen', 'systemctl'):
        path = bin_dir / name; path.write_text(stub); path.chmod(0o755)
    env = dict(os.environ, HOME=str(home), XDG_CONFIG_HOME=str(home / '.config'),
               PATH=f'{bin_dir}:/usr/bin:/bin', XDG_RUNTIME_DIR=str(base / 'runtime'),
               AWWW_WALLPAPER_CONFIG=str(config), HYPRLAND_INSTANCE_SIGNATURE='test',
               CALLS=str(base / 'calls'))
    def call(operation, theme=False, failure='', **fields):
        return subprocess.run([str(binary)] + (['theme'] if theme else []),
            input=json.dumps(dict(version=1, operation=operation, **fields)),
            text=True, capture_output=True, env=dict(env, FAIL=failure))
    status = call('status'); assert status.returncode == 0, status.stderr
    outputs = json.loads(status.stdout)['outputs']
    assert outputs[0]['focused'] and outputs[0]['wallpaper_state'] == 'configured'
    assert outputs[0]['wallpaper'] == str(image)
    invalid = call('apply', output='missing', path=str(image)); assert invalid.returncode != 0
    before = config.read_text()
    failed = call('apply', output='DP-1', path=str(image), failure='awww')
    assert failed.returncode != 0 and config.read_text() == before
    result = call('apply', output='DP-1', path=str(image)); assert result.returncode == 0, result.stderr
    assert 'DP-2=/unchanged.png' in config.read_text() and f'theme={image}' in config.read_text()
    calls = [json.loads(line) for line in (base / 'calls').read_text().splitlines()]
    assert not any(c[0] == 'matugen' for c in calls)
    assert any(c[0] == 'awww' and c[-1] == str(image) for c in calls)
    assert any(c == ['systemctl', '--user', 'restart', 'awww-wallpaper.service'] for c in calls)
    other = base / 'other.png'; other.touch()
    before = config.read_text()
    failed = call('apply', theme=True, path=str(other), failure='matugen')
    assert failed.returncode != 0 and config.read_text() == before
    (base / 'calls').write_text('')
    result = call('apply', theme=True, path=str(other)); assert result.returncode == 0, result.stderr
    assert f'theme={other}' in config.read_text()
    calls = [json.loads(line) for line in (base / 'calls').read_text().splitlines()]
    assert any(c[0] == 'matugen' for c in calls)
    assert not any(c[0] in ('awww', 'systemctl') for c in calls)
    assert call('regenerate', theme=True).returncode == 0
    env.pop('HYPRLAND_INSTANCE_SIGNATURE'); env['NIRI_SOCKET'] = 'test'
    status = call('status'); assert status.returncode == 0, status.stderr
    assert [o['id'] for o in json.loads(status.stdout)['outputs']] == ['HDMI-A-1']
    # Setup is additive, keeps existing preferences, and does not overwrite adapters.
    luma_config = home / '.config/Luma/config.json'; luma_config.parent.mkdir(parents=True)
    luma_config.write_text('{"ui":{"width_px":800},"integrations":{"email":{"evolution_enabled":true}}}')
    subprocess.run([str(root / 'scripts/configure_luma_wallpapers.sh')], env=env, check=True)
    saved = json.loads(luma_config.read_text()); assert saved['ui']['width_px'] == 800
    assert saved['integrations']['email']['evolution_enabled']
    assert saved['integrations']['wallpapers']['theme']['update_on_selection']
    saved['integrations']['wallpapers']['backend']['command'] = ['custom-adapter']
    luma_config.write_text(json.dumps(saved)); before = luma_config.read_bytes()
    subprocess.run([str(root / 'scripts/configure_luma_wallpapers.sh')], env=env, check=True)
    assert luma_config.read_bytes() == before
    luma_config.write_text('invalid existing config')
    subprocess.run([str(root / 'scripts/configure_luma_wallpapers.sh')], env=env, check=True, capture_output=True)
    assert luma_config.read_text() == 'invalid existing config'
print('Wallpaper adapter: targeting, persistence, independent themes, failure propagation, Niri, and additive setup passed.')
PY
