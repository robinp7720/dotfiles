#!/usr/bin/env python3
"""Print the plugin build ABI, refusing mismatched headers and executable."""

import json
from pathlib import Path
import re
import shlex
import subprocess

flags = shlex.split(subprocess.check_output(
    ["pkg-config", "--cflags-only-I", "hyprland"], text=True
))
header_path = next((Path(flag[2:], "src/version.h") for flag in flags
                    if Path(flag[2:], "src/version.h").is_file()), None)
if header_path is None:
    raise SystemExit("Cannot find Hyprland version.h through pkg-config")
header = header_path.read_text()
defines = dict(re.findall(r'^#define\s+(\w+)\s+"([^"]+)"', header, re.MULTILINE))
abi = defines["GIT_COMMIT_HASH"]
for key, name in (
    ("aq", "AQUAMARINE"), ("hu", "HYPRUTILS"), ("hg", "HYPRGRAPHICS"),
    ("hc", "HYPRCURSOR"), ("hlg", "HYPRLANG"),
):
    abi += f"_{key}_{defines[name + '_VERSION'].rsplit('.', 1)[0]}"
installed = json.loads(subprocess.check_output(["Hyprland", "--version-json"], text=True))
if abi != installed["abiHash"]:
    raise SystemExit("Hyprland headers do not match the installed executable; update them before building plugins")
print(abi)
