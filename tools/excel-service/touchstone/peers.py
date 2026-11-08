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
from pathlib import Path

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
    def __init__(self, broker, hosts, relay_args=None):
        self.broker, self.hosts = broker, hosts
        self.relay_args = relay_args or {}
        self.state = {h: "down" for h in hosts}
        self.sessions = {h: 0 for h in hosts}

    async def run(self):
        await asyncio.gather(*(self.keep(host) for host in self.hosts))

    async def keep(self, host):
        from broker import Conn
        backoff = 1.0
        while not self.broker.stopping:
            started = time.monotonic()
            process = None
            try:
                command, first = bundle(self.relay_args)
                process = await asyncio.create_subprocess_exec(
                    "ssh", *SSH_OPTIONS, host, command,
                    stdin=asyncio.subprocess.PIPE, stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.DEVNULL,
                    creationflags=subprocess.CREATE_NO_WINDOW)
                process.stdin.write(first)
                await process.stdin.drain()
                conn = Conn(host, process.stdout, process.stdin, compress=True)
                self.state[host] = "up"
                self.sessions[host] += 1
                self.broker.log_event("peer-up", host=host, session=self.sessions[host])
                await self.broker.serve(conn)
            except asyncio.CancelledError:
                raise
            except Exception as error:
                self.broker.log_event("peer-error", host=host, detail=str(error)[:200])
            finally:
                self.state[host] = "down"
                if process is not None and process.returncode is None:
                    try:
                        process.kill()          # our own ssh subprocess
                    except ProcessLookupError:
                        pass
                    await process.wait()
            lived = time.monotonic() - started
            self.broker.log_event("peer-down", host=host, seconds=round(lived, 1))
            if lived > 30:
                backoff = 1.0
            await asyncio.sleep(backoff)
            backoff = min(60.0, backoff * 2)
