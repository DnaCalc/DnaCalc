"""Engine worker processes (Windows): the C# Excel worker and identity-checked termination."""
import ctypes
import ctypes.wintypes as wt
import json
import os
from pathlib import Path
import struct
import subprocess
import threading
import time

HERE = Path(__file__).resolve().parent
WORKER_EXE = HERE.parent / "engines" / "excel" / "bin" / "excel-worker.exe"
FILETIME_TO_TICKS = 504911232000000000   # 1601-01-01 -> 0001-01-01 in 100 ns ticks
PROCESS_TERMINATE = 0x0001
PROCESS_QUERY_LIMITED_INFORMATION = 0x1000
PROTECTED_PIDS = {3144}


def process_identity(pid):
    """(image name, start ticks) of a process, or None."""
    k = ctypes.windll.kernel32
    k.OpenProcess.restype = wt.HANDLE
    handle = k.OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, False, pid)
    if not handle:
        return None
    try:
        created, exited, kernel, user = (wt.FILETIME() for _ in range(4))
        if not k.GetProcessTimes(handle, ctypes.byref(created), ctypes.byref(exited), ctypes.byref(kernel), ctypes.byref(user)):
            return None
        ticks = ((created.dwHighDateTime << 32) | created.dwLowDateTime) + FILETIME_TO_TICKS
        size = wt.DWORD(1024)
        buffer = ctypes.create_unicode_buffer(1024)
        if not k.QueryFullProcessImageNameW(handle, 0, buffer, ctypes.byref(size)):
            return None
        return os.path.basename(buffer.value).lower(), ticks
    finally:
        k.CloseHandle(handle)


def kill_owned_excel(pid, start_ticks):
    """Terminate an Excel this service started: refused unless PID, image and start ticks all match."""
    if not pid or pid in PROTECTED_PIDS:
        return "no owned Excel"
    identity = process_identity(pid)
    if identity is None:
        return "already exited"
    if identity != ("excel.exe", start_ticks):
        return "identity changed; termination refused"
    k = ctypes.windll.kernel32
    k.OpenProcess.restype = wt.HANDLE
    handle = k.OpenProcess(PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION, False, pid)
    if not handle:
        return "open failed"
    try:
        # Re-check on the pinned handle's process: the PID cannot be reused while the handle is open.
        if process_identity(pid) != ("excel.exe", start_ticks):
            return "identity changed; termination refused"
        k.TerminateProcess(handle, 1)
        k.WaitForSingleObject(handle, 5000)
        return "owned Excel terminated"
    finally:
        k.CloseHandle(handle)


class WorkerError(RuntimeError):
    pass


class WorkerTimeout(WorkerError):
    pass


class WorkerProcess:
    """One C# worker (one owned Excel). Synchronous; callers serialise access (one batch in flight)."""

    def __init__(self, root, name, mt=0, exe=WORKER_EXE, startup_seconds=60, log_dir=None):
        self.root, self.name, self.mt = Path(root), name, mt
        self.exe = exe if isinstance(exe, (list, tuple)) else Path(exe)
        self.startup_seconds = startup_seconds
        self.process = None
        self.hello = None
        self.events = []
        self.log_dir = Path(log_dir) if log_dir else self.root / "worker-logs"
        self.lock = threading.Lock()

    # framing -----------------------------------------------------------------------
    def _read_exact(self, count):
        data = bytearray()
        while len(data) < count:
            chunk = self.process.stdout.read(count - len(data))
            if not chunk:
                raise WorkerError("worker closed its pipe")
            data += chunk
        return bytes(data)

    def _read_frame(self):
        (size,) = struct.unpack(">I", self._read_exact(4))
        body = self._read_exact(size)
        return chr(body[0]), json.loads(body[1:])

    def _send(self, kind, obj):
        body = json.dumps(obj, ensure_ascii=True, separators=(",", ":")).encode("ascii")
        self.process.stdin.write(struct.pack(">I", len(body) + 1) + kind.encode("ascii") + body)
        self.process.stdin.flush()

    # lifecycle ---------------------------------------------------------------------
    def start(self):
        self.root.mkdir(parents=True, exist_ok=True)
        self.log_dir.mkdir(parents=True, exist_ok=True)
        err = open(self.log_dir / f"{self.name}.stderr", "ab")
        self.process = subprocess.Popen(
            [*(self.exe if isinstance(self.exe, (list, tuple)) else [str(self.exe)]), "--root", str(self.root), "--name", self.name, "--mt", str(self.mt),
             "--startup-seconds", str(self.startup_seconds)],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=err, bufsize=0,
            creationflags=subprocess.CREATE_NO_WINDOW)
        err.close()
        self.hello = self._expect_hello(self.startup_seconds + 30)
        return self.hello

    def _expect_hello(self, timeout):
        result = {}

        def read():
            try:
                while True:
                    kind, body = self._read_frame()
                    if kind == "H":
                        result["hello"] = body
                        return
                    if kind == "X" and body.get("fatal"):    # other X frames are events (idle drift); keep waiting
                        result["error"] = body
                        return
            except Exception as error:
                result["error"] = {"fatal": "worker-pipe", "detail": str(error)}

        thread = threading.Thread(target=read, daemon=True)
        thread.start()
        thread.join(timeout)
        if thread.is_alive():
            self.kill()
            raise WorkerError("worker did not report within the startup time")
        if "hello" not in result:
            self.kill()
            raise WorkerError(json.dumps(result["error"]))
        return result["hello"]

    def alive(self):
        return self.process is not None and self.process.poll() is None

    def kill(self):
        """Stop this worker and the one Excel it owns, nothing else."""
        outcome = None
        hello = self.hello
        if hello:
            outcome = kill_owned_excel(hello["pid"], hello["start_ticks"])
        if self.process is not None and self.process.poll() is None:
            self.process.kill()   # our own worker process
            try:
                self.process.wait(10)
            except Exception:
                pass
        return outcome

    def stop(self):
        if self.alive():
            try:
                self._send("Q", {})
                self.process.wait(15)
            except Exception:
                pass
        self.kill()

    # work --------------------------------------------------------------------------
    def run(self, message, timeout):
        """Send one batch; return the result frame. On a missed deadline kill the owned Excel and raise."""
        with self.lock:
            self._send("B", message)
            outcome = {}

            def read():
                try:
                    while True:
                        kind, body = self._read_frame()
                        if kind == "R":
                            outcome["result"] = body
                            return
                        if kind == "H":
                            self.hello = body
                            self.events.append(("restart", body))
                        elif kind == "X":
                            self.events.append(("x", body))
                            if body.get("fatal"):
                                outcome["fatal"] = body
                                return
                except Exception as error:
                    outcome["fatal"] = {"fatal": "worker-pipe", "detail": str(error)}

            thread = threading.Thread(target=read, daemon=True)
            thread.start()
            thread.join(timeout)
            if thread.is_alive():
                kill = self.kill()
                thread.join(5)
                raise WorkerTimeout("batch deadline passed; " + str(kill))
            if "fatal" in outcome:
                self.kill()
                raise WorkerError(json.dumps(outcome["fatal"]))
            return outcome["result"]

    def recycle(self):
        """Ask the idle worker to quit its Excel and start a new one; returns the new hello."""
        with self.lock:
            self._send("R", {})
            self.hello = self._expect_hello(self.startup_seconds + 30)
            return self.hello

    def await_restart(self):
        """After an excel-crashed result the worker restarts its Excel once and sends a new hello (or a fatal X)."""
        self.hello = self._expect_hello(self.startup_seconds + 30)
        return self.hello
