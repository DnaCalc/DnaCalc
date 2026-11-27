"""Invoke the Excel worker's offline VBA decoder."""
import os
from pathlib import Path
import subprocess

try:
    from . import protocol
except ImportError:
    import protocol


def worker_path():
    configured = os.environ.get("TOUCHSTONE_VBA_POLICY_WORKER")
    if configured:
        return Path(configured)
    name = "excel-worker.exe" if os.name == "nt" else "excel-worker"
    built = Path(__file__).resolve().parents[1] / "engines" / "excel" / "bin" / name
    return built if built.is_file() else None


def check(group, worker=None):
    worker = worker or worker_path()
    if worker is None:
        raise ValueError("the Excel worker's VBA decoder is unavailable")
    try:
        result = subprocess.run([str(worker), "--check-vba-policy"], input=protocol.dumps(group),
                                capture_output=True, timeout=5)
    except (OSError, subprocess.TimeoutExpired) as error:
        raise ValueError("the Excel worker's VBA decoder is unavailable") from error
    if result.returncode != 0:
        raise ValueError(result.stderr.decode("utf-8", errors="replace").strip()[:500]
                         or "the Excel worker refused the VBA case")
    digest = result.stdout.strip().decode("ascii", errors="replace")
    if len(digest) != 64 or any(c not in "0123456789abcdef" for c in digest):
        raise ValueError("the Excel worker returned an invalid VBA case digest")
    return digest
