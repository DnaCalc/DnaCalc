"""Run with: python -m unittest discover -s tools/excel-service/touchstone -p "test_*.py" -v

Hermetic: no Excel.  The broker tests drive a stand-in worker (fake_worker.py) over the real frame protocol.
"""
import asyncio
import json
import os
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
import peers  # noqa: E402
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


class PendingEnvironmentTests(unittest.TestCase):
    def test_environment_is_retained_for_each_answer_position(self):
        import importlib.util
        spec = importlib.util.spec_from_file_location("touchstone.client", HERE / "client.py")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        pending = module.Pending("id", 3, [], {}, {}, "host")
        pending.on_answer({"env": "a", "g": [{"i": [2, 0], "ty": "nn", "v": [TWO, ONE]}]})
        pending.on_answer({"env": "b", "g": [{"o": 1, "ty": "n", "v": [ONE]}]})
        self.assertEqual(pending.env_by_position, ["a", "b", "a"])
        self.assertEqual(pending.env_ids, {"a", "b"})


class FakeConn:
    def __init__(self, origin):
        self.origin = origin


def piece(origin, rid, f, calls, priority="bulk", k="n"):
    groups = protocol.with_positions([{"f": f, "k": k, "n": calls, "v": [ONE] * calls}])
    return batcher.Piece(FakeConn(origin), rid, 0, groups, True, priority)


class BatcherTests(unittest.TestCase):
    def test_serial_pieces_stay_whole_and_wait_for_the_serial_worker(self):
        b = batcher.Batcher()
        p = piece("a", "s", "ID", 40)
        p.serial = True
        p.primer = [{"f": "ABS", "k": "n", "n": 1, "v": [ONE], "p": True}]
        b.add_piece(p)
        self.assertFalse(b.has_work())                 # ordinary workers never see it
        self.assertTrue(b.serial_pending())
        batch = b.next_serial_batch()
        self.assertTrue(batch.serial)
        self.assertEqual([g["n"] for g in batch.groups], [40])
        self.assertEqual(batch.primer, p.primer)
        self.assertFalse(b.serial_pending())

    def test_spill_calls_never_share_a_batch_with_scalar_calls_and_are_capped(self):
        b = batcher.Batcher()
        spill = protocol.with_positions([{"f": "MINVERSE", "k": "a2x2", "n": 1500, "v": [ONE] * 6000, "spill": True}])
        b.add_piece(batcher.Piece(FakeConn("a"), "r1", 0, spill, True, "bulk"))
        b.add_piece(piece("a", "r2", "ABS", 50))
        kinds = []
        while b.has_work():
            batch = b.next_batch()
            kinds.append((bool(batch.groups[0].get("spill")), batch.calls))
            self.assertEqual({bool(g.get("spill")) for g in batch.groups}, {kinds[-1][0]})
        self.assertTrue(all(calls <= batcher.MAX_SPILL_CALLS for spill_kind, calls in kinds if spill_kind))
        self.assertEqual(sum(calls for _, calls in kinds), 1550)

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
        b.costs["ABS"] = 25.0
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
        self.assertLessEqual(b.costs["X"], batcher.MAX_COST_US)

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

    def test_admits_every_function_but_remote_services_and_refuses_malformed_groups(self):
        # Owner 2026-11-12: a test service on our own Excel admits every function, text and array argument;
        # only the remote service functions (deferred in CalcFunc) are skipped.
        self.admission.check(self.group())
        for good in (self.group(f="CALL"), self.group(f="INDIRECT"), self.group(f="NOW"),
                     self.group(f="LEN", k="s", n=1, v=["C:/secret.xlsx"]), self.group(f="LEN", k="s", n=1, v=["[book]A1"]),
                     self.group(f="LEN", k="s", n=1, v=["https://x.example"]), self.group(f="LEN", k="s", n=1, v=["a\u0001b"]),
                     self.group(f="SUMIF", k="a1x2,S", n=1, v=[ONE, TWO, ">1"]),
                     self.group(f="FILTER", k="a2x1,a2x1", n=1, v=[ONE, TWO, ONE, ONE], spill=True)):
            with self.subTest(good=good):
                self.admission.check(good)
        for bad in (self.group(f="WEBSERVICE"), self.group(f="STOCKHISTORY"), self.group(f="PY"), self.group(f="SUM(1)"),
                    self.group(v=["7ff8000000000000", ONE]), self.group(v=[ONE]), self.group(k="x"),
                    self.group(k="n,n", v=[ONE] * 4, f="LEN", x=[1, 1]), self.group(n=0, v=[]),
                    self.group(f="LEN", k="s", n=1, v=["a\u0000b"]), self.group(k="a40x40", f="LINEST", n=1, v=[ONE] * 1600)):
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
                              worker_command=[sys.executable, str(HERE / "fake_worker.py")],
                              **{"sweep_orphans": False, **(extra or {})})
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

    def test_workers_are_recycled_one_at_a_time_and_answers_keep_coming(self):
        broker, state, stop = start_broker(workers=2, extra={"recycle_batches": 3})
        try:
            conn = self.connect(state)
            for i in range(14):
                letters, values = conn.submit_groups([{"f": "ID", "k": "n", "n": 1, "v": [TWO]}]).raw(30)
                self.assertEqual(values, [TWO])
            deadline = time.monotonic() + 10
            while time.monotonic() < deadline and not any(s.since["batches"] < 14 and s.batches > 3 for s in broker.slots):
                time.sleep(0.05)
            broker.batch_log.flush()
            import logs
            events = [r for r in logs.BatchLog.read(state / "log") if r.get("event") == "recycled"]
            self.assertGreaterEqual(len(events), 1)
            self.assertEqual({s.state for s in broker.slots} <= {"idle", "busy", "new"}, True)
            self.assertNotIn("down", {s.state for s in broker.slots})
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



@unittest.skipUnless(sys.platform == "win32", "Native-only: the broker holds a Windows process lock")
class ClientApiTests(unittest.TestCase):
    """The v1-compatible Client over the socket transport, against the stand-in worker."""

    @classmethod
    def setUpClass(cls):
        cls.broker, cls.state, cls.stop = start_broker(workers=3)
        sys.path.insert(0, str(HERE.parent))
        import client as top
        cls.top = top

    @classmethod
    def tearDownClass(cls):
        cls.stop()

    def make(self, **options):
        return self.top.Client(endpoint=self.state / "endpoint.json", socket_path=self.state / "none.sock", timeout=30, **options)

    def test_ask_submit_ask_many_and_stream_keep_request_order_and_shape(self):
        c = self.make()
        try:
            self.assertTrue(c.uses_socket)
            rows = [[self.top.number(f"{0x3ff0000000000000 + i:016x}")] for i in range(300)]
            reply = c.ask("ID", rows, echo_inputs=True)
            self.assertEqual([r["bits"] for r in reply["rows"]], [r[0]["bits"] for r in rows])
            self.assertEqual(reply["inputs"][3][0], rows[3][0])
            future = c.submit("ID", rows[:5])
            self.assertEqual(len(future.result(30)["rows"]), 5)
            replies = c.ask_many([("ID", rows[:2]), ("ID", rows[2:6]), ("ID", rows[6:7])])
            self.assertEqual([len(r["rows"]) for r in replies], [2, 4, 1])
            seen = dict(c.stream([("ID", rows[:3]), ("ID", rows[3:5])]))
            self.assertEqual({k: len(v["rows"]) for k, v in seen.items()}, {0: 3, 1: 2})
        finally:
            c.close()

    def test_failures_raise_service_error_and_malformed_rows_raise_value_error(self):
        c = self.make()
        try:
            with self.assertRaises(ValueError):
                c.ask("ID", [[{"type": "bogus"}]])
            with self.assertRaises(self.top.ServiceError) as caught:
                c.ask("CRASH", [[self.top.number(1.0)]])
            self.assertIn("excel-crashed", str(caught.exception))
        finally:
            c.close()

    def test_priority_and_order_options_are_accepted(self):
        c = self.make()
        try:
            r = c.ask("ID", [[self.top.number(2.0)]], priority="bulk", order="serial")
            self.assertEqual(r["rows"][0]["bits"], self.top.bits(2.0))
            with self.assertRaises(ValueError):
                c.ask("ID", [[self.top.number(2.0)]], order="sometimes")
        finally:
            c.close()

    def test_bisect_edge_needs_about_nine_rounds_for_52_bits(self):
        c = self.make()
        try:
            threshold = 0x3e47bc15e9263272
            edge = c.bisect_edge("ID", "3e47bc15e9000000", "3e47bc15ea000000",
                                 lambda x, answer: int(answer["bits"], 16) > threshold)
            self.assertEqual((edge["low"], edge["high"]), (f"{threshold:016x}", f"{threshold + 1:016x}"))
            self.assertLessEqual(edge["questions"], 8)     # 2^24 wide bracket: 64 points per round
            wide = c.bisect_edge("ID", "3000000000000000", "4000000000000000",
                                 lambda x, answer: int(answer["bits"], 16) > threshold)
            self.assertEqual(wide["low"], f"{threshold:016x}")
            self.assertLessEqual(wide["questions"], 12)     # 2^60 wide bracket
        finally:
            c.close()

    def test_a_reading_that_cannot_tell_the_sides_is_skipped_not_guessed(self):
        c = self.make()
        try:
            threshold = 0x3e47bc15e9263272
            edge = c.bisect_edge("ID", "3e47bc15e9263000", "3e47bc15e9263400",
                                 lambda x, answer: None if x == "3e47bc15e9263270" else int(x, 16) > threshold)
            self.assertEqual((edge["low"], edge["high"]), (f"{threshold:016x}", f"{threshold + 1:016x}"))
        finally:
            c.close()

    def test_async_client(self):
        async def go():
            a = self.top.AsyncClient(endpoint=self.state / "endpoint.json", socket_path=self.state / "none.sock", timeout=30)
            try:
                one = await a.ask("ID", [[self.top.number(4.0)]])
                many = await a.ask_many([("ID", [[self.top.number(1.0)]]), ("ID", [[self.top.number(2.0)], [self.top.number(3.0)]])])
                got = {}
                async for index, reply in a.as_completed([("ID", [[self.top.number(5.0)]]), ("ID", [[self.top.number(6.0)]])]):
                    got[index] = reply["rows"][0]["bits"]
                return one, many, got
            finally:
                a.close()
        one, many, got = asyncio.run(go())
        self.assertEqual(one["rows"][0]["bits"], self.top.bits(4.0))
        self.assertEqual([len(r["rows"]) for r in many], [1, 2])
        self.assertEqual(got, {0: self.top.bits(5.0), 1: self.top.bits(6.0)})

    def test_consecutive_run_beyond_the_v1_limit_works_over_the_socket(self):
        c = self.make()
        try:
            run = c.consecutive_run("ID", "3e47bc15e9000000", 30000)
            self.assertEqual(len(run["rows"]), 30000)
            self.assertEqual(run["rows"][29999]["bits"], f"{0x3e47bc15e9000000 + 29999:016x}")
        finally:
            c.close()

    def test_serial_requests_run_whole_and_in_order_on_the_serial_worker_and_say_so(self):
        c = self.make()
        try:
            before = self.broker.slots[2].calls
            rows = [[self.top.number(float(i))] if i % 2 == 0 else [self.top.text(f"t{i}")] for i in range(20)]
            reply = c.ask("ID", rows, order="serial", primer=[("ABS", [self.top.number(1.0)])])
            self.assertEqual(reply["order"], "serial")
            self.assertEqual(len(reply["rows"]), 20)
            self.assertEqual(self.broker.slots[2].calls - before, 20)       # the primer call is evaluated, not counted or answered
            self.assertTrue(self.broker.slots[2].serial)
        finally:
            c.close()

    def test_a_state_sensitive_function_is_always_served_serially(self):
        c = self.make()
        try:
            before = self.broker.slots[2].calls
            reply = c.ask("WEEKNUM", [[self.top.number(43836.0), self.top.number(float(t))] for t in (1, 99, 2)])
            self.assertEqual(reply["order"], "serial")
            self.assertEqual(self.broker.slots[2].calls - before, 3)
        finally:
            c.close()

    def test_array_questions_are_flagged_as_spill_groups(self):
        c = self.make()
        try:
            conn = c._socket()
            pending = conn.submit_rows("MINVERSE", [[self.top.number_array([[1.0, 2.0], [3.0, 4.0]])]], spill=True)
            self.assertTrue(pending.groups[0]["spill"])
            self.assertEqual(pending.groups[0]["k"], "a2x2")
            pending.raw(30)
        finally:
            c.close()

    def test_the_file_queue_is_used_when_a_queue_is_named(self):
        c = self.top.Client(queue=self.state / "queue", timeout=30)
        self.assertFalse(c.uses_socket)
        r = c.ask("ID", [[self.top.number(7.0)]])
        self.assertEqual(r["rows"][0]["bits"], self.top.bits(7.0))
        c.close()


def _wsl_python():
    import shutil
    import subprocess
    if not shutil.which("wsl"):
        return False
    try:
        done = subprocess.run(["wsl", "-e", "python3", "--version"], capture_output=True, timeout=60)
        return done.returncode == 0
    except Exception:
        return False


@unittest.skipUnless(sys.platform == "win32" and _wsl_python(), "Native-only: runs the Linux relay in WSL")
class RelayTests(unittest.TestCase):
    """The Linux relay (relay.py) in WSL, with this test playing the broker on its stdin and stdout."""

    def setUp(self):
        import subprocess
        import peers
        self.subprocess = subprocess
        self.dir = "/tmp/touchstone-relay-test-%d" % int(time.time() * 1000)
        command, first = peers.bundle({"socket": self.dir + "/excel.sock", "v1_queue": self.dir + "/queue"})
        self.relay = subprocess.Popen(["wsl", "-e", "bash", "-c", command], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                      stderr=subprocess.PIPE)
        self.relay.stdin.write(first)
        self.relay.stdin.flush()
        self.frames = protocol.FrameReader()
        self.seen = []
        hello = self.read_frame()
        self.assertEqual(hello[0], protocol.HELLO)
        self.send(protocol.HELLO, {"v": protocol.VERSION, "server": "broker-host", "envs": {"e1": {"build": "7", "version": "16.0"}}})

    def tearDown(self):
        self.relay.stdin.close()
        try:
            self.relay.wait(20)
        except Exception:
            self.relay.kill()
        self.subprocess.run(["wsl", "-e", "rm", "-rf", self.dir], capture_output=True)

    def send(self, kind, body):
        self.relay.stdin.write(protocol.pack(kind, body, True))
        self.relay.stdin.flush()

    def read_frame(self):
        while not self.seen:
            chunk = self.relay.stdout.read1(1 << 16)
            if not chunk:
                raise AssertionError("relay closed: " + self.relay.stderr.read().decode()[-500:])
            self.seen.extend(self.frames.feed(chunk))
        return self.seen.pop(0)

    def answer_submits(self, count):
        """Answer `count` submit frames like a broker: every call answers the number 1.0; then done."""
        for _ in range(count):
            kind, body = self.read_frame()
            self.assertEqual(kind, protocol.SUBMIT)
            groups = [{"ty": "n" * g["n"], "v": [ONE] * g["n"], **({"o": g["o"]} if "o" in g else {"i": g["i"]}),
                       **({"ity": "n" * g["n"], "iv": [g["v"][i * protocol.group_width(g)] for i in range(g["n"])]} if body.get("opts", {}).get("echo") else {})}
                      for g in body["groups"]]
            self.send(protocol.ANSWER, {"id": body["id"], "seq": body["seq"], "c": body["c"], "env": "e1", "g": groups})
            if body["last"]:
                self.send(protocol.DONE, {"id": body["id"], "c": body["c"], "calls": 0})

    def test_a_lane_client_on_the_socket_is_answered_through_the_broker_stream(self):
        import peers
        runner = ("import json" + chr(10) +
                  "conn = sys.modules['client'].Connection.open(socket_path=%r, name='lane')" + chr(10) +
                  "p = conn.submit_groups([{'f': 'ABS', 'k': 'n', 'n': 3, 'v': ['%s', '%s', '%s']}])" + chr(10) +
                  "letters, values = p.raw(30)" + chr(10) +
                  "print(json.dumps({'values': values, 'server': conn.machine}))" + chr(10)) % (
                      self.dir + "/excel.sock", TWO, TWO, TWO)
        command, first = peers.bundle_program(("protocol", "v1", "client"), runner)
        lane = self.subprocess.Popen(["wsl", "-e", "bash", "-c", command], stdin=self.subprocess.PIPE,
                                     stdout=self.subprocess.PIPE, stderr=self.subprocess.PIPE)
        lane.stdin.write(first)
        lane.stdin.close()
        self.answer_submits(1)
        out, err = lane.communicate(timeout=60)
        self.assertEqual(lane.returncode, 0, err.decode()[-800:])
        result = json.loads(out.decode().strip().splitlines()[-1])
        self.assertEqual(result, {"values": [ONE, ONE, ONE], "server": "broker-host"})

    def test_a_v1_request_file_is_answered_in_the_v1_reply_shape(self):
        rid = "ab" * 16
        request = {"version": 1, "id": rid, "function": "ABS", "rows": [[number(TWO)], [number(ONE)]],
                   "created": time.time(), "expires": time.time() + 120}
        self.subprocess.run(["wsl", "-e", "bash", "-c", "mkdir -p %s/queue/in && cat > %s/queue/in/%s.json" % (self.dir, self.dir, rid)],
                            input=json.dumps(request).encode(), check=True, timeout=60)
        self.answer_submits(1)
        deadline = time.monotonic() + 30
        reply = None
        while time.monotonic() < deadline:
            done = self.subprocess.run(["wsl", "-e", "cat", "%s/queue/out/%s.json" % (self.dir, rid)], capture_output=True)
            if done.returncode == 0 and done.stdout:
                reply = json.loads(done.stdout)
                break
            time.sleep(0.2)
        self.assertIsNotNone(reply, "no v1 reply")
        self.assertTrue(reply["ok"], reply)
        self.assertEqual([r["bits"] for r in reply["rows"]], [ONE, ONE])
        self.assertEqual(reply["inputs"][0][0], number(TWO))
        self.assertEqual(reply["excel_build"], "7")
        self.assertEqual(reply["machine"], "broker-host")

    def test_the_relay_leaves_with_its_ssh_session(self):
        self.relay.stdin.close()
        self.assertIsNotNone(self.relay.wait(30))
        gone = self.subprocess.run(["wsl", "-e", "ls", self.dir + "/excel.sock"], capture_output=True)
        self.assertNotEqual(gone.returncode, 0)


class CostSizedBatchTests(unittest.TestCase):
    def test_an_unmeasured_function_runs_alone_as_a_small_probe(self):
        b = batcher.Batcher()
        b.add_piece(piece("a", "r1", "NEWFN", 3000))
        b.add_piece(piece("a", "r2", "ABS", 3000))
        b.costs["ABS"] = 25.0
        first = b.next_batch()
        self.assertEqual({g["f"] for g in first.groups}, {"NEWFN"})
        self.assertEqual(first.calls, batcher.PROBE_CALLS)

    def test_a_measured_heavy_function_is_cut_by_estimated_cost_not_by_count(self):
        b = batcher.Batcher()
        b.costs["HEAVY"] = 20_000.0          # 20 ms per call
        b.add_piece(piece("a", "r1", "HEAVY", 3000))
        batch = b.next_batch()
        self.assertLessEqual(batch.est_ms, batcher.BULK_BATCH_MS + 20.0)
        self.assertLessEqual(batch.calls, 41)          # 800 ms of estimated work at 20 ms a call

    def test_learning_from_a_batch_of_one_function_is_direct(self):
        b = batcher.Batcher()
        batch = batcher.Batch()
        batch.groups, batch.calls = [{"f": "X", "n": 50, "k": "n", "v": []}], 50
        b.learn(batch, 4.0 + 1.0 + 50 * 2.0)      # fixed costs plus 2 ms per call
        self.assertAlmostEqual(b.costs["X"], 2000.0, delta=1.0)

    def test_clients_share_by_cost_not_by_call_count(self):
        b = batcher.Batcher()
        b.costs.update({"HEAVY": 10_000.0, "LIGHT": 50.0})
        heavy, light = piece("a", "ra", "HEAVY", 4000), piece("b", "rb", "LIGHT", 1000000)
        heavy.fair, light.fair = "tcp:a", "tcp:b"
        b.add_piece(heavy)
        b.add_piece(light)
        cost = {"tcp:a": 0.0, "tcp:b": 0.0}
        for _ in range(40):
            batch = b.next_batch()
            for sl in batch.slices:
                cost[sl.item.piece.fair] += sl.item.n * b.costs[sl.item.group["f"]]
        self.assertGreater(min(cost.values()), 0.25 * max(cost.values()))

    def test_clients_behind_one_relay_are_separate_clients(self):
        b = batcher.Batcher()
        b.costs["ABS"] = 25.0
        one, two = piece("relay", "r1", "ABS", 400000), piece("relay", "r2", "ABS", 400000)
        one.fair, two.fair = "relay/1", "relay/2"
        b.add_piece(one)
        b.add_piece(two)
        seen = {}
        for _ in range(6):
            for sl in b.next_batch().slices:
                seen[sl.item.piece.fair] = seen.get(sl.item.piece.fair, 0) + sl.item.n
        self.assertEqual(set(seen), {"relay/1", "relay/2"})
        self.assertGreater(min(seen.values()), 0.3 * sum(seen.values()))

    def test_oldest_queued_item_is_reported(self):
        b = batcher.Batcher()
        self.assertIsNone(b.oldest_queued())
        first = piece("a", "r1", "ABS", 10)
        b.add_piece(first)
        b.add_piece(piece("b", "r2", "SQRT", 10, "interactive"))
        self.assertEqual(b.oldest_queued(), first.t_received)


@unittest.skipUnless(sys.platform == "win32", "Native-only: the broker holds a Windows process lock")
class SelfHealingTests(unittest.TestCase):
    def connect(self, state, name="t"):
        from touchstone import client
        return client.Connection.open(endpoint=state / "endpoint.json", socket_path=state / "none.sock", name=name)

    def wait_for(self, condition, seconds=20):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            if condition():
                return True
            time.sleep(0.05)
        return False

    def test_a_failed_start_is_retried_after_the_cooldown_and_never_hammered(self):
        import os
        flag = Path(tempfile.mkdtemp()) / "fail"
        flag.write_text("w2")
        os.environ["TOUCHSTONE_TEST_FAIL_FLAG"] = str(flag)
        try:
            broker, state, stop = start_broker(workers=2, extra={"cooldown": 0.4, "restart_cap": 3})
        except BaseException:
            del os.environ["TOUCHSTONE_TEST_FAIL_FLAG"]
            raise
        try:
            w2 = broker.slots[1]
            self.assertEqual(w2.state, "down")
            # still failing: two retries at the cool-down spacing, then the cap holds it down
            self.assertTrue(self.wait_for(lambda: broker.window_attempts(w2) == 3 and w2.state == "down"))
            time.sleep(1.5)
            self.assertEqual(broker.window_attempts(w2), 3)         # no fourth try inside the window
            self.assertIn("restart cap", w2.down_reason)
            waits = [json.loads(l) for l in (state / "log" / "waits.jsonl").read_text().splitlines()]
            downs = [w for w in waits if w["kind"] == "worker-down" and w.get("worker") == "w2"]
            self.assertGreaterEqual(len(downs), 3)
            self.assertTrue(any("restart cap" in w.get("detail", "") for w in downs))
            self.assertEqual(broker.slots[0].state, "idle")            # the pool carried on
            # Excel is fixed; the orchestrator asks for a retry now
            flag.unlink()
            (state / "heal-now").touch()
            self.assertTrue(self.wait_for(lambda: w2.state == "idle"))
        finally:
            del os.environ["TOUCHSTONE_TEST_FAIL_FLAG"]
            stop()

    def test_a_worker_whose_start_works_again_comes_back_after_the_cooldown(self):
        import os
        flag = Path(tempfile.mkdtemp()) / "fail"
        flag.write_text("w2")
        os.environ["TOUCHSTONE_TEST_FAIL_FLAG"] = str(flag)
        try:
            broker, state, stop = start_broker(workers=2, extra={"cooldown": 0.5})
        except BaseException:
            del os.environ["TOUCHSTONE_TEST_FAIL_FLAG"]
            raise
        try:
            w2 = broker.slots[1]
            self.assertEqual(w2.state, "down")
            flag.unlink()
            self.assertTrue(self.wait_for(lambda: w2.state == "idle", 10))
            conn = self.connect(state)
            self.assertEqual(conn.submit_groups([{"f": "ID", "k": "n", "n": 1, "v": [TWO]}]).raw(30)[1], [TWO])
            self.assertTrue(broker.health_line().startswith("OK "))
        finally:
            del os.environ["TOUCHSTONE_TEST_FAIL_FLAG"]
            stop()

    def test_a_lost_worker_process_is_restarted_and_the_batch_alone_fails(self):
        broker, state, stop = start_broker(workers=1, extra={"cooldown": 0.3})
        try:
            from touchstone import client
            conn = self.connect(state)
            with self.assertRaises(client.ServiceError):
                conn.submit_groups([{"f": "DIE", "k": "n", "n": 1, "v": [ONE]}]).raw(30)
            self.assertTrue(self.wait_for(lambda: broker.slots[0].state == "idle"))
            self.assertEqual(conn.submit_groups([{"f": "ID", "k": "n", "n": 1, "v": [TWO]}]).raw(30)[1], [TWO])
        finally:
            stop()

    def test_repeated_crashes_hit_the_cap_then_down_with_a_status_and_a_waits_entry_then_heal_now(self):
        broker, state, stop = start_broker(workers=1, extra={"cooldown": 0.3, "restart_cap": 3})
        try:
            from touchstone import client
            conn = self.connect(state)
            for _ in range(4):
                self.assertTrue(self.wait_for(lambda: broker.slots[0].state in ("idle", "down")))
                if broker.slots[0].state == "down":
                    break
                with self.assertRaises(client.ServiceError):
                    conn.submit_groups([{"f": "CRASHALWAYS", "k": "n", "n": 1, "v": [ONE]}]).raw(30)
            self.assertTrue(self.wait_for(lambda: broker.slots[0].state == "down"))
            slot = broker.slots[0]
            self.assertIn("restart cap", slot.down_reason)
            self.assertTrue(broker.health_line().startswith("DOWN "), broker.health_line())
            time.sleep(1.0)
            self.assertEqual(slot.state, "down")                      # the cap holds even after the cool-down
            self.assertIn("restart cap", (state / "log" / "waits.jsonl").read_text())
            self.assertTrue(json.loads((state / "isolated.json").read_text())["isolated"] == ["CRASHALWAYS"])
            (state / "heal-now").touch()
            self.assertTrue(self.wait_for(lambda: slot.state == "idle"))
            self.assertEqual(conn.submit_groups([{"f": "ID", "k": "n", "n": 1, "v": [TWO]}]).raw(30)[1], [TWO])
        finally:
            stop()

    def test_isolated_functions_survive_a_broker_restart(self):
        import broker as broker_module
        with tempfile.TemporaryDirectory() as folder:
            state = Path(folder)
            (state / "isolated.json").write_text(json.dumps({"isolated": ["COMBIN", "COMBINA"], "crash_counts": {"COMBIN": 2}}))
            (state / "costs.json").write_text(json.dumps({"ABS": 12.5}))
            instance = broker_module.Broker(SimpleNamespace(state=state, workers=1, local_queue=state / "q", hosts=[], admission=None))
            instance.prepare()
            self.assertEqual(instance.batcher.isolate, {"COMBIN", "COMBINA"})
            self.assertEqual(instance.crash_counts["COMBIN"], 2)
            self.assertEqual(instance.batcher.costs["ABS"], 12.5)
            instance.batch_log.close()

    def test_health_line_file_and_status_command(self):
        import broker as broker_module
        import contextlib
        import io
        broker, state, stop = start_broker(workers=2, extra={"health_seconds": 0.3})
        try:
            self.assertTrue(self.wait_for(lambda: (state / "health.txt").exists()))
            line = (state / "health.txt").read_text().strip()
            self.assertRegex(line, r"^OK workers_up=2/2 down=0 starting=0 queue_calls=0 oldest_queued_s=0\.0 last_answer=never utc=")
            conn = self.connect(state)
            conn.submit_groups([{"f": "ID", "k": "n", "n": 1, "v": [TWO]}]).raw(30)
            self.assertTrue(self.wait_for(lambda: "last_answer=never" not in (state / "health.txt").read_text()))
            time.sleep(0.8)         # let one tick publish status.json after the answer
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                with self.assertRaises(SystemExit) as raised:
                    broker_module.command_status(SimpleNamespace(state=state, json=False))
            self.assertEqual(raised.exception.code, 0)
            self.assertEqual(len(out.getvalue().strip().splitlines()), 1)
            self.assertIn("workers_up=2/2", out.getvalue())
            self.assertIn("last_answer=", out.getvalue())
            self.assertNotIn("never", out.getvalue())
        finally:
            stop()
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            with self.assertRaises(SystemExit) as raised:
                broker_module.command_status(SimpleNamespace(state=state, json=False))
        self.assertNotEqual(raised.exception.code, 0)
        self.assertTrue(out.getvalue().startswith("DOWN"))

    def test_an_old_status_file_reads_as_down(self):
        import broker as broker_module
        import contextlib
        import io
        with tempfile.TemporaryDirectory() as folder:
            state = Path(folder)
            (state / "status.json").write_text(json.dumps({"pid": 1, "running": True, "utc": "2020-01-01T00:00:00+00:00",
                                                           "health": "OK workers_up=3/3"}))
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                with self.assertRaises(SystemExit) as raised:
                    broker_module.command_status(SimpleNamespace(state=state, json=False))
            self.assertEqual(raised.exception.code, 2)
            self.assertTrue(out.getvalue().startswith("DOWN broker status is"))


@unittest.skipUnless(sys.platform == "win32", "Native-only: the broker holds a Windows process lock")
class CapacityTests(unittest.TestCase):
    def connect(self, state, name="t"):
        from touchstone import client
        return client.Connection.open(endpoint=state / "endpoint.json", socket_path=state / "none.sock", name=name)

    def test_bulk_never_fills_the_last_worker_and_interactive_calls_do_not_wait(self):
        broker, state, stop = start_broker(workers=3)
        try:
            conn = self.connect(state)
            bulk = conn.submit_groups([{"f": "SLEEP", "k": "n", "n": 400, "v": [ONE] * 400}], priority="bulk")
            most = 0
            waited = []
            deadline = time.monotonic() + 3.0
            while time.monotonic() < deadline:
                most = max(most, sum(1 for s in broker.slots if s.state == "busy" and s.running == "bulk"))
                t0 = time.monotonic()
                conn.submit_groups([{"f": "ID", "k": "n", "n": 1, "v": [TWO]}]).raw(30)
                waited.append(time.monotonic() - t0)
                time.sleep(0.1)
            self.assertEqual(most, 2)                       # three workers: two for bulk, one kept free
            self.assertLess(max(waited), 0.5)
            bulk.raw(120)
        finally:
            stop()

    def test_one_worker_alone_still_serves_bulk(self):
        broker, state, stop = start_broker(workers=1)
        try:
            conn = self.connect(state)
            letters, values = conn.submit_groups([{"f": "ID", "k": "n", "n": 3000, "v": [ONE] * 3000}], priority="bulk").raw(60)
            self.assertEqual(len(values), 3000)
        finally:
            stop()

    def test_a_heavy_function_is_probed_small_then_batched_by_cost(self):
        broker, state, stop = start_broker(workers=2)
        try:
            conn = self.connect(state)
            conn.submit_groups([{"f": "HEAVY", "k": "n", "n": 120, "v": [ONE] * 120}], priority="bulk").raw(120)
            broker.batch_log.flush()
            import logs
            sizes = [r["calls"] for r in logs.BatchLog.read(state / "log") if r.get("kind") == "batch" and "HEAVY" in r.get("fns", {})]
            self.assertEqual(sizes[0], batcher.PROBE_CALLS)
            self.assertGreater(broker.batcher.costs["HEAVY"], 5000.0)       # 20 ms per call measured, in microseconds
            self.assertTrue(all(n <= batcher.PROBE_CALLS for n in sizes), sizes)         # 800 ms / 20 ms = 40 calls
        finally:
            stop()


class OrphanAndBuildTests(unittest.TestCase):
    def test_only_a_command_line_naming_this_state_folder_matches(self):
        import engine
        state = r"C:\Work\DnaCalc\codex-runs\excel-service-v2"
        self.assertTrue(engine.names_state(r'EXCEL.EXE /x /automation /r "C:\Work\DnaCalc\codex-runs\excel-service-v2\excel\launches\w1-1.csv"', state))
        self.assertTrue(engine.names_state(r"excel.exe C:/Work/DnaCalc/codex-runs/excel-service-v2/excel/x.csv", state))
        self.assertFalse(engine.names_state(r"excel.exe C:\Work\DnaCalc\codex-runs\excel-service-v2-proof\excel\x.csv", state))
        self.assertFalse(engine.names_state(r"excel.exe C:\Work\DnaCalc\codex-runs\jnt-excel-session\launches\1.csv", state))
        self.assertFalse(engine.names_state(r'EXCEL.EXE /e "C:\Users\me\Documents\book.xlsx"', state))

    def test_orphans_are_stopped_by_pid_and_start_ticks_and_nothing_else(self):
        import engine
        state = r"C:\Work\DnaCalc\codex-runs\excel-service-v2"
        processes = [(10, 111, rf'EXCEL.EXE /x /r "{state}\excel\launches\w1-1.csv"'),
                     (11, 222, r'EXCEL.EXE /x /r "C:\Work\DnaCalc\codex-runs\jnt-excel-session\launches\1.csv"'),
                     (12, 333, r"EXCEL.EXE")]
        killed = []
        found = engine.stop_orphans(state, processes=processes, kill=lambda pid, ticks: killed.append((pid, ticks)) or "gone")
        self.assertEqual(killed, [(10, 111)])
        self.assertEqual(found, [(10, "gone")])

    def test_a_real_process_named_excel_with_this_state_in_its_command_line_is_stopped(self):
        import engine
        import shutil
        import subprocess
        with tempfile.TemporaryDirectory() as folder:
            state = Path(folder) / "state-v2"
            (state / "excel" / "launches").mkdir(parents=True)
            impostor = Path(folder) / "excel.exe"
            shutil.copy(r"C:\Windows\System32\cmd.exe", impostor)
            ours = subprocess.Popen([str(impostor), "/c", f'ping -n 60 127.0.0.1 >nul & rem {state}\\excel\\launches\\w1-1.csv'],
                                    creationflags=subprocess.CREATE_NO_WINDOW)
            other = subprocess.Popen([str(impostor), "/c", f'ping -n 60 127.0.0.1 >nul & rem {state}-other\\excel\\x.csv'],
                                     creationflags=subprocess.CREATE_NO_WINDOW)
            try:
                deadline = time.monotonic() + 15
                found = []
                while time.monotonic() < deadline and not any(pid == ours.pid for pid, _ in found):
                    found = engine.stop_orphans(state)
                    time.sleep(0.2)
                self.assertIn((ours.pid, "owned Excel terminated"), found)
                self.assertTrue(all(pid != other.pid for pid, _ in found))
                self.assertIsNone(other.poll())             # a sibling folder with the same prefix is left alone
            finally:
                for proc in (ours, other):
                    proc.kill()
                    proc.wait()

    def make_project(self, folder):
        project = Path(folder) / "excel"
        project.mkdir()
        (project / "ExcelWorker.csproj").write_text("<Project/>")
        (project / "Program.cs").write_text("class P{}")
        build = Path(folder) / "build.py"
        build.write_text("import pathlib, sys\nb = pathlib.Path('bin'); b.mkdir(exist_ok=True)\n"
                         "(b / 'excel-worker.dll').write_text('x'); (b / 'excel-worker.exe').write_text('x')\n"
                         "open(sys.argv[1], 'a').write('built\\n')\n")
        return project, build

    def test_the_worker_is_built_when_bin_is_missing_or_stale_and_not_otherwise(self):
        import engine
        with tempfile.TemporaryDirectory() as folder:
            project, build = self.make_project(folder)
            log = Path(folder) / "builds.txt"
            command = [sys.executable, str(build), str(log)]
            self.assertTrue(engine.worker_is_stale(project))
            self.assertTrue(engine.ensure_worker_built(project, command))
            self.assertFalse(engine.worker_is_stale(project))
            self.assertFalse(engine.ensure_worker_built(project, command))        # current: no build
            old = time.time() - 100          # the binaries age; the source is edited now
            for name in ("excel-worker.dll", "excel-worker.exe"):
                os.utime(project / "bin" / name, (old, old))
            os.utime(project / "Program.cs", None)
            self.assertTrue(engine.worker_is_stale(project))
            self.assertTrue(engine.ensure_worker_built(project, command))
            self.assertEqual(log.read_text().count("built"), 2)

    def test_a_failed_build_or_a_build_that_leaves_no_worker_fails_loudly(self):
        import engine
        with tempfile.TemporaryDirectory() as folder:
            project, build = self.make_project(folder)
            with self.assertRaises(RuntimeError) as failed:
                engine.ensure_worker_built(project, [sys.executable, "-c", "import sys; print('CS1002 broken'); sys.exit(1)"])
            self.assertIn("CS1002", str(failed.exception))
            with self.assertRaises(RuntimeError) as empty:
                engine.ensure_worker_built(project, [sys.executable, "-c", "pass"])
            self.assertIn("holds no current", str(empty.exception))


if __name__ == "__main__":
    unittest.main()
