#!/usr/bin/env python3
"""Build a JSON Lines usage ledger from saved coordinator sweeps."""

import argparse
from datetime import datetime
import json
from pathlib import Path
import re


SWEEP_NAME = re.compile(r"status-[0-9]{8}-[0-9]{4}\.md")
SWEEP_HEADER = "# DNA Calc sweep "
CLAUDE = "- Claude used (five-hour, weekly, Fable): "
GEMINI = "- Gemini remaining (weekly, five-hour, Claude/GPT group): "
RUNNING_LANE = re.compile(r"^- \*\*([^*]+)\*\* running\b")
MODEL = re.compile(r"\(usage [^,]+, model ([^)]+)\)")


def percent(value):
    result = float(value)
    if not 0 <= result <= 100:
        raise ValueError("percentage outside 0–100")
    return result


def percentages(line, prefix, keys):
    values = line.removeprefix(prefix).split(",")
    try:
        if len(values) != len(keys):
            raise ValueError("wrong number of percentages")
        return dict(zip(keys, map(percent, values)))
    except ValueError as error:
        raise ValueError(f"invalid {prefix.split()[1]} percentages") from error


def codex_reading(line, host):
    try:
        _, _, observed, used, reset, *_ = line.split()
        if not host or not observed.endswith("Z"):
            raise ValueError("missing host or UTC timestamp")
        timestamp = datetime.fromisoformat(observed[:-1] + "+00:00")
        return timestamp, {
            "host": host, "observed_utc": observed, "weekly_used_pct": percent(used),
            "reset_epoch": int(reset),
        }
    except ValueError as error:
        raise ValueError("invalid Codex reading") from error


def codex_account(readings):
    if not readings:
        return None
    latest_time = max(timestamp for timestamp, _ in readings)
    latest = [row for timestamp, row in readings if timestamp == latest_time]
    values = {(row["weekly_used_pct"], row["reset_epoch"]) for row in latest}
    return {
        "latest": latest[0] if len(values) == 1 else None,
        "readings": [row for _, row in readings],
    }


def parse_sweep(path, projects):
    lines = path.read_text(encoding="utf-8-sig").splitlines()
    if not lines or not lines[0].startswith(SWEEP_HEADER):
        raise ValueError("missing sweep heading")
    stamp = datetime.strptime(lines[0].removeprefix(SWEEP_HEADER), "%Y-%m-%d %H:%M")
    if path.name != stamp.strftime("status-%Y%m%d-%H%M.md"):
        raise ValueError("filename and sweep heading timestamps differ")

    host = None
    claude = gemini = None
    codex, lanes = [], []
    for line in lines[1:]:
        if line.startswith("## "):
            host = line[3:]
        elif host in (None, "Attention"):
            continue
        elif line.startswith("- CODEX "):
            codex.append(codex_reading(line, host))
        elif line.startswith(CLAUDE) and line != CLAUDE + "unread":
            used = percentages(line, CLAUDE, ("five_hour", "weekly", "fable"))
            claude = {"host": host, "used_pct": used}
        elif line.startswith(GEMINI) and line != GEMINI + "unread":
            remaining = percentages(
                line, GEMINI, ("weekly", "five_hour", "claude_gpt_group")
            )
            gemini = {
                "host": host,
                "remaining_pct": remaining,
                "used_pct": {key: round(100 - value, 10) for key, value in remaining.items()},
            }
        elif match := RUNNING_LANE.match(line):
            folder = f"{host}/{match[1]}"
            model = MODEL.search(line.partition("; loop:")[2].partition("; status:")[0])
            lanes.append({
                "folder": folder, "project": projects.get(folder),
                "model": model[1] if model else None,
            })

    return {
        "source": path.name,
        "sweep_local": stamp.isoformat(timespec="minutes"),
        "claude": claude,
        "codex": codex_account(codex),
        "gemini": gemini,
        "running_lanes": lanes,
    }


def load_projects(path):
    if path is None:
        return {}
    projects = json.loads(path.read_text(encoding="utf-8-sig"))
    if not isinstance(projects, dict) or any(
        len(folder.split("/")) != 2 or not all(folder.split("/"))
        or not isinstance(project, str) or not project.strip()
        for folder, project in projects.items()
    ):
        raise ValueError("projects must map host/folder keys to nonempty project names")
    return projects


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("sweeps", type=Path, help="directory of copied status-*.md files")
    parser.add_argument("--projects", type=Path, help="JSON map of host/folder to project")
    args = parser.parse_args()

    try:
        projects = load_projects(args.projects)
        paths = sorted(
            path for path in args.sweeps.iterdir()
            if SWEEP_NAME.fullmatch(path.name) and not path.is_symlink() and path.is_file()
        )
        if not paths:
            raise ValueError("no status-YYYYMMDD-HHMM.md sweeps found")
        rows = []
        for path in paths:
            try:
                rows.append(parse_sweep(path, projects))
            except (OSError, ValueError) as error:
                raise ValueError(f"{path.name}: {error}") from error
    except (OSError, ValueError) as error:
        parser.exit(1, f"{error}\n")

    for row in rows:
        print(json.dumps(row, allow_nan=False))


if __name__ == "__main__":
    main()
