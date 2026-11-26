"""The VBA client through the socket broker and its stand-in worker."""
import unittest
from unittest.mock import patch

from test_touchstone import ONE, TWO, number, start_broker, vba_group

from touchstone import client, protocol


CASE = vba_group()["v"][0]


class VbaClientTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.broker, cls.state, cls.stop = start_broker(2, {"admission": "policy"})

    @classmethod
    def tearDownClass(cls):
        cls.stop()

    def connect(self):
        conn = client.Connection.open(endpoint=self.state / "endpoint.json",
                                      socket_path=self.state / "not-a-socket",
                                      reconnect=False)
        self.addCleanup(conn.close)
        return conn

    def test_vba_case_returns_ordered_calls_cells_and_events(self):
        conn = self.connect()
        reply = conn.vba_case(CASE, timeout=10)
        error_case = dict(CASE, calls=[{"procedure": "Probes.Probe", "args": [number(TWO)]}])
        error_reply = conn.vba_case(error_case, timeout=10)
        self.assertEqual(len(reply["source_sha256"]), 64)
        self.assertEqual(reply["source_sha256"], error_reply["source_sha256"])
        self.assertNotEqual(reply["request_sha256"], error_reply["request_sha256"])
        self.assertEqual(reply["cpu"], "test cpu")
        self.assertEqual(reply["excel_build"], "0")
        row, = reply["rows"]
        self.assertEqual(row["type"], "vba_case")
        self.assertEqual(row["calls"], [{"return": number(ONE)}])
        self.assertEqual(row["sheet_state"], [{"at": "Data!A1", "value": number(ONE)}])
        self.assertEqual(row["event_log"], [{"at": "Data!B1", "value": {"type": "text", "value": "Change"}}])
        self.assertEqual(error_reply["rows"][0]["calls"], [{"err": {"number": 5, "description": "Fake VBA error"}}])

    def test_vba_case_preserves_the_service_refusal_code(self):
        conn = self.connect()
        case = dict(CASE, modules=[dict(CASE["modules"][0],
                                        source="Public Function Probe()\nShell \"calc\"\nEnd Function\n")])
        with self.assertRaises(client.ServiceError) as caught:
            conn.vba_case(case, timeout=10)
        self.assertEqual(caught.exception.response["code"], "refused")
        self.assertIn("Shell", str(caught.exception))

    def test_vba_deadline_keeps_the_worker_reason(self):
        conn = self.connect()
        case = dict(CASE, calls=[{"procedure": "Probes.Probe", "args": [number("402a000000000000")]}])
        with self.assertRaises(client.ServiceError) as caught:
            conn.vba_case(case, timeout=10)
        self.assertEqual(caught.exception.response.get("code"), "excel-deadline")
        self.assertIn("excel-deadline", str(caught.exception))

    def test_vba_answer_keeps_err_distinct_from_a_return(self):
        row = protocol.answer_to_typed("v", {
            "calls": [{"return": {"ty": "n", "v": ONE}},
                      {"err": {"number": 5, "description": "Probe raised 5"}}],
            "sheet_state": [{"at": "Data!A1", "ty": "t", "v": "changed"}],
            "event_log": [{"at": "Data!B1", "ty": "z", "v": None}],
        })
        self.assertEqual(row, {
            "type": "vba_case",
            "calls": [{"return": number(ONE)}, {"err": {"number": 5, "description": "Probe raised 5"}}],
            "sheet_state": [{"at": "Data!A1", "value": {"type": "text", "value": "changed"}}],
            "event_log": [{"at": "Data!B1", "value": {"type": "blank"}}],
        })
