#!/usr/bin/env python3
import subprocess, json, datetime, sys

MAX_ITEMS = 5

def day_label(date_str):
    today = datetime.date.today()
    try:
        d = datetime.datetime.strptime(date_str, "%Y-%m-%d").date()
    except ValueError:
        return date_str
    if d == today:
        return "Today"
    if d == today + datetime.timedelta(days=1):
        return "Tomorrow"
    return d.strftime("%a %b %-d") if sys.platform != "win32" else d.strftime("%a %b %d").lstrip('0')

def eta_string(start_dt):
    now = datetime.datetime.now()
    diff = (start_dt - now).total_seconds()
    if diff <= 0:
        return "Now"
    mins = int((diff + 59) // 60)
    if mins >= 90:
        hours, rem = divmod(mins, 60)
        return f"{hours}h" if rem == 0 else f"{hours}h {rem}m"
    return f"{mins}m"

def parse_khal():
    try:
        res = subprocess.run(["khal", "list", "now", "2d"], capture_output=True, text=True, check=False)
    except FileNotFoundError:
        return []
    if res.returncode != 0 or not res.stdout.strip():
        return []
    events = []
    for line in res.stdout.splitlines():
        parts = line.split(None, 2)
        if len(parts) < 3:
            continue
        date_str, span, summary = parts
        if "-" not in span:
            continue
        start_time, end_time = span.split("-", 1)
        try:
            start_dt = datetime.datetime.fromisoformat(f"{date_str} {start_time}")
        except ValueError:
            continue
        events.append({
            "summary": summary.strip(),
            "time_span": f"{day_label(date_str)}  {start_time} – {end_time}",
            "eta": eta_string(start_dt),
            "location": "",
        })
        if len(events) >= MAX_ITEMS:
            break
    return events

def parse_gcalcli():
    try:
        res = subprocess.run([
            "gcalcli", "--nocolor", "agenda", "--tsv", "--details=location", "now", "2d"
        ], capture_output=True, text=True, check=False)
    except FileNotFoundError:
        return []
    if res.returncode != 0 or not res.stdout.strip():
        return []
    events = []
    lines = res.stdout.splitlines()[1:]  # skip header
    for line in lines:
        parts = line.split('\t')
        if len(parts) < 5:
            continue
        start_date, start_time, end_date, end_time, summary = parts[:5]
        location = parts[5] if len(parts) > 5 else ""
        try:
            start_dt = datetime.datetime.fromisoformat(f"{start_date} {start_time}")
        except ValueError:
            start_dt = None
        events.append({
            "summary": summary.strip(),
            "time_span": f"{day_label(start_date)}  {start_time} – {end_time}",
            "eta": eta_string(start_dt) if start_dt else "",
            "location": location.strip(),
        })
        if len(events) >= MAX_ITEMS:
            break
    return events

def main():
    events = parse_khal()
    if not events:
        events = parse_gcalcli()
    if not events:
        print(json.dumps([{"summary":"No upcoming events","time_span":"","eta":"","location":""}]))
        return
    print(json.dumps(events[:MAX_ITEMS]))

if __name__ == "__main__":
    main()
