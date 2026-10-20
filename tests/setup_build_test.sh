#!/usr/bin/env bash
# Exercise setup from an unbuilt checkout without touching the live desktop.
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
python3 - "$ROOT" <<'PY'
from pathlib import Path
import os
import shutil
import subprocess
import sys
import tempfile

root = Path(sys.argv[1])
stub_source = r'''#!/usr/bin/env python3
from pathlib import Path
import os
import sys

name = Path(sys.argv[0]).name
args = sys.argv[1:]
repo = Path(os.environ['SETUP_TEST_REPO'])
home = Path(os.environ['HOME'])
log = Path(os.environ['SETUP_TEST_LOG'])
with log.open('a') as stream:
    stream.write(name + ' ' + ' '.join(args) + '\n')

if name == 'git':
    assert args[:6] == ['-C', str(repo), 'submodule', 'update', '--init', '--recursive']
    assert args[6] == '--'
    (repo / args[7] / 'Cargo.toml').touch()
elif name == 'cargo':
    assert args[:4] == ['build', '--release', '--locked', '--bins']
    manifest = Path(args[args.index('--manifest-path') + 1])
    target = Path(args[args.index('--target-dir') + 1])
    assert target == manifest.parent / 'target'
    assert manifest.is_file()
    assert not (home / '.zshrc').is_symlink()
    tool = manifest.parent.name
    if os.environ.get('SETUP_TEST_FAIL') == tool:
        sys.exit(17)
    for binary in {'bar': ['vigil'], 'launcher': ['Luma', 'luma-mail-eds'], 'tv-mode': ['tv-mode']}[tool]:
        output = target / 'release' / binary
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text('#!/usr/bin/env bash\nexit 0\n')
        output.chmod(0o755)
elif name == 'make':
    assert args[:2] == ['-C', str(repo / 'tools/hypr-swipe-events')]
    if args[2] == 'all':
        assert not (home / '.zshrc').is_symlink()
        if os.environ.get('SETUP_TEST_FAIL') == 'plugin':
            sys.exit(18)
        (repo / 'tools/hypr-swipe-events/hypr-swipe-events.so').touch()
    else:
        assert args[2] == 'install'
        assert (home / '.zshrc').is_symlink()
        assert (repo / 'tools/hypr-swipe-events/hypr-swipe-events.so').is_file()
        destination = Path(args[3].removeprefix('PLUGIN_DIR='))
        destination.mkdir(parents=True)
        (destination / 'hypr-swipe-events.so').touch()
elif name == 'systemctl':
    assert (home / '.local/bin/vigil').is_file()
    assert (home / '.local/bin/Luma').is_file()
    assert (home / '.local/bin/luma-mail-eds').is_file()
    assert (home / '.local/bin/tv-mode').is_file()
else:
    raise AssertionError(name)
'''

def check(failure='', missing_submodule=''):
    with tempfile.TemporaryDirectory(prefix='dotfiles setup build ') as temporary:
        base = Path(temporary)
        repo, home, stubs = [base / name for name in ('repo', 'home', 'stubs')]
        for directory in (repo, home, stubs):
            directory.mkdir()
        shutil.copy2(root / 'setup.sh', repo / 'setup.sh')
        (repo / 'zshrc').touch()
        (home / '.zshrc').write_text('original shell config\n')
        for tool in ('bar', 'launcher', 'tv-mode', 'hypr-swipe-events'):
            directory = repo / 'tools' / tool
            directory.mkdir(parents=True)
            if tool != missing_submodule:
                (directory / 'Cargo.toml').touch()
        for name in ('cargo', 'make', 'git', 'systemctl'):
            stub = stubs / name
            stub.write_text(stub_source)
            stub.chmod(0o755)
        env = dict(os.environ, HOME=str(home), PATH=f'{stubs}:/usr/bin:/bin',
                   INSTALL_SYSTEM_CONFIG='0', AUTO_ENABLE_SPOTIFY_SERVICE='0',
                   XDG_DATA_HOME=str(base / 'data'), CARGO_TARGET_DIR=str(base / 'foreign-target'),
                   SETUP_TEST_REPO=str(repo), SETUP_TEST_LOG=str(base / 'commands'),
                   SETUP_TEST_FAIL=failure)
        result = subprocess.run(['bash', str(repo / 'setup.sh')], env=env,
                                capture_output=True, text=True)
        commands = (base / 'commands').read_text().splitlines()
        if failure:
            assert result.returncode != 0, result.stdout
            assert (home / '.zshrc').read_text() == 'original shell config\n'
            assert not (home / '.config').exists()
            assert not list(home.glob('.zshrc.bak.*'))
            assert not any(line.startswith('systemctl ') for line in commands)
        else:
            assert result.returncode == 0, result.stderr
            for binary in ('vigil', 'Luma', 'luma-mail-eds', 'tv-mode'):
                assert (home / '.local/bin' / binary).is_file(), binary
            assert (base / 'data/hyprland/plugins/hypr-swipe-events.so').is_file()
            assert len([line for line in commands if line.startswith('cargo ')]) == 3
            assert any(line.startswith('git ') for line in commands) == bool(missing_submodule)
            assert len(list(home.glob('.zshrc.bak.*'))) == 1

check()
for submodule in ('bar', 'launcher'):
    check(missing_submodule=submodule)
for failure in ('bar', 'launcher', 'tv-mode', 'plugin'):
    check(failure=failure)
print('Setup builds every tool, initializes missing submodules, and preserves installation on build failure.')
PY
