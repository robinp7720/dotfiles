#!/usr/bin/env python3
"""Run under Xvfb at 1600x900; exercise the real Qt/libmpv player with local HLS."""
import argparse
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlparse

REPO = Path(__file__).resolve().parents[3]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--binary", type=Path, default=REPO / "tools/tv-mode/target/debug/tv-mode")
parser.add_argument("--artifacts", type=Path, help="Keep screenshots of the synthetic clip")
args = parser.parse_args()


def run(*command, **kwargs):
    return subprocess.run(command, check=True, **kwargs)


def wait_for(predicate, timeout=12):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        result = predicate()
        if result:
            return result
        time.sleep(0.1)
    raise AssertionError("Timed out waiting for player state")


with tempfile.TemporaryDirectory(prefix="tv-native-smoke-") as directory:
    root = Path(directory)
    artifacts = args.artifacts or root
    artifacts.mkdir(parents=True, exist_ok=True)
    hls = root / "hls"
    hls.mkdir()
    run("ffmpeg", "-v", "error", "-f", "lavfi", "-i", "testsrc2=size=1728x720:rate=30",
        "-t", "60", "-c:v", "libx264", "-preset", "ultrafast", "-g", "60",
        "-f", "hls", "-hls_time", "2", "-hls_list_size", "0", "-hls_playlist_type", "vod",
        str(hls / "stream.m3u8"))
    run("ffmpeg", "-v", "error", "-f", "lavfi", "-i", "testsrc2=size=320x480:rate=1",
        "-frames:v", "1", str(root / "art.png"))
    requests = []
    state = {"revision": 1, "position": 4.0}
    movie = {"id": 1, "movieName": "Native HLS playback", "runtime": 1,
             "releaseDate": "2026-01-01", "Files": [{"id": 73, "name": "HLS test", "duration": 60}],
             "TrackMovies": [{"time": 4.0, "progress": 0.07}]}

    def descriptor():
        return {"sessionId": "fixture-session", "revision": state["revision"],
                "position": state["position"], "duration": 60, "method": "remux",
                "mediaUrl": f"/media/stream.m3u8?revision={state['revision']}",
                "tracks": [], "selectedTracks": {}, "chapters": [], "segments": []}

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *unused):
            pass

        def handle_request(self):
            path = urlparse(self.path).path
            body = json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))) or "{}")
            requests.append((self.command, path, body))
            status = 200
            content_type = "application/json"
            if path.endswith(("/poster", "/fanart")):
                data = (root / "art.png").read_bytes()
                content_type = "image/png"
            elif path.startswith("/media/"):
                file = hls / Path(path).name
                data = file.read_bytes()
                content_type = "application/vnd.apple.mpegurl" if file.suffix == ".m3u8" else "video/mp2t"
            else:
                if path == "/auth/login-options":
                    data = {"profilePicker": True, "users": [
                        {"id": 1, "name": "Test", "username": "test", "passwordless": True}]}
                elif path == "/auth/login":
                    data = {"accessToken": "fixture-only"}
                elif path == "/api/v1/me":
                    data = {"username": "test", "name": "Test", "preferences": {
                        "quality": "original", "subtitleMode": "off", "autoplayNext": False}}
                elif path == "/movies/list/movieName":
                    data = {"items": [movie], "pageInfo": {"nextCursor": None}}
                elif path == "/series/list/seriesName":
                    data = {"items": [], "pageInfo": {"nextCursor": None}}
                elif path == "/movie/1/info":
                    data = movie
                elif path == "/movies/watching":
                    data = [movie]
                elif path in ["/episodes/watching", "/episodes/next"]:
                    data = []
                elif path.endswith("/progress"):
                    state["position"] = body["position"]
                    data, status = None, 204
                elif path == "/playback/sessions" and self.command == "POST":
                    state.update(position=body.get("position", 0), revision=1)
                    data, status = descriptor(), 201
                elif path == "/playback/sessions/fixture-session" and self.command == "PATCH":
                    state.update(body)
                    state["revision"] += 1
                    data = descriptor()
                elif path == "/playback/sessions/fixture-session" and self.command == "DELETE":
                    data, status = None, 204
                else:
                    data, status = {}, 404
                data = b"" if status == 204 else json.dumps(data).encode()
            self.send_response(status)
            self.send_header("Content-Type", content_type)
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            try:
                self.wfile.write(data)
            except (BrokenPipeError, ConnectionResetError):
                pass

        do_GET = do_POST = do_PATCH = do_DELETE = handle_request

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    config = (REPO / "tv-mode/config.toml").read_text().replace(
        "https://oblecto.home.robindecker.me", f"http://127.0.0.1:{server.server_port}")
    (root / "config.toml").write_text(config)
    runtime = root / "runtime"
    runtime.mkdir(mode=0o700)
    env = os.environ | {"QT_QPA_PLATFORM": "xcb", "XDG_RUNTIME_DIR": str(runtime),
                        "XDG_STATE_HOME": str(root / "state"), "TV_MODE_SOFTWARE_VIDEO": "1",
                        "TV_MODE_REDUCED_MOTION": "1"}
    with (artifacts / "player.log").open("w") as log:
        app = subprocess.Popen([str(args.binary.resolve()), "--config", str(root / "config.toml"),
                                "preview", "--no-controller"], env=env, stdout=log, stderr=log)
        try:
            socket_path = wait_for(lambda: next(runtime.glob("tv-video-*/control.sock"), None))
            connection = socket.socket(socket.AF_UNIX)
            connection.settimeout(3)
            connection.connect(str(socket_path))
            reader = connection.makefile("r")

            def ipc(command):
                connection.sendall((json.dumps({"command": command, "request_id": 100}) + "\n").encode())
                while True:
                    response = json.loads(reader.readline())
                    if response.get("request_id") == 100:
                        return response.get("data")

            def prop(name):
                return ipc(["get_property", name])

            def key(name, delay=0.4):
                run("xdotool", "key", "--clearmodifiers", name)
                time.sleep(delay)

            def shot(name):
                run("import", "-window", "root", str(artifacts / f"{name}.png"))

            ipc(["set_property", "mute", True])
            time.sleep(1.5)
            key("Return")
            # Disable Remember me before using the fixture's passwordless profile.
            run("xdotool", "mousemove", "150", "575", "click", "1")
            time.sleep(0.2)
            run("xdotool", "mousemove", "215", "305", "click", "1")
            time.sleep(1)
            key("Return")
            key("Return")
            key("Return")
            wait_for(lambda: (prop("time-pos") or 0) > 5)
            assert prop("file-format") == "hls"
            key("space")
            wait_for(lambda: prop("pause") is True)
            before_seek = prop("time-pos")
            key("Right")
            wait_for(lambda: abs((prop("time-pos") or 0) - before_seek - 10) < 0.2)
            shot("paused-seek")
            # Sample across the picture to catch black frames, GL corruption and wrong depth.
            for channel, x in [("R", 100), ("G", 350), ("B", 850), ("B", 1400)]:
                color = run("magick", str(artifacts / "paused-seek.png"), "-crop", f"120x100+{x}+350",
                            "-channel", channel, "-format", "%[fx:mean]", "info:",
                            capture_output=True, text=True)
                assert float(color.stdout) > 0.65, "Decoded video did not render correctly"
            position = prop("time-pos")
            key("q")
            key("Down")
            key("Return")
            wait_for(lambda: any(r[0] == "PATCH" for r in requests))
            wait_for(lambda: abs((prop("time-pos") or 0) - position) < 0.2 and prop("pause") is True)
            shot("quality-changed")
            key("Home")
            shot("home-during-playback")
            assert not any(r[0] == "DELETE" for r in requests)
            key("Escape")
            shot("returned-to-playback")
            key("Escape")
            wait_for(lambda: any(r[0] == "DELETE" for r in requests))
            # Reuse the same engine, then exit with playback active.
            key("Return")
            wait_for(lambda: sum(r[0] == "POST" and r[1] == "/playback/sessions" for r in requests) == 2)
            wait_for(lambda: (prop("time-pos") or 0) > 4.5)
            app.terminate()
            app.wait(timeout=10)
            assert app.returncode == 0
            operations = [r for r in requests if r[1].startswith("/playback/")]
            creates = [r for r in operations if r[0] == "POST" and r[1] == "/playback/sessions"]
            assert all(r[2]["forceHls"] is True for r in creates)
            patch_index = next(i for i, r in enumerate(operations) if r[0] == "PATCH")
            old, patch = operations[patch_index - 1], operations[patch_index]
            assert old[1].endswith("/progress") and old[2]["paused"] is True
            assert old[2]["revision"] == patch[2]["revision"] == 1
            assert abs(old[2]["position"] - patch[2]["position"]) < 0.2
            deletes = [i for i, r in enumerate(operations) if r[0] == "DELETE"]
            assert len(deletes) == 2
            assert all(operations[i - 1][1].endswith("/progress") for i in deletes)
            assert operations[deletes[0] - 1][2]["revision"] == 2
            assert operations[deletes[0] - 1][2]["paused"] is True
            print("Native HLS playback, rendering, controls, engine reuse and shutdown passed")
        finally:
            if app.poll() is None:
                app.kill()
                app.wait()
            server.shutdown()
