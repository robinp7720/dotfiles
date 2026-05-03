#!/usr/bin/env python3

import json
from pathlib import Path


POWER_SUPPLY = Path("/sys/class/power_supply")

BATTERY_ICONS = [
    "󰁺",
    "󰁻",
    "󰁼",
    "󰁽",
    "󰁾",
    "󰁿",
    "󰂀",
    "󰂁",
    "󰂂",
    "󰁹",
]

CHARGING_ICONS = [
    "󰢜",
    "󰂆",
    "󰂇",
    "󰂈",
    "󰢝",
    "󰂉",
    "󰢞",
    "󰂊",
    "󰂋",
    "󰂅",
]


def read_text(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8").strip()
    except OSError:
        return ""


def read_int(path: Path) -> int | None:
    value = read_text(path)
    try:
        return int(value)
    except ValueError:
        return None


def battery_paths() -> list[Path]:
    if not POWER_SUPPLY.exists():
        return []

    paths = []
    for path in POWER_SUPPLY.iterdir():
        if read_text(path / "type").lower() == "battery":
            paths.append(path)
    return sorted(paths)


def weighted_capacity(paths: list[Path]) -> int | None:
    current_total = 0
    full_total = 0

    for path in paths:
        current = read_int(path / "energy_now")
        full = read_int(path / "energy_full")
        if current is None or full is None:
            current = read_int(path / "charge_now")
            full = read_int(path / "charge_full")

        if current is None or full in (None, 0):
            continue

        current_total += current
        full_total += full

    if full_total > 0:
        return max(0, min(100, round(current_total * 100 / full_total)))

    capacities = [read_int(path / "capacity") for path in paths]
    capacities = [value for value in capacities if value is not None]
    if capacities:
        return max(0, min(100, round(sum(capacities) / len(capacities))))

    return None


def pick_icon(capacity: int, charging: bool) -> str:
    icons = CHARGING_ICONS if charging else BATTERY_ICONS
    index = min(len(icons) - 1, max(0, capacity // 10))
    return icons[index]


def main() -> None:
    paths = battery_paths()
    if not paths:
        print(json.dumps({"text": "", "tooltip": "", "class": "hidden"}))
        return

    capacity = weighted_capacity(paths)
    if capacity is None:
        print(json.dumps({"text": "", "tooltip": "Battery status unavailable", "class": "hidden"}))
        return

    statuses = [read_text(path / "status") for path in paths]
    charging = any(status in {"Charging", "Full"} for status in statuses)

    classes = ["charging" if charging else "discharging"]
    if not charging and capacity <= 10:
        classes.append("critical")
    elif not charging and capacity <= 20:
        classes.append("warning")

    tooltip_lines = []
    for path, status in zip(paths, statuses):
        name = read_text(path / "model_name") or path.name
        item_capacity = read_int(path / "capacity")
        if item_capacity is None:
            tooltip_lines.append(f"{name}: {status or 'Unknown'}")
        else:
            tooltip_lines.append(f"{name}: {item_capacity}% {status or 'Unknown'}")

    output = {
        "text": f"{capacity}% {pick_icon(capacity, charging)}",
        "tooltip": "\n".join(tooltip_lines),
        "class": classes,
        "percentage": capacity,
    }
    print(json.dumps(output))


if __name__ == "__main__":
    main()
