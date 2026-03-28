#!/usr/bin/env python3
import json
import os
import re
import subprocess
import sys
from typing import Any


def run_command(command: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        command,
        capture_output=True,
        text=True,
        check=False,
    )


def load_json_output(command: list[str]) -> Any | None:
    result = run_command(command)
    if result.returncode != 0 or not result.stdout.strip():
        return None

    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError:
        return None


def detect_backend() -> str | None:
    if os.environ.get("NIRI_SOCKET"):
        return "niri"
    if os.environ.get("HYPRLAND_INSTANCE_SIGNATURE"):
        return "hyprland"

    if run_command(["pgrep", "-x", "niri"]).returncode == 0:
        return "niri"
    if run_command(["pgrep", "-x", "Hyprland"]).returncode == 0:
        return "hyprland"

    return None


def format_label(layout_name: str) -> str:
    normalized = layout_name.strip().lower()

    if "dvorak" in normalized:
        return "DVORAK"
    if "colemak" in normalized:
        return "COLEMAK"
    if "workman" in normalized:
        return "WORKMAN"
    if normalized in {"english (us)", "us", "qwerty"} or "qwerty" in normalized:
        return "US"

    match = re.search(r"\(([^)]+)\)", layout_name)
    if match:
        candidate = match.group(1).strip()
        if candidate:
            return candidate.upper()

    words = re.findall(r"[A-Za-z0-9]+", layout_name)
    if not words:
        return "??"

    return words[-1].upper()[:8]


def get_niri_layout_state() -> dict[str, Any]:
    result = run_command(["niri", "msg", "keyboard-layouts"])
    if result.returncode != 0:
        raise RuntimeError("niri is not running")

    layouts: list[str] = []
    current = None
    for line in result.stdout.splitlines():
        match = re.match(r"\s*([* ])\s*(\d+)\s+(.*)", line)
        if not match:
            continue
        name = match.group(3).strip()
        layouts.append(name)
        if match.group(1) == "*":
            current = name

    if current is None and layouts:
        current = layouts[0]

    if current is None:
        raise RuntimeError("no keyboard layouts configured")

    tooltip = "\n".join(layouts) if layouts else current
    return {
        "text": format_label(current),
        "tooltip": tooltip,
        "class": "niri",
        "alt": current,
    }


def get_hyprland_layout_state() -> dict[str, Any]:
    devices = load_json_output(["hyprctl", "devices", "-j"])
    if not isinstance(devices, dict):
        raise RuntimeError("hyprctl is not available")

    keyboards = devices.get("keyboards")
    if not isinstance(keyboards, list) or not keyboards:
        raise RuntimeError("no keyboards found")

    current = None
    tooltips: list[str] = []
    for keyboard in keyboards:
        if not isinstance(keyboard, dict):
            continue

        name = str(keyboard.get("name", "")).strip()
        active_keymap = str(keyboard.get("active_keymap", "")).strip()
        if not active_keymap:
            continue

        if keyboard.get("main") and current is None:
            current = active_keymap
        elif current is None:
            current = active_keymap

        if name:
            tooltips.append(f"{name}: {active_keymap}")

    if current is None:
        raise RuntimeError("no active keymap found")

    tooltip = "\n".join(tooltips) if tooltips else current
    return {
        "text": format_label(current),
        "tooltip": tooltip,
        "class": "hyprland",
        "alt": current,
    }


def toggle_niri_layout() -> int:
    return run_command(["niri", "msg", "action", "switch-layout", "next"]).returncode


def toggle_hyprland_layout() -> int:
    devices = load_json_output(["hyprctl", "devices", "-j"])
    if not isinstance(devices, dict):
        return 1

    keyboards = devices.get("keyboards")
    if not isinstance(keyboards, list):
        return 1

    keyboard_names = [
        str(keyboard.get("name", "")).strip()
        for keyboard in keyboards
        if isinstance(keyboard, dict) and keyboard.get("name")
    ]
    if not keyboard_names:
        return 1

    exit_code = 0
    for keyboard_name in keyboard_names:
        result = run_command(["hyprctl", "switchxkblayout", keyboard_name, "next"])
        if result.returncode != 0:
            exit_code = result.returncode

    return exit_code


def main() -> int:
    backend = detect_backend()
    toggle = len(sys.argv) > 1 and sys.argv[1] == "--toggle"

    if toggle:
        if backend == "niri":
            return toggle_niri_layout()
        if backend == "hyprland":
            return toggle_hyprland_layout()
        return 0

    try:
        if backend == "niri":
            state = get_niri_layout_state()
        elif backend == "hyprland":
            state = get_hyprland_layout_state()
        else:
            state = {"text": "", "tooltip": "No supported compositor detected", "class": "hidden"}
    except RuntimeError as exc:
        state = {"text": "", "tooltip": str(exc), "class": "hidden"}

    print(json.dumps(state))
    return 0


if __name__ == "__main__":
    sys.exit(main())
