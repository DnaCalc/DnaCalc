"""Exercise the existing drift tool's selection of whole-array replies."""
import contextlib
import importlib.util
import io
import json
import sys
import tempfile
from unittest import mock
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("witness_drift", Path(__file__).with_name("witness-drift.py"))
drift = importlib.util.module_from_spec(spec)
spec.loader.exec_module(drift)

spec = importlib.util.spec_from_file_location("bank_coverage", Path(__file__).with_name("bank-coverage.py"))
coverage = importlib.util.module_from_spec(spec)
spec.loader.exec_module(coverage)


class CaptureArgumentTests(unittest.TestCase):
    def test_choose_materialization_preserves_the_range_distinction(self):
        # Excel20527: TOCOL(CHOOSE({1;2},blank,6),1) starts with zero;
        # TOCOL(the same two source cells as a range,1) starts with six.
        cells = [[coverage.blank(), coverage.number(6.0)]]
        self.assertEqual(coverage.witness_arg(coverage.cell_array(cells)),
                         [["0x0000000000000000", "0x4018000000000000"]])
        self.assertEqual(coverage.witness_arg(coverage.cell_range(cells)),
                         {"range": [[None, "0x4018000000000000"]]})
        self.assertIsNone(coverage.witness_arg(coverage.blank()))
        self.assertEqual(coverage.witness_arg(coverage.cell_array([
            [coverage.text(""), coverage.boolean(False), coverage.error("#N/A")]
        ])), [["", False, {"error": "NA"}]])

    def test_mixed_worker_revalidation_keeps_each_rows_environment(self):
        pending = mock.Mock()
        pending.env_ids = {"worker-a", "worker-b"}
        pending.env_by_position = ["worker-b", "worker-a"]
        pending.envs = {"worker-a": {"build": "20527"}, "worker-b": {"build": "20527"}}
        rows = [[coverage.number(1.0)], [coverage.number(2.0)]]
        pending.result.return_value = {"rows": [rows[0][0], rows[1][0]], "inputs": rows}
        client = mock.Mock(timeout=30)
        client._socket.return_value.submit_rows.return_value = pending
        with self.assertRaisesRegex(ValueError, "one recorded worker"):
            coverage.ask_checked(client, "ABS", rows)
        reply = coverage.ask_checked(client, "ABS", rows, row_provenance=True)
        self.assertEqual(reply["environment_ids_by_row"], ["worker-b", "worker-a"])
        self.assertEqual(reply["worker_environments"], pending.envs)
        pending.env_by_position[0] = None
        with self.assertRaisesRegex(ValueError, "missing per-row"):
            coverage.ask_checked(client, "ABS", rows, row_provenance=True)


class SpillSelectionTests(unittest.TestCase):
    def test_zero_axis_selects_the_requested_cell(self):
        cells = [[{"type": "number", "bits": f"{r * 4 + c:016x}"} for c in range(4)] for r in range(4)]
        value = {"type": "array", "rows": 4, "cols": 4, "values": cells}
        self.assertEqual(drift.select_spill(value, [0, 1]), cells[0][0])
        self.assertEqual(drift.select_spill(value, [0, 2]), cells[0][1])
        self.assertEqual(drift.select_spill(value, [0, 3]), cells[0][2])
        self.assertIsNone(drift.select_spill(value, [0, 5]))
        self.assertIsNone(drift.select_spill({"type": "array", "rows": 1, "cols": 2, "values": [[cells[0][0]]]}, [0, 1]))

    def test_replay_submits_a_whole_spill_and_checks_input_readback(self):
        cells = [[{"type": "number", "bits": f"{r * 3 + c:016x}"} for c in range(3)] for r in range(3)]
        observed = {"type": "array", "rows": 3, "cols": 3, "values": cells}
        requests = []

        class Pending:
            rid = "test-request"
            seconds = 0.1
            env_ids = ["env"]
            env_by_position = ["env"]
            tamper = False

            def __init__(self, rows):
                self.rows = rows

            def result(self, timeout):
                inputs = json.loads(json.dumps(self.rows))
                if self.tamper:
                    inputs[0][0]["values"][0][0]["bits"] = "3ff0000000000000"
                return {"rows": [observed], "inputs": inputs}

        class Connection:
            envs = {"env": {"env": {"build": "20527", "cpu": "test"}}}

            def submit_rows(self, function, rows, **options):
                requests.append((function, options))
                return Pending(rows)

        class Client:
            def __init__(self, **options):
                self.connection = Connection()

            def _socket(self):
                return self.connection

            def close(self):
                pass

        with tempfile.TemporaryDirectory() as folder:
            source = Path(folder) / "source.jsonl"
            output = Path(folder) / "result"
            matrix = [[f"0x{r * 3 + c:016x}" for c in range(3)] for r in range(3)]
            source.write_text(json.dumps({"function": "MINVERSE", "args": [matrix],
                "result_index": [0, 3], "expected": "0x0000000000000002", "exclusion": None}) + "\n")
            argv = ["witness-drift.py", str(source), str(output), "--whole-array-zero-axis"]
            with mock.patch.object(drift, "Client", Client), mock.patch.object(sys, "argv", argv), contextlib.redirect_stdout(io.StringIO()):
                drift.main()
            result = json.loads((output / "summary.json").read_text())
            self.assertEqual(result["functions"]["MINVERSE"]["agree"], 1)
            self.assertEqual(requests[0][0], "MINVERSE")
            self.assertTrue(requests[0][1]["spill"])
            self.assertIsNone(requests[0][1]["result_index"])
            self.assertTrue(requests[0][1]["echo"])
            Pending.tamper = True
            bad_output = Path(folder) / "changed-input"
            argv[2] = str(bad_output)
            with mock.patch.object(drift, "Client", Client), mock.patch.object(sys, "argv", argv), contextlib.redirect_stdout(io.StringIO()):
                drift.main()
            changed = json.loads((bad_output / "summary.json").read_text())
            self.assertEqual(changed["functions"]["MINVERSE"]["input_changed"], 1)
            self.assertNotIn("agree", changed["functions"]["MINVERSE"])

    def test_scalar_error_does_not_invent_an_unobserved_cell(self):
        value = {"type": "error", "code": "#NUM!"}
        self.assertEqual(drift.select_spill(value, [0, 1]), value)
        self.assertIsNone(drift.select_spill(value, [0, 2]))
        self.assertTrue(drift.use_whole_array({"function": "MINVERSE", "result_index": [0, 2]}, True))
        self.assertFalse(drift.use_whole_array({"function": "MINVERSE", "result_index": [0, 2]}, False))
        self.assertFalse(drift.use_whole_array({"function": "ABS", "result_index": [0, 2]}, True))


if __name__ == "__main__":
    unittest.main()
