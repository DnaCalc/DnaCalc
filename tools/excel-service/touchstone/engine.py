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
PROTECTED_PIDS = set()


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
        self.sweep_leftovers = True      # after a kill, stop an Excel this worker started that never reported

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

    def kill(self, sweep=None):
        """Stop this worker and the Excel it owns, nothing else.

        A worker that was restarting its Excel when it was killed may leave a new Excel the broker never heard
        of; with `sweep` that one is found by its command line (this state folder, this worker's launch files)
        and stopped by PID and start ticks."""
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
        if self.sweep_leftovers if sweep is None else sweep:
            try:
                self.sweep()
            except Exception:
                pass
        return outcome

    def sweep(self):
        marker = ("\\launches\\" + self.name + "-").lower()
        stopped = []
        for pid, ticks, command in list_excel_processes():
            if names_state(command, self.root) and marker in command.lower().replace("/", "\\"):
                identity = process_identity(pid)
                if identity and abs(identity[1] - ticks) <= 10_000:
                    stopped.append((pid, kill_owned_excel(pid, identity[1])))
        return stopped

    def stop(self):
        if self.alive():
            try:
                self._send("Q", {})
                self.process.wait(15)
            except Exception:
                pass
        self.kill(sweep=self.sweep_leftovers and self.alive())

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



# ---- orphans and the build --------------------------------------------------------------
def list_excel_processes():
    """[(pid, start_ticks, command line)] of every Excel on this machine (read-only query)."""
    script = ("Get-CimInstance Win32_Process -Filter \"Name='EXCEL.EXE'\" | ForEach-Object { "
              "[pscustomobject]@{pid=$_.ProcessId; ticks=$_.CreationDate.ToUniversalTime().Ticks; cmd=$_.CommandLine} } "
              "| ConvertTo-Json -Compress")
    out = subprocess.run(["pwsh", "-NoProfile", "-NonInteractive", "-Command", script], capture_output=True, text=True,
                         timeout=60, creationflags=subprocess.CREATE_NO_WINDOW)
    text = out.stdout.strip()
    if not text:
        return []
    data = json.loads(text)
    if isinstance(data, dict):
        data = [data]
    return [(int(d["pid"]), int(d["ticks"]), d.get("cmd") or "") for d in data]


def names_state(command_line, state_dir):
    """True when an Excel command line names this service's state folder (the folder itself, not a sibling
    whose name merely starts the same)."""
    low = command_line.lower().replace("/", "\\")
    for form in {str(Path(state_dir)), str(Path(state_dir).resolve())}:
        if form.lower().rstrip("\\") + "\\" in low:
            return True
    return False


def stop_orphans(state_dir, processes=None, kill=None):
    """Stop worker Excels left behind by an earlier broker: only those whose command line names state_dir.

    Each is stopped by PID and start ticks through kill_owned_excel (refused when the identity changed).
    Returns [(pid, outcome)].  `processes` and `kill` are test seams."""
    kill = kill or kill_owned_excel
    found = []
    for pid, ticks, command in (list_excel_processes() if processes is None else processes):
        if not names_state(command, state_dir):
            continue
        identity = process_identity(pid) if processes is None else ("excel.exe", ticks)
        if identity is None:
            continue
        # CIM keeps microseconds, the process times keep 100 ns: the same instant within a millisecond
        if abs(identity[1] - ticks) > 10_000:
            found.append((pid, "start time differs; left alone"))
            continue
        found.append((pid, kill(pid, identity[1])))
    return found


def worker_sources_newest(project):
    newest = 0
    for path in Path(project).rglob("*"):
        if path.suffix in (".cs", ".csproj") and "obj" not in path.parts and "bin" not in path.parts:
            newest = max(newest, path.stat().st_mtime_ns)
    return newest


def worker_is_stale(project=None):
    project = Path(project) if project else WORKER_EXE.parent.parent
    dll = project / "bin" / "excel-worker.dll"
    exe = project / "bin" / "excel-worker.exe"
    if not dll.exists() or not exe.exists():
        return True
    return worker_sources_newest(project) > dll.stat().st_mtime_ns


def ensure_worker_built(project=None, command=None):
    """Build the C# worker (dotnet build -c Release) when bin/ is missing or older than its sources.

    Fails loudly (RuntimeError with the build output) when the build fails or leaves no worker behind."""
    project = Path(project) if project else WORKER_EXE.parent.parent
    if not worker_is_stale(project):
        return False
    command = command or ["dotnet", "build", "-c", "Release", "--nologo", "-v", "q"]
    result = subprocess.run(command, cwd=str(project), capture_output=True, text=True, timeout=900,
                            creationflags=subprocess.CREATE_NO_WINDOW)
    tail = (result.stdout + result.stderr)[-3000:]
    if result.returncode != 0:
        raise RuntimeError(f"worker build failed (exit {result.returncode}) in {project}:\n{tail}")
    if worker_is_stale(project):
        raise RuntimeError(f"worker build reported success but {project / 'bin'} holds no current "
                           f"excel-worker.exe and .dll:\n{tail}")
    return True
