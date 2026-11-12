"""touchstone/1 relay: runs on a Linux lane host, started by the Windows broker over one persistent ssh session.

Nothing is installed on the host: the broker sends this module with protocol.py and v1.py in the ssh command.
The ssh stdin/stdout carry length-prefixed zlib frames to and from the broker; lanes connect to a 0600 Unix
socket (default ~/.excel-service/excel.sock) with the same frames, and the v1 file queue (~/excel-queue/in) is
also served, so v1 clients keep working.  The relay holds no state worth keeping: it exits when its ssh
session ends, and clients resend unanswered ids, which the broker answers from its cache without evaluating
them again.
"""
import asyncio
import hashlib
import json
import os
import socket
import sys
import time

import protocol
import v1


class Outgoing:
    """Two-priority outgoing frame queue onto one stream."""

    def __init__(self, writer, compress):
        self.writer, self.compress = writer, compress
        self.high, self.low = [], []
        self.wake = asyncio.Event()
        self.closed = False

    def send(self, kind, obj, high=False):
        if self.closed:
            return
        (self.high if high else self.low).append(protocol.pack(kind, obj, self.compress))
        self.wake.set()

    async def run(self):
        try:
            while not self.closed:
                await self.wake.wait()
                self.wake.clear()
                while self.high or self.low:
                    queue = self.high if self.high else self.low
                    self.writer.write(queue.pop(0))
                    await self.writer.drain()
        except (ConnectionError, OSError):
            pass
        finally:
            self.closed = True


class Relay:
    def __init__(self, args):
        self.socket_path = os.path.expanduser(args.get("socket", "~/.excel-service/excel.sock"))
        self.socket_ino = None
        self.v1_queue = os.path.expanduser(args["v1_queue"]) if args.get("v1_queue") else None
        self.server = None
        self.envs = {}
        self.clients = {}
        self.next_client = 1
        self.priority = {}          # request id -> interactive?
        self.v1_pending = {}        # client tag -> (Reply, queue dir, rid)
        self.broker = None
        self.hello_seen = asyncio.Event()
        self.host = socket.gethostname()

    # -- broker link --------------------------------------------------------------------
    async def run(self):
        loop = asyncio.get_running_loop()
        reader = asyncio.StreamReader(limit=1 << 26)
        await loop.connect_read_pipe(lambda: asyncio.StreamReaderProtocol(reader), sys.stdin.buffer)
        transport, proto = await loop.connect_write_pipe(asyncio.streams.FlowControlMixin, os.fdopen(sys.stdout.fileno(), "wb", 0))
        writer = asyncio.StreamWriter(transport, proto, None, loop)
        self.broker = Outgoing(writer, True)
        writer_task = asyncio.create_task(self.broker.run())
        self.broker.send(protocol.HELLO, {"v": protocol.VERSION, "name": "relay:" + self.host, "compress": True, "pid": os.getpid()}, True)
        os.makedirs(os.path.dirname(self.socket_path), mode=0o700, exist_ok=True)
        try:
            os.unlink(self.socket_path)
        except FileNotFoundError:
            pass
        await self.bind_socket()
        tasks = [asyncio.create_task(self.v1_scan())] if self.v1_queue else []
        tasks.append(asyncio.create_task(self.keep_socket_path()))
        frames = protocol.FrameReader()
        try:
            while True:
                data = await reader.read(1 << 20)
                if not data:
                    break
                for kind, body in frames.feed(data):
                    self.from_broker(kind, body)
        finally:
            for t in tasks:
                t.cancel()
            self.listener.close()
            for client in list(self.clients.values()):
                client.closed = True
                try:
                    client.writer.close()
                except Exception:
                    pass
            if self.owns_socket_path():
                try:
                    os.unlink(self.socket_path)
                except OSError:
                    pass
            self.broker.closed = True
            writer_task.cancel()

    async def bind_socket(self):
        self.listener = await asyncio.start_unix_server(self.on_client, path=self.socket_path)
        os.chmod(self.socket_path, 0o600)
        self.socket_ino = os.stat(self.socket_path).st_ino

    def owns_socket_path(self):
        """True while the socket path still names this relay's own socket (another relay may have replaced it)."""
        try:
            return os.stat(self.socket_path).st_ino == self.socket_ino
        except OSError:
            return False

    async def keep_socket_path(self):
        # A relay that starts while this one runs replaces the path and removes it when it exits, which
        # left this relay listening on a socket no client could find (2026-11-12, both lane hosts).
        # Re-create a missing path; leave a path another live relay owns alone.
        while True:
            await asyncio.sleep(5)
            if not os.path.exists(self.socket_path):
                try:
                    self.listener.close()
                    await self.bind_socket()
                except OSError:
                    pass

    def from_broker(self, kind, body):
        if kind == protocol.HELLO:
            self.server = body.get("server") or self.server
            self.envs.update(body.get("envs", {}))
            self.hello_seen.set()
            return
        if kind == protocol.ENV:
            self.envs[body["env_id"]] = body["env"]
            for client in self.clients.values():
                client.send(protocol.ENV, body, True)
            return
        tag = body.get("c")
        if isinstance(tag, str) and tag in self.v1_pending:
            self.v1_answer(tag, kind, body)
            return
        client = self.clients.get(tag)
        if client is None:
            return
        high = self.priority.get(body.get("id"), False)
        client.send(kind, body, high)
        if kind in (protocol.DONE, protocol.ERROR) and "seq" not in body:
            self.priority.pop(body.get("id"), None)

    # -- lanes --------------------------------------------------------------------------
    async def on_client(self, reader, writer):
        number = self.next_client
        self.next_client += 1
        out = Outgoing(writer, False)
        out.writer = writer
        out.number = number
        self.clients[number] = out
        task = asyncio.create_task(out.run())
        frames = protocol.FrameReader()
        try:
            while True:
                data = await reader.read(1 << 20)
                if not data:
                    break
                for kind, body in frames.feed(data):
                    await self.from_client(out, number, kind, body)
        except (ConnectionError, OSError, ValueError):
            pass
        finally:
            out.closed = True
            self.clients.pop(number, None)
            task.cancel()
            try:
                writer.close()
            except Exception:
                pass

    async def from_client(self, out, number, kind, body):
        if kind == protocol.HELLO:
            try:
                await asyncio.wait_for(self.hello_seen.wait(), 10)
            except asyncio.TimeoutError:
                pass
            out.send(protocol.HELLO, {"v": protocol.VERSION, "server": self.server or self.host, "envs": self.envs}, True)
        elif kind == protocol.PING:
            out.send(protocol.PONG, body, True)
        elif kind == protocol.SUBMIT:
            body["c"] = number
            opts = body.get("opts")
            if opts is not None:
                interactive = opts.get("priority") == "interactive" or (opts.get("priority") is None and 0 < opts.get("calls", 0) <= 100)
                self.priority[body["id"]] = interactive
            self.broker.send(kind, body, self.priority.get(body["id"], False))
        elif kind in (protocol.WAIT, protocol.CANCEL):
            body["c"] = number
            self.broker.send(kind, body, True)

    # -- v1 file queue ------------------------------------------------------------------
    async def v1_scan(self):
        root = self.v1_queue
        os.makedirs(os.path.join(root, "in"), exist_ok=True)
        os.makedirs(os.path.join(root, "out"), exist_ok=True)
        await self.hello_seen.wait()
        seen = {}
        while True:
            try:
                for name in os.listdir(os.path.join(root, "in")):
                    if len(name) != 37 or not name.endswith(".json"):
                        continue
                    path = os.path.join(root, "in", name)
                    if os.path.islink(path):
                        continue
                    stamp = os.stat(path).st_mtime_ns
                    if seen.get(name) == stamp:
                        continue
                    seen[name] = stamp
                    self.v1_submit(root, name[:-5], path)
            except OSError:
                pass
            await asyncio.sleep(0.02)

    def v1_submit(self, root, rid, path):
        out_path = os.path.join(root, "out", rid + ".json")
        if os.path.exists(out_path):
            return
        try:
            with open(path, "rb") as stream:
                raw = stream.read(v1.MAX_BYTES + 1)
        except OSError:
            return
        digest = hashlib.sha256(raw).hexdigest()
        tag = "v1:" + rid
        try:
            function, rows, index = v1.parse_request(raw, rid)
            frames = v1.submit_frames(rid, function, rows, index, echo=True)
            reply = v1.Reply(rid, function, rows, index, digest, {k: {"env": v} for k, v in self.envs.items()}, self.server)
        except Exception as error:
            reply = v1.Reply(rid, None, [], None, digest, {}, self.server)
            reply.fail("refused", str(error))
            self.v1_finish(root, rid, path, reply)
            return
        self.v1_pending[tag] = (reply, root, rid, path)
        for frame in frames:
            frame["c"] = tag
            self.broker.send(protocol.SUBMIT, frame, 0 < len(rows) <= 100)

    def v1_answer(self, tag, kind, body):
        reply, root, rid, path = self.v1_pending[tag]
        reply.envs = {k: {"env": v} for k, v in self.envs.items()}
        if kind == protocol.ANSWER:
            reply.add(body)
        elif kind == protocol.ERROR:
            reply.fail(body.get("code", "error"), body.get("detail"))
            del self.v1_pending[tag]
            self.v1_finish(root, rid, path, reply)
        elif kind == protocol.DONE:
            del self.v1_pending[tag]
            self.v1_finish(root, rid, path, reply)

    def v1_finish(self, root, rid, path, reply):
        answer = reply.build()
        out_path = os.path.join(root, "out", rid + ".json")
        temporary = out_path + ".tmp"
        with open(temporary, "w", encoding="utf-8") as stream:
            json.dump(answer, stream, allow_nan=False, ensure_ascii=True)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, out_path)
        try:
            os.unlink(path)
        except OSError:
            pass


def main(args):
    asyncio.run(Relay(args).run())
