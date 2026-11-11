#!/usr/bin/env python3
"""Test curtain rendering in an isolated Hyprland; never use the live desktop.

Set GREETER_TEST_RUNTIME_DIR, GREETER_TEST_WAYLAND_DISPLAY and
GREETER_TEST_HYPRLAND_INSTANCE_SIGNATURE to the isolated compositor.
Requires grim and Pillow. Readiness queries are fixtures; surfaces are real GTK.
Start the isolated compositor with HYPRLAND_NO_SD_VARS=1,
HYPRLAND_NO_SD_TARGET=1 and HYPRLAND_NO_SD_NOTIFY=1 to isolate user systemd too.
"""
import io
import json
import os
from pathlib import Path
import select
import subprocess
import tempfile
import time
from PIL import Image

root = Path(__file__).resolve().parent.parent
required = ('GREETER_TEST_RUNTIME_DIR', 'GREETER_TEST_WAYLAND_DISPLAY',
            'GREETER_TEST_HYPRLAND_INSTANCE_SIGNATURE')
if not all(os.environ.get(key) for key in required):
    raise SystemExit('Explicit isolated compositor environment required; see module docstring')
env = dict(os.environ, XDG_RUNTIME_DIR=os.environ[required[0]],
           WAYLAND_DISPLAY=os.environ[required[1]],
           HYPRLAND_INSTANCE_SIGNATURE=os.environ[required[2]],
           GDK_BACKEND='wayland', GTK_A11Y='none')
env.pop('SHELL_SESSION_REDUCED_MOTION', None)

def ipc(*args):
    return subprocess.check_output(['/usr/bin/hyprctl', *args], env=env, text=True)

def curtains():
    return [layer for output in json.loads(ipc('-j', 'layers')).values()
            for level in output['levels'].values() for layer in level
            if layer['namespace'] == 'session-transition']

def screenshot(output):
    data = subprocess.check_output(['grim', '-o', output, '-'], env=env)
    return Image.open(io.BytesIO(data)).convert('RGB')

outputs = [monitor['name'] for monitor in json.loads(ipc('-j', 'monitors'))]
assert outputs
old_color = json.loads(ipc('-j', 'getoption', 'misc:background_color'))['int']
# A flat, nonblack background makes partial opacity measurable.
ipc('eval', 'hl.config({misc = {background_color = "rgb(406080)"}})')
try:
    with tempfile.TemporaryDirectory(prefix='session-transition-') as temp:
        temp = Path(temp)
        bin_dir = temp / 'bin'
        bin_dir.mkdir()
        signal = temp / 'ready'
        layers = {name: {'levels': {'0': [
            {'namespace': 'awww-daemon', 'alpha': 1, 'w': 100, 'h': 100},
            {'namespace': 'vigil', 'alpha': 1, 'w': 100, 'h': 44}]}} for name in outputs}
        (temp / 'layers').write_text(json.dumps(layers))
        (temp / 'images').write_text('\n'.join(
            f': {name}: 100x100, scale: 1, currently displaying: image: /test' for name in outputs))
        for name, fixture in [('hyprctl', 'layers'), ('awww', 'images')]:
            executable = bin_dir / name
            executable.write_text('#!/usr/bin/env python3\nfrom pathlib import Path\n'
                                  f'base = Path({str(temp)!r})\n'
                                  f'print((base / {fixture!r}).read_text() if (base / "ready").exists() else "")\n')
            executable.chmod(0o755)
        child_env = dict(env, PATH=f'{bin_dir}:{env["PATH"]}')
        binary = str(root / 'tools/greeter/target/release/shell-greeter')

        def launch(extra_env=None):
            return subprocess.Popen([binary, '--desktop-reveal', '--config', '/does/not/exist'],
                                    env=dict(child_env, **(extra_env or {})),
                                    stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)

        def covered(process):
            assert select.select([process.stdout], [], [], 2)[0], 'No first-frame handshake'
            assert process.stdout.readline() == 'ready\n'
            assert len(curtains()) == len(outputs), 'Not every output is covered'
            for output in outputs:
                image = screenshot(output)
                assert image.getpixel((20, 20)) == (0, 0, 0), 'Handshake preceded opaque frame'

        process = launch()
        try:
            covered(process)
            signal.touch()
            samples = []
            deadline = time.monotonic() + 3
            while process.poll() is None and time.monotonic() < deadline:
                image = screenshot(outputs[0])
                samples.append(image.getpixel((20, 20))[2])
                time.sleep(.025)
            _, error = process.communicate(timeout=1)
            assert process.returncode == 0 and not error, error
            assert any(2 < value < 125 for value in samples), f'No intermediate fade frames: {samples}'
            assert not curtains(), 'Ready desktop retained the curtain'
            print('Ready reveal: all outputs black before handshake, intermediate fade frames, clean exit.')
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()

        signal.unlink()
        started = time.monotonic()
        process = launch()
        try:
            covered(process)
            _, error = process.communicate(timeout=6)
            elapsed = time.monotonic() - started
            assert process.returncode == 0 and not error, error
            assert 4 <= elapsed < 6, elapsed
            assert not curtains(), 'Timeout retained the curtain'
            print(f'Missing wallpaper/bar: timeout released all surfaces after {elapsed:.2f}s.')
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()

        started = time.monotonic()
        process = launch({'SHELL_SESSION_REDUCED_MOTION': '1'})
        output, error = process.communicate(timeout=2)
        assert process.returncode == 0 and not output and not error, (output, error)
        assert not curtains()
        print(f'Reduced motion: no curtain or readiness wait ({time.monotonic() - started:.2f}s).')
finally:
    rgba = ((old_color & 0xffffff) << 8) | ((old_color >> 24) & 0xff)
    ipc('eval', f'hl.config({{misc = {{background_color = "rgba({rgba:08x})"}}}})')
