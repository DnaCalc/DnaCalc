"""Compact logs for the broker: one gzip record per batch, the environment list, the waits log, stats."""
from datetime import datetime, timezone, timedelta
import gzip
import hashlib
import json
import os
from pathlib import Path
import statistics
import threading


def utc():
    return datetime.now(timezone.utc)


def compact(obj):
    return json.dumps(obj, ensure_ascii=True, separators=(",", ":"), allow_nan=False)


class BatchLog:
    """log/YYYY-MM-DD.jsonl.gz: each record is its own gzip member, fsynced, so a crash loses nothing earlier."""

    def __init__(self, directory):
        self.directory = Path(directory)
        self.directory.mkdir(parents=True, exist_ok=True)
        self.lock = threading.Lock()

    def write(self, record):
        now = utc()
        record = {"utc": now.isoformat(timespec="milliseconds"), **record}
        data = gzip.compress((compact(record) + "\n").encode("ascii"), compresslevel=6)
        path = self.directory / (now.strftime("%Y-%m-%d") + ".jsonl.gz")
        with self.lock, path.open("ab") as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())

    @staticmethod
    def read(directory, since=None):
        for path in sorted(Path(directory).glob("*.jsonl.gz")):
            if since and path.stem.split(".")[0] < since.strftime("%Y-%m-%d"):
                continue
            with gzip.open(path, "rt", encoding="ascii") as stream:
                for line in stream:
                    record = json.loads(line)
                    if since and record["utc"] < since.isoformat():
                        continue
                    yield record


class EnvLog:
    """envs.jsonl: one entry per distinct environment; env_id is the hash of the entry."""

    def __init__(self, path):
        self.path = Path(path)
        self.known = {}
        if self.path.exists():
            for line in self.path.read_text(encoding="ascii").splitlines():
                entry = json.loads(line)
                self.known[entry["env_id"]] = entry

    def add(self, env_id, env, extra=None):
        if env_id in self.known:
            return False
        entry = {"env_id": env_id, "first_seen_utc": utc().isoformat(timespec="seconds"), "env": env, **(extra or {})}
        self.known[env_id] = entry
        with self.path.open("a", encoding="ascii") as stream:
            stream.write(compact(entry) + "\n")
            stream.flush()
            os.fsync(stream.fileno())
        return True


WAIT_KINDS = ("transport", "queue", "excel-calc", "excel-start", "refused-kind", "job-route", "client-timeout",
              "worker-down")


class WaitsLog:
    """log/waits.jsonl: every request over its budget (1 s + 10 us per call), queueing over 250 ms, refusals
    of unsupported kinds, job-route use, workers down, and waits the lanes report."""

    def __init__(self, path):
        self.path = Path(path)
        self.lock = threading.Lock()

    def add(self, kind, **data):
        if kind not in WAIT_KINDS:
            raise ValueError(kind)
        line = compact({"utc": utc().isoformat(timespec="milliseconds"), "kind": kind, **data})
        with self.lock, self.path.open("a", encoding="ascii") as stream:
            stream.write(line + "\n")
            stream.flush()

    @staticmethod
    def summary(path, since=None):
        counts, seconds = {}, {}
        path = Path(path)
        if not path.exists():
            return {}
        for line in path.read_text(encoding="ascii").splitlines():
            record = json.loads(line)
            if since and record["utc"] < since.isoformat():
                continue
            kind = record["kind"]
            counts[kind] = counts.get(kind, 0) + 1
            seconds[kind] = seconds.get(kind, 0.0) + record.get("seconds", 0.0)
        return {k: {"count": counts[k], "seconds": round(seconds[k], 3)} for k in sorted(counts)}


def percentile(values, fraction):
    if not values:
        return None
    values = sorted(values)
    return values[min(len(values) - 1, int(len(values) * fraction))]


def stats(directory, since=None):
    batches = 0
    calls = 0
    cells = 0
    by_worker = {}
    ms = []
    waits = []
    seconds_busy = {}
    functions = {}
    first = last = None
    for record in BatchLog.read(directory, since):
        if record.get("kind") != "batch":
            continue
        batches += 1
        calls += record["calls"]
        cells += record.get("cells", 0)
        worker = record["worker"]
        entry = by_worker.setdefault(worker, {"batches": 0, "calls": 0, "busy_ms": 0.0})
        entry["batches"] += 1
        entry["calls"] += record["calls"]
        entry["busy_ms"] += record["ms"]["total"]
        ms.append(record["ms"]["total"])
        waits.append(record["ms"]["queued"])
        for f, n in record["fns"].items():
            functions[f] = functions.get(f, 0) + n
        first = first or record["utc"]
        last = record["utc"]
    for entry in by_worker.values():
        entry["calls_per_busy_second"] = round(entry["calls"] / (entry["busy_ms"] / 1000), 1) if entry["busy_ms"] else None
        entry["busy_ms"] = round(entry["busy_ms"], 1)
    return {"batches": batches, "calls": calls, "cells": cells, "first_utc": first, "last_utc": last,
            "batch_ms_p50": percentile(ms, .5), "batch_ms_p99": percentile(ms, .99),
            "queue_ms_p50": percentile(waits, .5), "queue_ms_p99": percentile(waits, .99),
            "workers": by_worker, "top_functions": dict(sorted(functions.items(), key=lambda kv: -kv[1])[:10])}


def env_hash(env):
    return hashlib.sha256(compact(env).encode("ascii")).hexdigest()[:12]
