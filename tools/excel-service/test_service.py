"""Run with: python -m unittest discover -s tools/excel-service -v"""
import json
from pathlib import Path
import tempfile
import threading
import time
import unittest

from client import Client, ServiceError, atomic_json, bits, from_bits, number, text
from policy import catalogue_names, decode, validate

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


if __name__ == "__main__":
    unittest.main()
