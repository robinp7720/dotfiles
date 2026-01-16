#!/usr/bin/env python3
import json
import re
import subprocess
import sys

def main() -> int:
    try:
        output = subprocess.check_output(["niri", "msg", "keyboard-layouts"], text=True)
    except Exception:
        print(json.dumps({"text": "??", "tooltip": "niri not running"}))
        return 0

    layouts = []
    current = None
    for line in output.splitlines():
        match = re.match(r"\s*([* ])\s*(\d+)\s+(.*)", line)
        if not match:
            continue
        name = match.group(3)
        layouts.append(name)
        if match.group(1) == "*":
            current = name

    if current is None and layouts:
        current = layouts[0]

    text = "??"
    if current:
        if re.search(r"dvorak", current, re.IGNORECASE):
            text = "DVORAK"
        else:
            text = "QWERTY"

    tooltip = "\n".join(layouts) if layouts else "No layouts"

    print(json.dumps({"text": text, "tooltip": tooltip}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
