#!/usr/bin/env python3
"""Exercise the real GTK client in an explicitly supplied isolated compositor.

GREETER_TEST_RUNTIME_DIR=/path/to/private/runtime \
GREETER_TEST_WAYLAND_DISPLAY=wayland-1 python3 tests/greeter_wayland_smoke.py

No PAM or session commands run: the only authentication peer is a fake socket.
"""
from pathlib import Path
import json
import os
import socket
import struct
import subprocess
import tempfile
import time

root = Path(__file__).resolve().parent.parent
runtime = os.environ.get('GREETER_TEST_RUNTIME_DIR')
display = os.environ.get('GREETER_TEST_WAYLAND_DISPLAY')
if not runtime or not display:
    raise SystemExit('Explicit isolated GREETER_TEST_RUNTIME_DIR and GREETER_TEST_WAYLAND_DISPLAY required')

with tempfile.TemporaryDirectory(prefix='greeter-protocol-') as directory:
    base = Path(directory)
    sessions = base / 'wayland-sessions'
    sessions.mkdir()
    (sessions / 'test.desktop').write_text(
        '[Desktop Entry]\nName=Protocol test\nExec=/bin/true "two words"\nDesktopNames=Test;\n')
    config = base / 'config.toml'
    config.write_text(
        'preferred_output = "missing-test-output"\n'
        f'state = {json.dumps(str(base / "state.json"))}\n'
        f'session_dirs = [{json.dumps(str(sessions))}]\n'
        f'wallpaper = {json.dumps(str(base / "absent-image"))}\n'
        f'theme = {json.dumps(str(base / "absent-theme"))}\n')
    sockpath = base / 'greetd.sock'
    server = socket.socket(socket.AF_UNIX)
    server.bind(str(sockpath))
    server.listen()
    server.settimeout(10)
    env = dict(os.environ, XDG_RUNTIME_DIR=runtime, WAYLAND_DISPLAY=display,
               GDK_BACKEND='wayland', GTK_A11Y='none', GREETD_SOCK=str(sockpath))

    def receive(conn):
        def exact(length):
            data = b''
            while len(data) < length:
                chunk = conn.recv(length - len(data))
                assert chunk, 'Client disconnected unexpectedly'
                data += chunk
            return data
        size = struct.unpack('=I', exact(4))[0]
        return json.loads(exact(size))

    def send(conn, value):
        data = json.dumps(value).encode()
        conn.sendall(struct.pack('=I', len(data)) + data)

    def key(*args):
        time.sleep(.2)
        subprocess.run(['wtype', *args], env=env, check=True)

    with (base / 'client.log').open('w') as log:
        process = subprocess.Popen(
            [str(root / 'tools/greeter/target/release/shell-greeter'), '--config', str(config)],
            env=env, stdout=log, stderr=log)
        try:
            conn, _ = server.accept()
            with conn:
                conn.settimeout(10)
                assert receive(conn)['type'] == 'create_session'
                send(conn, {'type': 'auth_message', 'auth_message_type': 'secret',
                            'auth_message': 'Password:'})
                key('test-only-password')
                key('-k', 'Return')
                assert receive(conn) == {'type': 'post_auth_message_response',
                                         'response': 'test-only-password'}
                send(conn, {'type': 'auth_message', 'auth_message_type': 'visible',
                            'auth_message': 'Verification code:'})
                key('123456')
                key('-k', 'Return')
                assert receive(conn) == {'type': 'post_auth_message_response', 'response': '123456'}
                send(conn, {'type': 'success'})
                launch = receive(conn)
                assert launch['type'] == 'start_session'
                assert launch['cmd'] == ['/bin/true', 'two words']
                assert 'XDG_SESSION_TYPE=wayland' in launch['env']
                assert 'XDG_CURRENT_DESKTOP=Test' in launch['env']
                assert not any(e.startswith(('DISPLAY=', 'WAYLAND_DISPLAY=')) for e in launch['env'])
                time.sleep(.2)
                assert process.poll() is None, 'Client exited before launch acknowledgement'
                assert not (base / 'state.json').exists(), 'State persisted before acknowledgement'
                send(conn, {'type': 'success'})
                assert process.wait(timeout=10) == 0
                state = json.loads((base / 'state.json').read_text())
                assert state['sessions'][state['user']] == 'wayland:test.desktop'
                assert 'test-only-password' not in (base / 'state.json').read_text()
            assert not (base / 'client.log').read_text(), (base / 'client.log').read_text()
            print('GTK/greetd integration: prompts, argv, environment, acknowledgement and persistence passed.')
        finally:
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=5)
            server.close()
