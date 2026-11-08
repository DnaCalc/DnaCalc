"""Run with: python -m unittest discover -s tools/excel-service/touchstone -p "test_*.py" -v

Hermetic: no Excel.  The broker tests drive a stand-in worker (fake_worker.py) over the real frame protocol.
"""
import asyncio
import json
from pathlib import Path
import sys
import tempfile
import threading
import time
import unittest
from types import SimpleNamespace

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE.parent))

import batcher  # noqa: E402
import protocol  # noqa: E402
import v1  # noqa: E402

ROOT = HERE.parents[2]


def number(bits):
    return {"type": "number", "bits": bits}


ONE = "3ff0000000000000"
TWO = "4000000000000000"


class ProtocolTests(unittest.TestCase):
    def test_rows_group_by_shape_and_keep_positions(self):
        rows = [[number(ONE)], [{"type": "text", "value": "a", "mode": "cell"}], [number(TWO)],
                [{"type": "boolean", "value": True, "mode": "literal"}]]
        groups = protocol.rows_to_groups("F", rows)
        self.assertEqual([g["k"] for g in groups], ["n", "s", "B"])
        self.assertEqual(list(protocol.positions(groups[0])), [0, 2])
        self.assertEqual(groups[1]["o"], 1)
        back = []
        for g in groups:
            back.extend(zip(protocol.positions(g), protocol.group_to_rows(g)))
        self.assertEqual([r for _, r in sorted(back, key=lambda x: x[0])][0], [number(ONE)])

    def test_arrays_flatten_inline_and_split_keeps_attached_positions(self):
        a = {"type": "array", "values": [[number(ONE), number(TWO)], [number(TWO), number(ONE)]]}
        groups = protocol.rows_to_groups("LINEST", [[a, number(ONE)]] * 5, [1, 1])
        self.assertEqual(groups[0]["k"], "a2x2,n")
        self.assertEqual(protocol.group_width(groups[0]), 5)
        self.assertEqual(len(groups[0]["v"]), 25)
        parts = protocol.split_group(groups[0], 2)
        self.assertEqual([p["n"] for p in parts], [2, 2, 1])
        self.assertEqual([p["o"] for p in parts], [0, 2, 4])
        self.assertEqual(protocol.group_to_rows(parts[2])[0][0], a)

    def test_frames_roundtrip_with_and_without_compression(self):
        payload = {"id": "x" * 32, "groups": [{"f": "ABS", "k": "n", "n": 300, "v": [ONE] * 300}]}
        for compress in (False, True):
            data = protocol.pack(protocol.SUBMIT, payload, compress) + protocol.pack(protocol.PING, {"t": 1})
            reader = protocol.FrameReader()
            frames = []
            for i in range(0, len(data), 7):          # arbitrary chunking
                frames.extend(reader.feed(data[i:i + 7]))
            self.assertEqual(frames, [(protocol.SUBMIT, payload), (protocol.PING, {"t": 1})])
        self.assertLess(len(protocol.pack(protocol.SUBMIT, payload, True)), len(protocol.pack(protocol.SUBMIT, payload)) // 3)

    def test_submit_frames_stay_small_and_flag_the_last(self):
        groups = protocol.rows_to_groups("ABS", [[number(ONE)]] * 20000)
        frames = protocol.submit_frames("i" * 32, groups, {"calls": 20000})
        self.assertGreater(len(frames), 3)
        self.assertEqual([f["last"] for f in frames], [False] * (len(frames) - 1) + [True])
        self.assertTrue(all(len(protocol.dumps(f)) < protocol.MAX_FRAME for f in frames))
        self.assertEqual(sum(g["n"] for f in frames for g in f["groups"]), 20000)
        self.assertIn("opts", frames[0])


class FakeConn:
    def __init__(self, origin):
        self.origin = origin


def piece(origin, rid, f, calls, priority="bulk", k="n"):
    groups = protocol.with_positions([{"f": f, "k": k, "n": calls, "v": [ONE] * calls}])
    return batcher.Piece(FakeConn(origin), rid, 0, groups, True, priority)


class BatcherTests(unittest.TestCase):
    def test_interactive_goes_first_and_alone(self):
        b = batcher.Batcher()
        b.add_piece(piece("a", "bulk", "ABS", 3000))
        b.add_piece(piece("b", "inter", "SQRT", 3, "interactive"))
        batch = b.next_batch()
        self.assertTrue(batch.interactive)
        self.assertEqual([g["f"] for g in batch.groups], ["SQRT"])
        self.assertEqual(batch.calls, 3)
        self.assertEqual(b.next_batch().groups[0]["f"], "ABS")

    def test_items_of_one_shape_merge_into_one_group(self):
        b = batcher.Batcher()
        for i in range(4):
            b.add_piece(piece("a", f"r{i}", "ABS", 50))
        batch = b.next_batch()
        self.assertEqual(len(batch.groups), 1)
        self.assertEqual(batch.groups[0]["n"], 200)
        self.assertEqual([s.offset for s in batch.slices], [0, 50, 100, 150])

    def test_deficit_round_robin_shares_a_batch_between_origins(self):
        b = batcher.Batcher()
        b.add_piece(piece("a", "ra", "ABS", 20000))
        b.add_piece(piece("b", "rb", "SQRT", 20000))
        seen = {}
        for _ in range(4):
            batch = b.next_batch()
            for s in batch.slices:
                seen[s.item.piece.origin] = seen.get(s.item.piece.origin, 0) + s.item.n
        self.assertGreater(min(seen.values()), 0.3 * sum(seen.values()))

    def test_a_big_group_is_cut_and_every_piece_still_completes_once(self):
        b = batcher.Batcher()
        p = piece("a", "r", "ABS", 30000)
        b.add_piece(p)
        done, calls = 0, 0
        while b.has_work():
            batch = b.next_batch()
            for s in batch.slices:
                p.remaining -= 1
                calls += s.item.n
        self.assertEqual(calls, 30000)
        self.assertEqual(p.remaining, 0)

    def test_isolated_function_runs_as_a_one_call_batch(self):
        b = batcher.Batcher()
        b.isolate.add("CRASHY")
        b.add_piece(piece("a", "r1", "CRASHY", 5))
        b.add_piece(piece("a", "r2", "ABS", 10))
        sizes = []
        while b.has_work():
            batch = b.next_batch()
            sizes.append((batch.groups[0]["f"], batch.calls))
        self.assertEqual([s for s in sizes if s[0] == "CRASHY"], [("CRASHY", 1)] * 5)

    def test_cost_learning_ignores_small_batches_and_stays_bounded(self):
        b = batcher.Batcher()
        small = batcher.Batch()
        small.groups, small.calls = [{"f": "X", "n": 5, "k": "n", "v": []}], 5
        b.learn(small, 500.0)
        self.assertNotIn("X", b.costs)
        big = batcher.Batch()
        big.groups, big.calls = [{"f": "X", "n": 5000, "k": "n", "v": []}], 5000
        for _ in range(30):
            b.learn(big, 1e9)
        self.assertLessEqual(b.costs["X"], 2000.0)

    def test_requeue_puts_work_back_in_front(self):
        b = batcher.Batcher()
        b.add_piece(piece("a", "r1", "ABS", 10))
        b.add_piece(piece("a", "r2", "SQRT", 10))
        first = b.next_batch()
        b.requeue(first)
        self.assertEqual(b.next_batch().groups[0]["f"], first.groups[0]["f"])


class V1Tests(unittest.TestCase):
    def request(self, **changes):
        value = {"version": 1, "id": "a" * 32, "function": "ABS", "rows": [[number(ONE)]],
                 "created": time.time(), "expires": time.time() + 60}
        value.update(changes)
        return value

    def test_parse_checks_identity_lifetime_and_duplicate_keys(self):
        self.assertEqual(v1.parse_request(json.dumps(self.request()).encode(), "a" * 32)[0], "ABS")
        for bad in (self.request(id="b" * 32), self.request(version=2), self.request(expires=time.time() - 5),
                    self.request(expires=time.time() + 5000), {"version": 1}):
            with self.assertRaises(ValueError):
                v1.parse_request(json.dumps(bad).encode(), "a" * 32)
        with self.assertRaises(ValueError):
            v1.parse_request(b'{"id":1,"id":2}', "a" * 32)

    def test_reply_rebuilds_typed_rows_and_inputs_in_request_order(self):
        rows = [[number(ONE)], [{"type": "text", "value": "x", "mode": "cell"}], [number(TWO)]]
        frames = v1.submit_frames("c" * 32, "F", rows)
        reply = v1.Reply("c" * 32, "F", rows, None, "sha", {"e1": {"env": {"build": "7", "version": "16.0"}}}, "host")
        for frame in frames:
            for g in frame["groups"]:
                positions = list(protocol.positions(g))
                answer = {"ty": "".join("n" if g["k"] == "n" else "t" for _ in positions),
                          "v": [ONE if g["k"] == "n" else "x" for _ in positions], "env": "e1",
                          "ity": ("n" if g["k"] == "n" else "t") * len(positions), "iv": list(g["v"])}
                answer["i"] = positions
                reply.add({"g": [answer], "env": "e1"})
        built = reply.build()
        self.assertTrue(built["ok"], built)
        self.assertEqual([r["type"] for r in built["rows"]], ["number", "text", "number"])
        self.assertEqual(built["inputs"][2][0], number(TWO))
        self.assertEqual(built["excel_build"], "7")

    def test_a_changed_input_makes_the_reply_a_failure(self):
        reply = v1.Reply("d" * 32, "F", [[number(ONE)]], None, "sha", {}, "host")
        reply.add({"g": [{"ty": "u", "v": [{"reason": "input-changed", "read_back": []}], "o": 0}]})
        self.assertFalse(reply.build()["ok"])


class AdmissionTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        import policy
        cls.policy = policy
        cls.admission = policy.admission(ROOT)

    def group(self, **changes):
        g = {"f": "ERF.PRECISE", "k": "n", "n": 2, "v": [ONE, TWO]}
        g.update(changes)
        return g

    def test_accepts_the_catalogue_and_refuses_everything_else(self):
        self.admission.check(self.group())
        for bad in (self.group(f="CALL"), self.group(f="INDIRECT"), self.group(f="NOW"), self.group(f="SUM(1)"),
                    self.group(v=["7ff8000000000000", ONE]), self.group(v=[ONE]), self.group(k="x"),
                    self.group(k="n,n", v=[ONE] * 4, f="LEN", x=[1, 1]), self.group(n=0, v=[]),
                    self.group(f="LEN", k="s", n=1, v=["C:/secret.xlsx"]), self.group(f="LEN", k="s", n=1, v=["[book]A1"]),
                    self.group(f="LEN", k="s", n=1, v=["https://x.example"]), self.group(f="LEN", k="s", n=1, v=["a\u0001b"]),
                    self.group(f="LEN", k="a1x2", n=1, v=[ONE, TWO]), self.group(k="a40x40", f="LINEST", n=1, v=[ONE] * 1600)):
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                self.admission.check(bad)

    def test_regression_needs_a_bounded_result_index_and_arrays(self):
        g = {"f": "LINEST", "k": "a3x1,a3x1,b,b", "n": 1, "x": [4, 2],
             "v": [ONE, TWO, ONE, ONE, TWO, ONE, False, True]}
        self.admission.check(g)
        for bad in ({**g, "x": None}, {**g, "x": [9, 9]}, {**g, "f": "SUM"}):
            if bad["x"] is None:
                del bad["x"]
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                self.admission.check(bad)


def start_broker(workers=2, extra=None):
    """A broker over fake workers, in a thread.  Returns (broker, state_dir, stop_function)."""
    import broker as broker_module
    temporary = tempfile.TemporaryDirectory()
    state = Path(temporary.name)
    options = SimpleNamespace(state=state, workers=workers, local_queue=state / "queue", hosts=[], admission=None,
                              worker_command=[sys.executable, str(HERE / "fake_worker.py")], **(extra or {}))
    instance = broker_module.Broker(options)
    instance.cpu_name = lambda: "test cpu"
    failure = []

    def run():
        try:
            asyncio.run(instance.run())
        except BaseException as error:    # pragma: no cover
            failure.append(error)

    thread = threading.Thread(target=run, daemon=True)
    thread.start()
    deadline = time.monotonic() + 40
    while not (state / "endpoint.json").exists():
        if failure or time.monotonic() > deadline:
            raise RuntimeError(f"broker did not start: {failure}")
        time.sleep(0.05)

    def stop():
        (state / "stop").touch()
        thread.join(30)
        temporary.cleanup()

    return instance, state, stop


@unittest.skipUnless(sys.platform == "win32", "Native-only: the broker holds a Windows process lock")
class BrokerTests(unittest.TestCase):
    def connect(self, state, name="t"):
        from touchstone import client
        return client.Connection.open(endpoint=state / "endpoint.json", socket_path=state / "none.sock", name=name)

    def test_answers_arrive_in_request_order_with_the_environment(self):
        broker, state, stop = start_broker()
        try:
            conn = self.connect(state)
            rows = [[number(f"{0x3ff0000000000000 + i:016x}")] for i in range(5000)]
            reply = conn.submit_rows("ID", rows, echo=True).result(30)
            self.assertEqual([r["bits"] for r in reply["rows"]], [row[0]["bits"] for row in rows])
            self.assertEqual(reply["inputs"][7][0], rows[7][0])
            self.assertEqual(reply["excel_build"], "0")
            self.assertIn("fake00000001", broker.envs.known)
        finally:
            stop()

    def test_refusal_is_typed_and_logged_not_evaluated(self):
        _, state, stop = start_broker()
        try:
            conn = self.connect(state)
            from touchstone import client
            # the open admission only checks structure: a count that does not match the shape is refused
            bad = {"f": "ID", "k": "n", "n": 3, "v": [ONE]}
            with self.assertRaises(client.ServiceError) as caught:
                conn.submit_groups([bad]).raw(10)
            self.assertIn("refused", str(caught.exception))
            waits = (state / "log" / "waits.jsonl").read_text()
            self.assertIn("refused-kind", waits)
        finally:
            stop()

    def test_a_crash_fails_only_its_batch_and_the_worker_comes_back_once(self):
        broker, state, stop = start_broker(workers=1)
        try:
            from touchstone import client
            conn = self.connect(state)
            with self.assertRaises(client.ServiceError) as caught:
                conn.submit_groups([{"f": "CRASH", "k": "n", "n": 1, "v": [ONE]}]).raw(30)
            self.assertIn("excel-crashed", str(caught.exception))
            deadline = time.monotonic() + 15
            while broker.slots[0].state != "idle" and time.monotonic() < deadline:
                time.sleep(0.05)
            letters, values = conn.submit_groups([{"f": "ID", "k": "n", "n": 2, "v": [ONE, TWO]}]).raw(30)
            self.assertEqual(values, [ONE, TWO])
            self.assertEqual(broker.slots[0].crashes, 1)
        finally:
            stop()

    def test_a_worker_that_fails_to_start_stays_down_and_the_pool_carries_on(self):
        import os
        os.environ["TOUCHSTONE_TEST_FAIL_START"] = "w2"
        try:
            broker, state, stop = start_broker(workers=2)
        finally:
            del os.environ["TOUCHSTONE_TEST_FAIL_START"]
        try:
            self.assertEqual([s.state for s in broker.slots], ["idle", "down"])
            conn = self.connect(state)
            letters, values = conn.submit_groups([{"f": "ID", "k": "n", "n": 1, "v": [ONE]}]).raw(30)
            self.assertEqual(values, [ONE])
            self.assertIn("worker-down", (state / "log" / "waits.jsonl").read_text())
        finally:
            stop()

    def test_interactive_calls_are_not_stuck_behind_bulk(self):
        broker, state, stop = start_broker(workers=2)
        try:
            conn = self.connect(state)
            bulk = conn.submit_groups([{"f": "SLEEP", "k": "n", "n": 40000, "v": [ONE] * 40000}], priority="bulk")
            time.sleep(0.2)
            started = time.monotonic()
            letters, values = conn.submit_groups([{"f": "ID", "k": "n", "n": 1, "v": [TWO]}]).raw(30)
            waited = time.monotonic() - started
            self.assertEqual(values, [TWO])
            self.assertLess(waited, 0.9, "the interactive call waited for the whole bulk request")
            bulk.raw(120)
        finally:
            stop()

    def test_a_resent_piece_is_replayed_not_evaluated_twice(self):
        broker, state, stop = start_broker()
        try:
            conn = self.connect(state)
            pending = conn.submit_groups([{"f": "ID", "k": "n", "n": 2, "v": [ONE, TWO]}], rid="e" * 32)
            pending.raw(30)
            batches = broker.counters["batches"]
            again = self.connect(state)
            second = again.submit_groups([{"f": "ID", "k": "n", "n": 2, "v": [ONE, TWO]}], rid="e" * 32)
            second.raw(30)
            self.assertEqual(broker.counters["batches"], batches)
        finally:
            stop()

    def test_v1_file_queue_is_answered_in_the_v1_shape(self):
        import hashlib
        broker, state, stop = start_broker()
        try:
            rid = "f" * 32
            request = {"version": 1, "id": rid, "function": "ID", "rows": [[number(TWO)], [number(ONE)]],
                       "created": time.time(), "expires": time.time() + 60}
            (state / "queue" / "in").mkdir(parents=True, exist_ok=True)
            tmp = state / "queue" / "in" / (rid + ".tmp")
            tmp.write_text(json.dumps(request))
            tmp.replace(state / "queue" / "in" / (rid + ".json"))
            out = state / "queue" / "out" / (rid + ".json")
            deadline = time.monotonic() + 20
            while not out.exists() and time.monotonic() < deadline:
                time.sleep(0.05)
            reply = json.loads(out.read_text())
            self.assertTrue(reply["ok"], reply)
            self.assertEqual([r["bits"] for r in reply["rows"]], [TWO, ONE])
            self.assertFalse((state / "queue" / "in" / (rid + ".json")).exists())
        finally:
            stop()


if __name__ == "__main__":
    unittest.main()
