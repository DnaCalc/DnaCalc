"""Run with: python -m unittest discover -s tools/excel-service -v"""
import json
import os
import base64
import hashlib
from pathlib import Path
import subprocess
import tempfile
import threading
import time
import unittest
from types import SimpleNamespace
from unittest.mock import patch

from client import Client, ServiceError, atomic_json, bits, from_bits, number, number_array, text
from policy import catalogue_names, decode, validate
from queue_bridge import operation
from service import Service, typed, typed_inputs, witness_request

ROOT = Path(__file__).resolve().parents[2]


class ProtocolTests(unittest.TestCase):
    def setUp(self):
        self.names = catalogue_names(ROOT)

    def request(self, **changes):
        value = {"id": "a" * 32, "version": 1, "created": time.time(),
                 "expires": time.time() + 60, "function": "ERF.PRECISE",
                 "rows": [[number("3e47bc15e9263272")]]}
        value.update(changes)
        return value

    def test_admission_rejects_executable_and_external_inputs(self):
        for name in ("CALL", "REGISTER", "RTD", "DDE", "WEBSERVICE", "INFO", "CELL",
                     "INDIRECT", "OFFSET", "NOW", "TODAY", "RAND", "SUM(1)", "X!SUM", "_xlfn.SUM"):
            with self.subTest(name=name), self.assertRaises(ValueError):
                validate(self.request(function=name), self.names, "a" * 32)
        for arg in (text("C:\\secret.xlsx"), text("https://example.com"), text("[book]A1"),
                    {"type": "cell", "address": "A1"}, {"type": "formula", "value": "=1"},
                    number("7ff0000000000000"), number("7ff8000000000000"),
                    {**number(1.0), "mode": "literal"}, {**number(1.0), "formula": "=CALL()"}):
            with self.subTest(arg=arg), self.assertRaises(ValueError):
                validate(self.request(rows=[[arg]]), self.names, "a" * 32)
        with self.assertRaises(ValueError):
            decode(b'{"id":1,"id":2}')
        with self.assertRaises(ValueError):
            validate(self.request(rows=[[]] * 20001), self.names, "a" * 32)
        with self.assertRaises(ValueError):
            validate(self.request(expires=time.time() - 1), self.names, "a" * 32)
        validate(self.request(rows=[[number(-0.0), text('="test"')]]), self.names, "a" * 32)

    def test_scalar_numeric_formatting_entries_are_admitted_without_array_or_link_entries(self):
        for name in ("FIXED", "DOLLAR", "TEXT", "VALUE", "NUMBERVALUE", "VALUETOTEXT", "ASC", "DBCS"):
            with self.subTest(name=name):
                request = self.request(function=name, rows=[[number(1234.5)]])
                self.assertIs(validate(request, self.names, "a" * 32), request)
        for name in ("TEXTSPLIT", "REGEXEXTRACT", "HYPERLINK", "ENCODEURL"):
            with self.subTest(name=name), self.assertRaises(ValueError):
                validate(self.request(function=name), self.names, "a" * 32)
        request = self.request(function="FIXED", rows=[[text("C:\\secret.xlsx")]])
        with self.assertRaises(ValueError):
            validate(request, self.names, "a" * 32)

    def test_base_rounding_catalogue_entries_are_admitted(self):
        expected = {"ABS", "SIGN", "INT", "TRUNC", "ROUND", "ROUNDUP", "ROUNDDOWN",
                    "MROUND", "CEILING", "FLOOR", "MOD", "QUOTIENT", "EVEN", "ODD",
                    "ECMA.CEILING"}
        self.assertTrue(expected <= self.names)
        request = self.request(function="ROUNDUP",
                               rows=[[number("0010000000000000"), number(-309.0)]])
        self.assertIs(validate(request, self.names, "a" * 32), request)
        with self.assertRaises(ValueError):
            validate(self.request(function="ROUNDUP", rows=[[text("[book]A1"), number(2.0)]]),
                     self.names, "a" * 32)

    def test_regression_selects_one_cell_and_audits_numeric_array_operands(self):
        a = number_array([[1., 2.], [3., 4.]])
        request = self.request(function="LINEST", rows=[[a, a, number(0.), number(1.)]],
                               result_index=[4, 2])
        validate(request, self.names, "a" * 32)
        prepared = witness_request(request)["rows"][0]
        self.assertEqual(prepared["formula"],
            "=INDEX(LINEST(CHOOSE({1,2;3,4},RC[1],RC[2],RC[3],RC[4]),"
            "CHOOSE({1,2;3,4},RC[5],RC[6],RC[7],RC[8]),RC[9],RC[10]),4,2)")
        self.assertEqual(len(prepared["args"]), 10)
        self.assertEqual(prepared["argument_modes"], ["cell"]*10)
        self.assertEqual(typed_inputs(request["rows"][0], prepared["args"]), request["rows"][0])
        for changes in ({"result_index": None}, {"result_index": [0, 1]},
                        {"result_index": [1, True]}, {"function": "SUM"},
                        {"rows": [[number_array([[1.], [2., 3.]])]]},
                        {"rows": [[{"type": "array", "values": [[text("=CALL()")]]}]]},
                        {"rows": [[{"type": "array", "values": [[number("7ff0000000000000")]]}]]},
                        {"rows": [[number_array([[1.]*32]*5)]]}):
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                validate({**request, **changes}, self.names, "a" * 32)
        with self.assertRaises(ValueError):
            validate(self.request(rows=[[a]]), self.names, "a" * 32)

    def test_ets_questions_accept_numeric_arrays_without_result_selection(self):
        a = number_array([[1.], [2.], [3.], [4.]])
        for name in ("FORECAST.ETS", "FORECAST.ETS.CONFINT",
                     "FORECAST.ETS.SEASONALITY", "FORECAST.ETS.STAT"):
            request = self.request(function=name, rows=[[number(5.), a, a]])
            self.assertIs(validate(request, self.names, "a" * 32), request)
            prepared = witness_request(request)["rows"][0]
            self.assertTrue(prepared["formula"].startswith("=" + name + "("))
            self.assertEqual(typed_inputs(request["rows"][0], prepared["args"]), request["rows"][0])
            with self.assertRaises(ValueError):
                validate({**request, "result_index": [1, 1]}, self.names, "a" * 32)
            with self.assertRaises(ValueError):
                validate({**request, "rows": [[{"type": "array", "values": [[text("=CALL()")]]}]]},
                         self.names, "a" * 32)
        with self.assertRaises(ValueError):
            validate(self.request(function="SUM", rows=[[a]]), self.names, "a" * 32)

    def test_queue_and_adaptive_helpers_retain_exact_inputs(self):
        with tempfile.TemporaryDirectory() as directory:
            client = Client(directory, timeout=3)
            stop = threading.Event()
            observed = []

            def responder():
                while not stop.is_set():
                    for path in (Path(directory) / "in").glob("*.json"):
                        request = json.loads(path.read_text())
                        observed.append(request)
                        values = [row[0]["bits"] for row in request["rows"]]
                        response = {"version": 1, "id": request["id"], "ok": True,
                                    "rows": [number(value) for value in values]}
                        atomic_json(Path(directory) / "out" / path.name, response)
                        path.unlink()
                    stop.wait(.005)

            thread = threading.Thread(target=responder)
            thread.start()
            try:
                start = "3e47bc15e9263260"
                run = client.consecutive_run("ERF.PRECISE", start, 32)
                self.assertEqual(run["rows"][0]["bits"], start)
                edge = client.bisect_edge("ERF.PRECISE", start, "3e47bc15e9263280",
                                          lambda x, answer: int(answer["bits"], 16) > 0x3e47bc15e9263272)
                self.assertEqual(edge["low"], "3e47bc15e9263272")
                self.assertEqual(edge["high"], "3e47bc15e9263273")
                self.assertEqual(len(observed), 7)
                edge = client.bisect_edge("ERF.PRECISE", start, "3e47bc15e9263280",
                                          lambda x, answer: None if x == "3e47bc15e9263270" else int(x, 16) > 0x3e47bc15e9263272)
                self.assertEqual(edge["low"], "3e47bc15e9263272")
                self.assertEqual(edge["high"], "3e47bc15e9263273")
                self.assertEqual(bits(from_bits("8000000000000000")), "8000000000000000")
            finally:
                stop.set()
                thread.join()

    def test_failure_and_missing_service_are_bounded(self):
        with tempfile.TemporaryDirectory() as directory:
            client = Client(directory, timeout=.05)
            with self.assertRaises(TimeoutError):
                client.ask("SUM", [[number(1.0)]])
            self.assertEqual(len(list((Path(directory) / "in").glob("*.json"))), 1)

    def test_delivery_checks_contents_and_publishes_before_removal(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            request = self.request()
            source = root / "in" / (request["id"] + ".json")
            atomic_json(source, request)
            item = operation(root, {"op": "take"})
            digest = hashlib.sha256(base64.b64decode(item["raw"])).hexdigest()
            answer = {"request_sha256": digest}
            request["function"] = "SUM"
            atomic_json(source, request)
            self.assertFalse(operation(root, {"op": "put", "id": item["id"], "answer": answer}))
            self.assertTrue(source.exists())
            item = operation(root, {"op": "take"})
            answer["request_sha256"] = hashlib.sha256(base64.b64decode(item["raw"])).hexdigest()
            self.assertTrue(operation(root, {"op": "put", "id": item["id"], "answer": answer}))
            self.assertFalse(source.exists())
            self.assertEqual(json.loads((root / "out" / source.name).read_text()), answer)

    def test_queue_takes_oldest_arrival_before_newer_lower_uuids(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            older = root / "in" / ("f" * 32 + ".json")
            newer = root / "in" / ("0" * 32 + ".json")
            atomic_json(older, self.request(id="f" * 32))
            atomic_json(newer, self.request(id="0" * 32))
            os.utime(older, ns=(1_000_000_000, 1_000_000_000))
            os.utime(newer, ns=(2_000_000_000, 2_000_000_000))
            taken = operation(root, {"op": "take"})
            self.assertEqual(taken["id"], "f" * 32)
            self.assertTrue(older.exists())
            self.assertTrue(newer.exists())

    def test_interruption_replay_and_unsafe_request_never_evaluate(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "log").mkdir()
            service = Service(SimpleNamespace(state=root, local_queue=root / "queue", hosts=[]))
            def forbidden(*args):
                self.fail("Excel must not be invoked")
            service.evaluate = forbidden
            request = self.request(function="CALL")
            item = {"id": request["id"], "raw": base64.b64encode(json.dumps(request).encode()).decode(), "oversized": False}
            answer = service.answer("local", item)
            self.assertFalse(answer["ok"])
            self.assertEqual(service.answer("local", item), answer)
            request["function"] = "SUM"
            item["raw"] = base64.b64encode(json.dumps(request).encode()).decode()
            conflict = service.answer("local", item)
            self.assertIn("reused", conflict["error"])
            # A durable question without an answer represents a service interruption.
            request["id"] = "b" * 32
            item["id"] = request["id"]
            item["raw"] = base64.b64encode(json.dumps(request).encode()).decode()
            pending = root / "requests/local" / request["id"]
            pending.mkdir(parents=True)
            (pending / "question.json").write_text(json.dumps(request))
            interrupted = service.answer("local", item)
            self.assertFalse(interrupted["ok"])
            self.assertIn("interrupted", interrupted["error"])
            records = [json.loads(line) for line in (root / "log/questions.jsonl").read_text().splitlines()]
            self.assertEqual(sum(row["kind"] == "answer" for row in records), 2)

    def test_literal_quotes_are_escaped_and_utf16_answers_survive(self):
        prepared = witness_request(self.request(rows=[[text('"=SUM(1,2)"', literal=True)]]))
        self.assertEqual(prepared["rows"][0]["formula"], '=ERF.PRECISE("""=SUM(1,2)""")')
        self.assertEqual(typed({"text": "0041d83dde000000"}), {"type": "text", "value": "A😀\x00"})

    @unittest.skipUnless(os.name == "nt", "the service owns a Windows process lock")
    def test_stop_stat_1450_keeps_polling_without_repeating_answers_or_restarting_host(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            service = Service(SimpleNamespace(state=root, local_queue=root / "queue", hosts=[]))
            host_calls, evaluations, failures = [], [], []
            inject, observed = threading.Event(), threading.Event()
            original_exists, original_log = Path.exists, service.log

            def exists(path):
                if path == root / "stop" and inject.is_set() and not observed.is_set():
                    observed.set()
                    error = OSError(1450, "Insufficient system resources exist to complete the requested service")
                    error.winerror = 1450
                    raise error
                return original_exists(path)

            def log(kind, **data):
                if kind == "resource-retry":
                    error = OSError(1450, "Injected log resource shortage")
                    error.winerror = 1450
                    raise error
                return original_log(kind, **data)

            def evaluate(request, evidence):
                evaluations.append(request["id"])
                return {"version": 1, "id": request["id"], "ok": True, "rows": [number(1.0)]}

            def run():
                try:
                    service.run()
                except BaseException as error:
                    failures.append(error)

            service.host = lambda *args: host_calls.append(args)
            service.evaluate, service.log = evaluate, log
            thread = threading.Thread(target=run)
            with patch.object(Path, "exists", exists):
                thread.start()
                try:
                    client = Client(root / "queue", timeout=8)
                    first = client.ask("SUM", [[number(1.0)]])
                    inject.set()
                    self.assertTrue(observed.wait(3), "stop stat fault was not injected")
                    second = client.ask("SUM", [[number(1.0)]])
                    self.assertTrue(thread.is_alive())
                    self.assertEqual(evaluations, [first["id"], second["id"]])
                    self.assertEqual(host_calls, [("-Stop",), ("-Start", "-NoSaveBaseline")])
                    for answer in (first, second):
                        saved = root / "requests/local" / answer["id"] / "answer.json"
                        self.assertEqual(json.loads(saved.read_text()), answer)
                finally:
                    (root / "stop").touch()
                    thread.join(timeout=5)
            self.assertFalse(thread.is_alive())
            self.assertEqual(failures, [])
            events = [json.loads(line) for line in (root / "log/questions.jsonl").read_text().splitlines()]
            self.assertEqual(sum(row["kind"] == "resource-recovered" for row in events), 1)

    def test_stop_stat_other_errors_remain_errors_and_a_real_stop_is_preserved(self):
        with tempfile.TemporaryDirectory() as directory:
            service = Service(SimpleNamespace(state=Path(directory), local_queue=Path(directory), hosts=[]))
            stop = Path(directory) / "stop"
            with patch.object(Path, "exists", side_effect=PermissionError("denied")):
                with self.assertRaises(PermissionError):
                    service.stop_requested(stop)
            stop.touch()
            self.assertTrue(service.stop_requested(stop))


class SessionShutdownRequestTests(unittest.TestCase):
    """The session host in a private temporary directory; no Excel process is launched."""

    def submit(self, directory, *args):
        return subprocess.run(["pwsh", "-NoLogo", "-NoProfile", "-NonInteractive", "-File",
                               str(ROOT / "tools/excel-session/submit.ps1"), "-SessionDirectory",
                               str(directory), *args], capture_output=True, encoding="utf-8",
                              errors="replace", timeout=120, creationflags=subprocess.CREATE_NO_WINDOW)

    def test_start_removes_a_stale_request_and_stop_consumes_its_own(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            request = directory / "shutdown.request"
            request.write_text("stop")
            started = self.submit(directory, "-Start")
            try:
                self.assertEqual(started.returncode, 0, started.stdout + started.stderr)
                self.assertFalse(request.exists())
                self.assertIsNone(json.loads(started.stdout)["excel"])
            finally:
                stopped = self.submit(directory, "-Stop")
            self.assertEqual(stopped.returncode, 0, stopped.stdout + stopped.stderr)
            self.assertFalse(request.exists(), "-Stop must wait for the host and consume the request")
            again = self.submit(directory, "-Stop")
            self.assertEqual(again.returncode, 0, again.stdout + again.stderr)
            self.assertFalse(request.exists(), "-Stop with no live host must leave no request")

    def test_start_fails_loudly_naming_the_path_when_the_request_cannot_be_removed(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            request = directory / "shutdown.request"
            request.write_text("stop")
            # An open handle without delete sharing blocks removal on Windows.
            holder = subprocess.Popen(["pwsh", "-NoLogo", "-NoProfile", "-NonInteractive", "-Command",
                                       f"$s=[IO.File]::Open('{request}','Open','Read','Read');"
                                       "'held';Start-Sleep 25"],
                                      stdout=subprocess.PIPE, encoding="utf-8",
                                      creationflags=subprocess.CREATE_NO_WINDOW)
            try:
                self.assertEqual(holder.stdout.readline().strip(), "held")
                started = self.submit(directory, "-Start")
            finally:
                holder.kill()
                holder.wait()
            self.assertNotEqual(started.returncode, 0)
            output = started.stdout + started.stderr
            self.assertIn("Leftover shutdown request could not be removed", output)
            self.assertIn(str(request.resolve()), output)

    def test_start_does_not_parse_retained_running_jobs_that_already_have_receipts(self):
        """The cause of the 45 second start failure: every retained running/*.json was parsed."""
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            for name in ("running", "completed"):
                (directory / name).mkdir()
            for index in range(5):
                ident = f"job{index}"
                (directory / "running" / f"{ident}.json").write_text("{ not json")
                (directory / "completed" / ident).mkdir()
                (directory / "completed" / ident / "receipt.json").write_text("{}")
            started = self.submit(directory, "-Start")
            try:
                self.assertEqual(started.returncode, 0, started.stdout + started.stderr)
            finally:
                self.submit(directory, "-Stop")


if __name__ == "__main__":
    unittest.main()
