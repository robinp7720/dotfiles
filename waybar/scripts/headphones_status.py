#!/usr/bin/env python3

import json
import os
import subprocess
from typing import List, Tuple


def run_command(command: List[str]) -> subprocess.CompletedProcess:
    return subprocess.run(
        command,
        capture_output=True,
        text=True,
        check=False,
    )


def parse_devices(output: str) -> List[Tuple[str, str]]:
    devices = []
    for line in output.strip().splitlines():
        parts = line.split(" ", 2)
        if len(parts) == 3:
            devices.append((parts[1], parts[2]))
    return devices


def bluetooth_info(mac: str) -> subprocess.CompletedProcess:
    return run_command(["bluetoothctl", "info", mac])


def main() -> None:
    alias = os.environ.get("HEADPHONES_ALIAS", "").strip()
    mac = os.environ.get("HEADPHONES_MAC", "").strip()

    try:
        connected_result = run_command(["bluetoothctl", "devices", "Connected"])
    except FileNotFoundError:
        print(
            json.dumps(
                {
                    "text": "",
                    "class": "disconnected",
                    "tooltip": "bluetoothctl not found",
                    "alt": "Headphones",
                }
            )
        )
        return

    connected_devices = parse_devices(connected_result.stdout)

    target = None
    label = alias or mac or "Headphones"

    if mac:
        for addr, name in connected_devices:
            if addr.lower() == mac.lower():
                target = (addr, name)
                label = name or mac
                break
        if target is None:
            info = bluetooth_info(mac).stdout
            if "Connected: yes" in info:
                for line in info.splitlines():
                    stripped = line.strip()
                    if stripped.startswith("Alias:"):
                        label = stripped.split("Alias:", 1)[1].strip() or label
                        break
                target = (mac, label)
    elif alias:
        for addr, name in connected_devices:
            if alias.lower() in name.lower():
                target = (addr, name)
                label = name
                break
        if target is None:
            paired_devices = parse_devices(
                run_command(["bluetoothctl", "devices", "Paired"]).stdout
            )
            for addr, name in paired_devices:
                if alias.lower() in name.lower():
                    info = bluetooth_info(addr).stdout
                    if "Connected: yes" in info:
                        for line in info.splitlines():
                            stripped = line.strip()
                            if stripped.startswith("Alias:"):
                                label = stripped.split("Alias:", 1)[1].strip() or name
                                break
                        target = (addr, label)
                        break
    else:
        if connected_devices:
            target = connected_devices[0]
            label = target[1]

    if target:
        output = {
            "text": "",
            "class": "connected",
            "tooltip": f"{label} connected",
            "alt": label,
        }
    else:
        output = {
            "text": "",
            "class": "disconnected",
            "tooltip": f"{label} not connected",
            "alt": label,
        }

    print(json.dumps(output))


if __name__ == "__main__":
    main()
