"""Explicitly started Windows loop; all Excel work uses the existing session host."""
import argparse
import base64
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import queue
import re
import socket
import subprocess
import sys
import threading
import time

from client import atomic_json
from policy import ERRORS, catalogue_names, decode, validate
from queue_bridge import operation

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
DEFAULT_STATE = Path(r"C:\Work\DnaCalc\codex-runs\excel-service")


def utc():
    return datetime.now(timezone.utc).isoformat()


class Peer:
    def __init__(self, name, local_queue):
        self.name, self.local_queue = name, local_queue
        self.process = None
        self.replies = queue.Queue()

    def close(self):
        if self.process:
            # This is our SSH subprocess, never an Excel process.
            self.process.kill()
            self.process.wait(timeout=10)
            self.process = None

    def call(self, message):
        if self.name == "local":
            return operation(self.local_queue, message)
        if not self.process:
            code = base64.b64encode((HERE / "queue_bridge.py").read_bytes()).decode("ascii")
            command = "python3 -u -c \"import base64;exec(base64.b64decode('" + code + "'))\""
            self.process = subprocess.Popen(
                ["ssh", "-T", "-o", "BatchMode=yes", "-o", "ConnectTimeout=8",
                 "-o", "ServerAliveInterval=5", "-o", "ServerAliveCountMax=2", self.name, command],
                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                encoding="utf-8", creationflags=subprocess.CREATE_NO_WINDOW)
            self.replies = queue.Queue()
            def reader(stream, replies):
                try:
                    for line in stream:
                        replies.put(line)
                finally:
                    replies.put(None)
            threading.Thread(target=reader, args=(self.process.stdout, self.replies), daemon=True).start()
        self.process.stdin.write(json.dumps(message, ensure_ascii=True) + "\n")
        self.process.stdin.flush()
        line = self.replies.get(timeout=20)
        if line is None:
            raise ConnectionError("SSH queue peer disconnected")
        response = json.loads(line)
        if not response["ok"]:
            raise RuntimeError(response["error"])
        return response["value"]


def typed(value):
    if "number" in value:
        return {"type": "number", "bits": value["number"]}
    if "logical" in value:
        return {"type": "boolean", "value": value["logical"]}
    if "text" in value:
        units = value["text"]
        raw = b"".join(int(units[i:i+4], 16).to_bytes(2, "little") for i in range(0, len(units), 4))
        return {"type": "text", "value": raw.decode("utf-16-le", errors="surrogatepass")}
    if "empty" in value:
        return {"type": "blank"}
    if "error_code" in value:
        code = value["error_code"]
        return {"type": "error", "code": next((k for k, v in ERRORS.items() if v == code), code)}
    raise ValueError("Excel returned an unread or unsupported value")


def witness_request(request):
    rows = []
    for i, row in enumerate(request["rows"]):
        args, references = [], []
        for c, arg in enumerate(row, 1):
            kind = arg["type"]
            value = ({"number": arg["bits"]} if kind == "number" else
                     {"text": arg["value"]} if kind == "text" else
                     {"logical": arg["value"]} if kind == "boolean" else
                     {"empty": True} if kind == "blank" else {"error_code": ERRORS[arg["code"]]})
            args.append(value)
            if arg.get("mode", "cell") == "cell":
                references.append(f"RC[{c}]")
            elif kind == "text":
                references.append('"' + arg["value"].replace('"', '""') + '"')
            elif kind == "boolean":
                references.append("TRUE" if arg["value"] else "FALSE")
            else:
                references.append(arg["code"])
        rows.append({"id": str(i), "args": args,
                     "argument_modes": [arg.get("mode", "cell") for arg in row],
                     "formula": "=" + request["function"] + "(" + ",".join(references) + ")"})
    return {"rows": rows}


class Service:
    def __init__(self, options):
        self.options = options
        self.state = options.state.resolve()
        self.session = self.state / "session"
        self.stopping = threading.Event()
        self.pending = queue.Queue()
        self.names = catalogue_names(ROOT)
        self.log_lock = threading.Lock()

    def log(self, kind, **data):
        line = json.dumps({"utc": utc(), "kind": kind, **data}, ensure_ascii=True, allow_nan=False)
        with self.log_lock, (self.state / "log/questions.jsonl").open("a", encoding="utf-8") as stream:
            stream.write(line + "\n")
            stream.flush()
            os.fsync(stream.fileno())

    def host(self, *args, timeout=110):
        result = subprocess.run(["pwsh", "-NoLogo", "-NoProfile", "-NonInteractive", "-File",
                                 str(ROOT / "tools/excel-session/submit.ps1"), "-SessionDirectory",
                                 str(self.session), *args], cwd=ROOT, capture_output=True,
                                encoding="utf-8", errors="replace", timeout=timeout,
                                creationflags=subprocess.CREATE_NO_WINDOW)
        if result.returncode:
            raise RuntimeError((result.stdout + result.stderr)[-5000:])
        return result.stdout

    def fail(self, request_id, error):
        return {"version": 1, "id": request_id, "ok": False, "error": str(error),
                "rows": [], "excel_build": None, "machine": socket.gethostname(), "timestamp": utc()}

    def stop_requested(self, stop):
        retries = 0
        while True:
            try:
                requested = stop.exists()
                break
            except OSError as error:
                if getattr(error, "winerror", None) != 1450:
                    raise
                retries += 1
                if retries == 1:
                    self.resource_log("resource-retry", operation="stop-file-stat", error=str(error))
                # Retry only this read. Never repeat an Excel job or replace a
                # pending answer, and do not assume the stop file is absent.
                self.stopping.wait(1)
        if retries:
            self.resource_log("resource-recovered", operation="stop-file-stat", retries=retries)
        return requested

    def resource_log(self, kind, **data):
        # The same resource shortage may prevent opening the diagnostic log.
        try:
            self.log(kind, **data)
        except OSError as error:
            try:
                print(f"{utc()} {kind}: {data}; log unavailable: {error}", file=sys.stderr, flush=True)
            except OSError:
                pass

    def evaluate(self, request, directory):
        prepared = directory / "witness-request.json"
        output = directory / "witness-result.json"
        atomic_json(prepared, witness_request(request))
        # The deadline is enforced by the existing host, which alone owns Excel.
        receipt_line = self.host("-NoSaveBaseline", "-Script",
                  str(ROOT / "platform/calc-oracle-excel/worker/witness-worker.ps1"),
                  "-Parameters", json.dumps({"Request": str(prepared), "Result": str(output), "BuiltInBlank": True}),
                  "-WorkingDirectory", str(ROOT), "-TimeoutSeconds", "75", "-Environment", "excel-service")
        receipt_path = Path(receipt_line.split("dnacalc-excel-session/1 receipt ", 1)[1].strip())
        receipt_bytes = receipt_path.read_bytes()
        receipt = json.loads(receipt_bytes)
        if not receipt["success"] or not receipt["no_save_baseline"] or not receipt["preserve_automation_security"]:
            raise RuntimeError("session receipt does not establish the required execution policy")
        if receipt["result_sha256"] != hashlib.sha256(output.read_bytes()).hexdigest():
            raise RuntimeError("session result digest changed")
        (directory / "session-receipt.json").write_bytes(receipt_bytes)
        raw = json.loads(output.read_text(encoding="utf-8-sig"))
        if raw["fatal"] or raw["failures"]:
            raise RuntimeError(raw["fatal"] or str(raw["failures"]))
        if len(raw["rows"]) != len(request["rows"]) or any(not row["input_kept"] for row in raw["rows"]):
            raise RuntimeError("Excel changed an input; no answer is qualified (see retained input audit)")
        return {"version": 1, "id": request["id"], "ok": True,
                "rows": [typed(row["value"]) for row in raw["rows"]],
                "inputs": [[typed(v) for v in row["input_audit"]["read_back"]] for row in raw["rows"]],
                "excel_version": raw["excel_version"], "excel_build": raw["excel_build"],
                "excel_file_version": raw["excel_file_version"], "excel_pid": raw["excel_pid"],
                "calculation_settings": raw.get("calculation_settings"),
                "automation_security_before": raw["automation_security_before"],
                "automation_security_after": raw["automation_security_after"],
                "session_receipt": str(receipt_path),
                "session_receipt_sha256": hashlib.sha256(receipt_bytes).hexdigest(),
                "machine": socket.gethostname(), "timestamp": utc()}

    def peer_loop(self, name):
        peer = Peer(name, self.options.local_queue)
        previous_error = None
        try:
            while not self.stopping.is_set():
                try:
                    item = peer.call({"op": "take"})
                    if item:
                        done = queue.Queue()
                        self.pending.put((name, item, done))
                        shutdown_deadline = None
                        while True:
                            try:
                                answer = done.get(timeout=.2)
                                peer.call({"op": "put", "id": item["id"], "answer": answer})
                                break
                            except queue.Empty:
                                if self.stopping.is_set():
                                    if shutdown_deadline is None:
                                        shutdown_deadline = time.monotonic() + 20
                                    elif time.monotonic() > shutdown_deadline:
                                        raise RuntimeError("shutdown response was not supplied")
                    previous_error = None
                    self.stopping.wait(.2)
                except Exception as error:
                    if str(error) != previous_error:
                        self.log("peer-error", peer=name, error=str(error))
                        previous_error = str(error)
                    peer.close()
                    self.stopping.wait(2)
        finally:
            peer.close()

    def answer(self, origin, item, *, aborted=False):
        request_id = item["id"]
        if not isinstance(request_id, str) or not re.fullmatch("[0-9a-f]{32}", request_id):
            raise ValueError("invalid queue filename")
        directory = self.state / "requests" / origin / request_id
        directory.mkdir(parents=True, exist_ok=True)
        raw = base64.b64decode(item["raw"], validate=True)
        digest = hashlib.sha256(raw).hexdigest()
        saved = directory / "answer.json"
        if saved.exists():
            answer = json.loads(saved.read_text(encoding="utf-8"))
            if answer["request_sha256"] != digest:
                answer = self.fail(request_id, "request id was reused with different contents")
                answer["request_sha256"] = digest
                self.log("question-conflict", origin=origin, id=request_id, raw_base64=item["raw"])
            self.log("answer-replay", origin=origin, answer=answer)
            return answer
        started = time.monotonic()
        try:
            interrupted = (directory / "question.json").exists()
            # Retain before validation/evaluation. Invalid questions also receive answers.
            if not interrupted:
                with (directory / "question.json").open("wb") as stream:
                    stream.write(raw)
                    stream.flush()
                    os.fsync(stream.fileno())
            self.log("question", origin=origin, id=request_id, sha256=digest,
                     raw_base64=item["raw"], oversized=item["oversized"])
            if interrupted:
                raise RuntimeError("service interrupted this request; not automatically recalculated")
            if aborted:
                raise RuntimeError("service stopped before evaluating this request")
            if item["oversized"]:
                raise ValueError("request exceeds 16 MiB; retained prefix only")
            request = validate(decode(raw), self.names, request_id)
            answer = self.evaluate(request, directory)
        except Exception as error:
            answer = self.fail(request_id, error)
        answer["service_seconds"] = time.monotonic() - started
        answer["request_sha256"] = digest
        answer["evidence"] = str(directory)
        # Save before delivery. Recovery resends this exact answer, without rerunning Excel.
        atomic_json(saved, answer)
        self.log("answer", origin=origin, answer=answer)
        return answer

    def run(self):
        import msvcrt
        for name in ("log", "requests", "session"):
            (self.state / name).mkdir(parents=True, exist_ok=True)
        with (self.state / "service.lock").open("a+b") as lock:
            lock.seek(0)
            msvcrt.locking(lock.fileno(), msvcrt.LK_NBLCK, 1)
            stop = self.state / "stop"
            stop.unlink(missing_ok=True)
            threads = []
            try:
                # Recover only our dedicated host, including an orphan from an earlier loop.
                self.host("-Stop")
                self.host("-Start", "-NoSaveBaseline")
                self.log("started", pid=os.getpid(), catalogue=sorted(self.names))
                for name in ["local", *self.options.hosts]:
                    thread = threading.Thread(target=self.peer_loop, args=(name,), daemon=True)
                    thread.start()
                    threads.append(thread)
                atomic_json(self.state / "status.json", {"pid": os.getpid(), "running": True, "utc": utc()})
                while not self.stop_requested(stop):
                    try:
                        origin, item, done = self.pending.get(timeout=.2)
                    except queue.Empty:
                        continue
                    answer = self.answer(origin, item)
                    done.put(answer)
            finally:
                self.stopping.set()
                # Publish the active answer and fail requests already taken by peers,
                # including a take that finishes just as the stop file is observed.
                deadline = time.monotonic() + 22
                while any(thread.is_alive() for thread in threads) and time.monotonic() < deadline:
                    try:
                        origin, item, done = self.pending.get(timeout=.2)
                        done.put(self.answer(origin, item, aborted=True))
                    except queue.Empty:
                        pass
                for thread in threads:
                    thread.join(timeout=.1)
                self.host("-Stop")
                self.log("stopped", pid=os.getpid())
                atomic_json(self.state / "status.json", {"pid": os.getpid(), "running": False, "utc": utc()})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["start", "stop", "run"])
    parser.add_argument("--state", type=Path, default=DEFAULT_STATE)
    parser.add_argument("--local-queue", type=Path, default=Path.home() / "excel-queue")
    parser.add_argument("--hosts", nargs="*", choices=["dna-firehorse", "dna-koderbot"],
                        default=["dna-firehorse", "dna-koderbot"])
    options = parser.parse_args()
    options.state.mkdir(parents=True, exist_ok=True)
    status = options.state / "status.json"
    if options.command == "run":
        Service(options).run()
    elif options.command == "start":
        # Launch manually, detached and hidden; no login entry, task or OS service.
        with (options.state / "console.log").open("a", encoding="utf-8") as log:
            process = subprocess.Popen([sys.executable, str(HERE / "service.py"), "run", "--state",
                                        str(options.state), "--local-queue", str(options.local_queue),
                                        "--hosts", *options.hosts], stdout=log, stderr=log,
                                       creationflags=subprocess.CREATE_NO_WINDOW | subprocess.DETACHED_PROCESS)
        deadline = time.monotonic() + 45
        while time.monotonic() < deadline:
            if process.poll() is not None:
                raise RuntimeError("Service did not start; see " + str(options.state / "console.log"))
            if status.exists() and json.loads(status.read_text())["pid"] == process.pid:
                print("Started Excel question service; stop file: " + str(options.state / "stop"))
                return
            time.sleep(.1)
        raise TimeoutError("Service startup not confirmed; inspect console.log")
    else:
        import msvcrt
        (options.state / "stop").touch()
        deadline = time.monotonic() + 150
        # The process lock remains authoritative even after an unclean loop exit;
        # status/PID alone cannot distinguish a crash from an unrelated reused PID.
        with (options.state / "service.lock").open("a+b") as lock:
            while True:
                lock.seek(0)
                try:
                    msvcrt.locking(lock.fileno(), msvcrt.LK_NBLCK, 1)
                except OSError:
                    if time.monotonic() > deadline:
                        raise TimeoutError("Stop not confirmed; inspect console.log; no process was killed")
                    time.sleep(.1)
                else:
                    msvcrt.locking(lock.fileno(), msvcrt.LK_UNLCK, 1)
                    break
        Service(options).host("-Stop")
        atomic_json(status, {"pid": None, "running": False, "utc": utc()})
        print("Excel question service stopped")


if __name__ == "__main__":
    main()
