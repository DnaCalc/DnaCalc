"""The broker's link to the Linux lane hosts: one persistent ssh session per host, the relay inside it.

Windows keeps opening the connection (no inbound ports, nothing installed on Linux).  A dropped session is
re-established with backoff 1, 2, 4 ... 60 s; the relay exits with its session and lanes resend what was not
answered, which the broker serves from its answer cache.
"""
import asyncio
import base64
import subprocess
import time
import zlib
from datetime import datetime, timezone
from pathlib import Path

import protocol
from broker import Conn

HERE = Path(__file__).resolve().parent

SSH_OPTIONS = ["-T", "-C", "-o", "BatchMode=yes", "-o", "ConnectTimeout=8", "-o", "ServerAliveInterval=5",
               "-o", "ServerAliveCountMax=3"]


REMOTE_COMMAND = ("python3 -u -c \"import zlib,base64;"
                  "exec(zlib.decompress(base64.b64decode(open(0,'rb',0,closefd=False).readline())))\"")


def bundle_program(modules, epilogue, extra=None):
    """(remote command, first stdin line): a self-extracting Python program carrying the named touchstone
    modules and then `epilogue` source.  The program travels on stdin (a long ssh command line is cut at
    about 8 KB); the stub reads that one line unbuffered, so the frames that follow it are not consumed."""
    sources = {name: ((extra or {}).get(name) or (HERE / (name + ".py"))).read_text(encoding="utf-8") for name in modules}
    program = (
        "import sys, types\n"
        f"SRC = {sources!r}\n"
        f"for name in {tuple(modules)!r}:\n"
        "    m = types.ModuleType(name); sys.modules[name] = m\n"
        "    exec(compile(SRC[name], name + '.py', 'exec'), m.__dict__)\n" + epilogue)
    payload = base64.b64encode(zlib.compress(program.encode("utf-8"), 9))
    return REMOTE_COMMAND, payload + b"\n"


def bundle(relay_args):
    """The remote command and first line that run the relay."""
    return bundle_program(("protocol", "v1", "relay"), f"sys.modules['relay'].main({relay_args!r})\n")


class PeerManager:
    """One supervised relay per host.  Besides the ssh session ending, a relay is dead when it stops answering
    pings (ssh up, relay wedged) or reports that its socket is gone and stays gone (2026-11-16: the sockets
    vanished at 10:40 and nobody knew until 13:03).  Each loss and restart is logged with host, time and
    reason, and `snapshot()` feeds `broker.py status`."""

    PING_SECONDS = 5.0
    SILENT_SECONDS = 20.0       # no frame at all from the relay for this long: dead
    SOCKET_GONE_SECONDS = 20.0  # the relay reports its socket path missing for this long: dead

    def __init__(self, broker, hosts, relay_args=None, spawn=None, ping_seconds=None, silent_seconds=None,
                 socket_gone_seconds=None, max_backoff=60.0, first_backoff=1.0):
        self.first_backoff = first_backoff
        self.broker, self.hosts = broker, hosts
        self.relay_args = relay_args or {}
        self.spawn = spawn or self.spawn_ssh
        self.ping_seconds = ping_seconds or self.PING_SECONDS
        self.silent_seconds = silent_seconds or self.SILENT_SECONDS
        self.socket_gone_seconds = socket_gone_seconds or self.SOCKET_GONE_SECONDS
        self.max_backoff = max_backoff
        self.state = {h: "down" for h in hosts}
        self.sessions = {h: 0 for h in hosts}
        self.info = {h: {"since": time.time(), "restarts": 0, "reason": "not started", "last_rx": None, "socket": None,
                         "backoff": None} for h in hosts}

    @staticmethod
    def utc(stamp):
        if stamp is None:
            return None
        return datetime.fromtimestamp(stamp, timezone.utc).isoformat(timespec="seconds")

    def snapshot(self):
        """Per host: state, since when, session and restart counts, last reason, last frame from the relay."""
        return [{"host": h, "state": self.state[h], "since_utc": self.utc(self.info[h]["since"]),
                 "sessions": self.sessions[h], "restarts": self.info[h]["restarts"], "reason": self.info[h]["reason"],
                 "last_rx_utc": self.utc(self.info[h]["last_rx"]), "relay_socket": self.info[h]["socket"],
                 "retry_in_s": self.info[h]["backoff"] if self.state[h] == "down" else None} for h in self.hosts]

    def summary(self):
        """One short clause for the health line, empty when every relay is up."""
        down = [h for h in self.hosts if self.state[h] != "up"]
        if not down:
            return ""
        return "relays down: " + ", ".join(f"{h} since {self.utc(self.info[h]['since'])} ({self.info[h]['reason'][:60]})" for h in down)

    async def spawn_ssh(self, host):
        command, first = bundle(self.relay_args)
        process = await asyncio.create_subprocess_exec(
            "ssh", *SSH_OPTIONS, host, command,
            stdin=asyncio.subprocess.PIPE, stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.DEVNULL,
            creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        process.stdin.write(first)
        await process.stdin.drain()
        return process

    async def run(self):
        await asyncio.gather(*(self.keep(host) for host in self.hosts))

    def mark(self, host, state, reason):
        if self.state[host] != state:
            self.info[host]["since"] = time.time()
        self.state[host] = state
        self.info[host]["reason"] = reason

    async def watch(self, host, conn):
        """Ping the relay; close the connection (so the session is torn down and restarted) when it is dead."""
        info = self.info[host]
        gone_since = None
        while not conn.closed:
            await asyncio.sleep(self.ping_seconds)
            conn.send(protocol.PING, {"t": time.time(), "broker_flags": self.broker.broker_flags()}, True)
            now = time.monotonic()
            if now - conn.last_rx > self.silent_seconds:
                info["drop"] = f"relay silent for {now - conn.last_rx:.0f}s"
                break
            if getattr(conn, "relay_socket", True) is False:
                gone_since = gone_since or now
                if now - gone_since > self.socket_gone_seconds:
                    info["drop"] = f"relay socket gone for {now - gone_since:.0f}s"
                    break
            else:
                gone_since = None
        conn.close()

    async def keep(self, host):
        first = self.first_backoff
        backoff = first
        info = self.info[host]
        while not self.broker.stopping:
            started = time.monotonic()
            process = None
            watcher = None
            info["drop"] = None
            try:
                process = await self.spawn(host)
                conn = Conn(host, process.stdout, process.stdin, compress=True)
                conn.source = {"transport": "relay", "host": host}
                conn.last_rx = time.monotonic()
                self.sessions[host] += 1
                self.mark(host, "up", "session %d" % self.sessions[host])
                info["backoff"] = None
                self.broker.log_event("peer-up", host=host, session=self.sessions[host])
                watcher = asyncio.create_task(self.watch(host, conn))
                await self.broker.serve(conn)
                info["last_rx"] = time.time() - (time.monotonic() - conn.last_rx)
                info["socket"] = getattr(conn, "relay_socket", None)
                reason = info["drop"] or "ssh session ended"
            except asyncio.CancelledError:
                raise
            except Exception as error:
                reason = f"{type(error).__name__}: {str(error)[:150]}"
                self.broker.log_event("peer-error", host=host, detail=reason)
            finally:
                if watcher is not None:
                    watcher.cancel()
                if process is not None and process.returncode is None:
                    try:
                        process.kill()          # our own ssh subprocess
                    except ProcessLookupError:
                        pass
                    await process.wait()
            lived = time.monotonic() - started
            if lived > 30:
                backoff = first
            info["restarts"] += 1
            info["backoff"] = backoff
            self.mark(host, "down", reason)
            self.broker.log_event("peer-down", host=host, seconds=round(lived, 1), reason=reason,
                                  lost_utc=self.utc(time.time()), retry_in_s=backoff, restarts=info["restarts"])
            await asyncio.sleep(backoff)
            backoff = min(self.max_backoff, backoff * 2)
