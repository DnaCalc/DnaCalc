"""Run with: python -m unittest discover -s tools/excel-service/touchstone -p "test_*.py" -v

Hermetic: no Excel.  The broker tests drive a stand-in worker (fake_worker.py) over the real frame protocol.
"""
import asyncio
from contextlib import closing
import errno
import io
import json
import os
import re
import struct
import subprocess
from pathlib import Path
import sys
import tempfile
import threading
import time
import unittest
from unittest.mock import patch
from types import SimpleNamespace

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE.parent))

import base64  # noqa: E402
import batcher  # noqa: E402
import peers  # noqa: E402
import protocol  # noqa: E402
import v1  # noqa: E402

ROOT = HERE.parents[2]


def number(bits):
    return {"type": "number", "bits": bits}


ONE = "3ff0000000000000"
TWO = "4000000000000000"
IMPLICIT_VBA_PARAMETERS = "Function Probe(x, y)\n    Probe = x + y\nEnd Function\n"


DEFAULT_VBA_SOURCE = ('Option Explicit\r\nPublic Function Probe(ByVal x As Long) As Long\r\n'
                      '    Dim result As Long\r\n'
                      '    result = x + 1 \' Shell is only a comment\r\n'
                      '    ThisWorkbook.Worksheets("Data").Range("A1").Value2 = "He said ""Open"""\r\n'
                      '    Probe = result\r\nEnd Function\r\n')
DICTIONARY_VBA_SOURCE = ('Public Function Probe(ByVal x As Long) As Long\n'
                         '    Dim items As Object\n'
                         '    Set items = CreateObject("Scripting.Dictionary")\n'
                         '    Probe = items.Count + x\nEnd Function\n')
MULTI_DECLARATION_VBA_SOURCE = ("Public Function Probe(ByVal x As Long) As Long\nDim a As Long, b As Long\n"
                                "a = 1: b = 2\nProbe = a + b\nEnd Function\n")
MODULE_VARIABLE_VBA_SOURCE = ("Private shared As Long\nPublic Function Probe(ByVal x As Long) As Long\n"
                              "shared = x\nProbe = shared\nEnd Function\n")
TAB_CONTINUATION_VBA_SOURCE = "Public Function Probe(ByVal x As Long) As Long\nProbe = x + \t_\n 1\nEnd Function\n"
# Sources the grammar accepts, and the worker builds a package for.
ACCEPTED_VBA_SOURCES = [DEFAULT_VBA_SOURCE, IMPLICIT_VBA_PARAMETERS, DICTIONARY_VBA_SOURCE,
                        MULTI_DECLARATION_VBA_SOURCE, MODULE_VARIABLE_VBA_SOURCE, TAB_CONTINUATION_VBA_SOURCE]


def vba_group(source=None, **changes):
    if source is None:
        source = DEFAULT_VBA_SOURCE
    case = {"sheets": [{"name": "Data", "cells": {"A1": number(ONE)}}], "names": {},
            "modules": [{"name": "Probes", "kind": "standard", "source": source}],
            "calls": [{"procedure": "Probes.Probe", "args": [number(ONE)]}],
            "readback": ["Data!A1"], "event_log": ["Data!B1"]}
    case.update(changes)
    return {"f": "_VBA", "k": "V", "n": 1, "o": 0, "v": [case]}


def worker_flag(group, action="check"):
    """--check-vba-policy / --build-vba-package for a template case (the route Excel gets); the offline -expanded seam
    for a case that carries modules (grammar tests)."""
    expanded = "template" not in group["v"][0]
    return f"--{action}-vba-" + ("expanded" if expanded else "policy" if action == "check" else "package")


def template_vba_group(**changes):
    """The reviewed smoke template as a _VBA group: the only form that reaches Excel (dc-45ndr)."""
    case = json.loads((HERE / "vba-smoke.json").read_text(encoding="ascii"))
    case.update(calls=[{"procedure": "Module1.Probe", "args": [number(ONE)]}], readback=["Data!A1"], event_log=["Data!B1"])
    case.update(changes)
    return {"f": "_VBA", "k": "V", "n": 1, "o": 0, "v": [{"names": {}, **case}]}


class ProtocolTests(unittest.TestCase):
    def test_a_cached_refusal_is_replayed_with_its_original_code_and_detail(self):
        from broker import Broker

        with tempfile.TemporaryDirectory() as directory:
            broker = Broker(SimpleNamespace(state=Path(directory), admission=None))
            broker.prepare()
            first, replayed = [], []
            submit = {"id": "r" * 32, "seq": 0, "last": True,
                      "groups": [{"f": "ID", "k": "n", "n": 2, "v": [ONE]}]}
            broker.on_submit(SimpleNamespace(origin="same-client", send=lambda *args: first.append(args)), submit)
            broker.on_submit(SimpleNamespace(origin="same-client", send=lambda *args: replayed.append(args)), submit)
            self.assertEqual(first[0][0], protocol.ERROR)
            self.assertEqual(replayed[0][:2], first[0][:2])
            self.assertEqual(broker.counters["refused"], 1)

    def test_range_and_file_round_trip_through_stand_in_worker(self):
        cells = [[number(ONE), {"type": "text", "value": "a", "mode": "cell"}],
                 [{"type": "boolean", "value": True, "mode": "cell"}, {"type": "blank"}],
                 [{"type": "error", "code": "#N/A"}, number(TWO)]]
        row = [{"type": "range", "values": cells}, {"type": "file", "content": "a,b\n1,2\n"}]
        group = protocol.rows_to_groups("SUMIF", [row])[0]
        self.assertEqual(group["k"], "r3x2,f")
        self.assertEqual(protocol.group_to_rows(group), [row])
        import policy
        policy.Admission(ROOT).check(group)
        mixed = [{"type": "mixed_array", "values": cells}]
        mixed_group = protocol.rows_to_groups("ROWS", [mixed])[0]
        self.assertEqual(mixed_group["k"], "m3x2")
        self.assertEqual(protocol.group_to_rows(mixed_group), [mixed])
        policy.Admission(ROOT).check(mixed_group)
        for bad in ({"type": "number", "bits": "7ff0000000000000"},
                    {"type": "text", "value": "x", "mode": "literal"},
                    {"type": "formula", "value": "=1"}):
            malformed = dict(group, v=[bad] + group["v"][1:])
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                policy.Admission(ROOT).check(malformed)
        big = protocol.rows_to_groups("IMPORTTEXT", [[{"type": "file", "content": "x" * 4000}]] * 30)
        frames = protocol.submit_frames("a" * 32, big)
        self.assertGreater(len(frames), 1)
        self.assertTrue(all(len(protocol.dumps(frame)) < protocol.MAX_FRAME for frame in frames))
        worker = subprocess.Popen([sys.executable, str(HERE / "fake_worker.py"), "--name", "range-test"],
                                  stdin=subprocess.PIPE, stdout=subprocess.PIPE)
        def read_frame():
            header = worker.stdout.read(4)
            size = struct.unpack(">I", header)[0]
            return protocol.unpack_body(worker.stdout.read(size))
        try:
            self.assertEqual(read_frame()[0], b"H")
            worker.stdin.write(protocol.pack(b"B", {"id": 1, "groups": [group], "echo": True}))
            worker.stdin.flush()
            kind, response = read_frame()
            self.assertEqual(kind, b"R")
            answered = response["groups"][0]
            self.assertEqual(answered["ity"], "ntbzen t".replace(" ", ""))
            self.assertEqual(answered["iv"][-1], row[1]["content"])
            reply = v1.Reply("id", "SUMIF", [row], None, None, {}, "fake")
            reply.add({"g": [dict(answered, o=0)], "env": "fake00000001"})
            reconstructed = reply.build()["inputs"][0]
            self.assertEqual(reconstructed[0]["values"][2][0], {"type": "error", "code": "#N/A"})
            self.assertEqual(reconstructed[0]["values"][1][0], {"type": "boolean", "value": True})
            self.assertEqual(reconstructed[1], row[1])
        finally:
            worker.stdin.close()
            worker.stdout.close()
            worker.wait(timeout=5)

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
        b.requeue(batch)
        self.assertFalse(b.has_work())
        self.assertEqual(b.next_serial_batch().primer, p.primer)

    def test_add_in_pieces_wait_for_a_worker_with_exactly_those_add_ins(self):
        b = batcher.Batcher()
        p = piece("a", "x", "EUROCONVERT", 30)
        p.addins = ("EUROTOOL.XLAM",)
        b.add_piece(p)
        b.add_piece(piece("a", "y", "ABS", 5))
        self.assertFalse(b.pinned_pending(()))
        self.assertTrue(b.pinned_pending(("EUROTOOL.XLAM",)))
        plain = b.next_batch()
        self.assertEqual({g["f"] for g in plain.groups}, {"ABS"})    # ordinary workers never see it
        self.assertFalse(b.has_work())
        batch = b.next_pinned_batch(("EUROTOOL.XLAM",))
        self.assertEqual((batch.addins, batch.calls), (("EUROTOOL.XLAM",), 30))
        b.requeue(batch)                                              # a lost worker gives it back to the same queue
        self.assertTrue(b.pinned_pending(("EUROTOOL.XLAM",)))
        self.assertFalse(b.has_work())

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

    def test_isolated_and_suspect_calls_wait_for_the_isolation_worker_and_ordinary_work_passes_them(self):
        b = batcher.Batcher()
        b.isolate.add("CRASHY")
        b.add_piece(piece("a", "r1", "CRASHY", 5))
        b.add_piece(piece("a", "r2", "ABS", 10))
        batch = b.next_batch(allow_isolated=False)
        self.assertEqual([(g["f"], g["n"]) for g in batch.groups], [("ABS", 10)])
        self.assertIsNone(b.next_batch(allow_isolated=False))          # only CRASHY is left: not for a shared worker
        self.assertTrue(b.has_work())
        item = b.next_batch(allow_isolated=True)
        self.assertEqual((item.groups[0]["f"], item.calls), ("CRASHY", 1))
        self.assertTrue(item.slices[0].item.suspect)

    def test_a_suspect_item_runs_whole_and_alone_and_a_halved_item_keeps_the_suspicion(self):
        b = batcher.Batcher()
        b.add_piece(piece("a", "r1", "ABS", 8))
        first = b.next_batch()
        for sl in first.slices:
            sl.item.suspect = True
        head, tail = batcher.split_item(first.slices[0].item, 4)
        head.suspect = tail.suspect = True
        b.requeue_items([head, tail])
        b.add_piece(piece("a", "r2", "ABS", 3))
        got = b.next_batch()
        self.assertEqual((got.calls, len(got.slices)), (4, 1))

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

    def check_source(self, group):
        """The grammar and layout checks on a case that carries modules (the internal seam behind the template route)."""
        self.admission.check_expanded_vba_case(group, group["v"][0])

    def test_vba_allows_a_workbook_probe_and_a_listed_literal_object(self):
        self.check_source(vba_group())
        self.check_source(vba_group(IMPLICIT_VBA_PARAMETERS))
        self.check_source(vba_group(DICTIONARY_VBA_SOURCE))

    def refused_vba_sources(self):
        refused = [
            ("Declare", 'Private Declare Sub NativeCall Lib "fixture" ()'),
            ("PtrSafe", "PtrSafe"), ("Shell", 'sHeLl "fixture"'),
            ("CreateObject", 'Set obj = CreateObject("Unlisted.Object")'),
            ("CreateObject", 'Set obj = CreateObject(x)'),
            ("GetObject", 'Set obj = GetObject("fixture")'),
            ("Open", 'Open "fixture" For Input As #1'), ("Kill", 'Kill "fixture"'),
            ("MkDir", 'MkDir "fixture"'), ("FileCopy", 'FileCopy "a", "b"'),
            ("Dir", 'Probe = Dir("fixture")'), ("ChDir", 'ChDir "fixture"'),
            ("SendKeys", 'Application.SendKeys "fixture"'), ("Environ", 'Probe = Environ("fixture")'),
            ("VBProject", 'Set obj = ThisWorkbook.VBProject'),
            ("XMLHTTP", 'obj = XMLHTTP'), ("MsgBox", 'MsgBox "fixture"'),
            ("InputBox", 'Probe = Application.InputBox("fixture")'),
            ("Workbooks.Open", 'Workbooks.Open "C:\\outside.xlsx"'),
            ("CallByName", 'CallByName obj, "fixture", 1'),
            ("SaveAs", 'ThisWorkbook.SaveAs "fixture"'),
        ]
        for name, line in refused:
            source = f"Public Function Probe(ByVal x As Long) As Long\n{line}\nEnd Function\n"
            yield source, name
        # rw14: a name declared in another procedure must not admit an omitted global or builtin.
        smuggled = {
            "ExecuteExcel4Macro": 'Probe = ExecuteExcel4Macro("CALL(""x"")")', "Evaluate": 'Probe = Evaluate("WEBSERVICE(1)")',
            "Run": 'Run "Other"', "Workbooks": 'Workbooks.Add', "RmDir": 'RmDir "fixture"',
            "ChDrive": 'ChDrive "C"', "SetAttr": 'SetAttr "a", 1', "GetAttr": 'Probe = GetAttr("a")',
            "FileLen": 'Probe = FileLen("a")', "Reset": 'Reset', "AppActivate": 'AppActivate "a"',
            "Chr": 'Probe = Chr(65)', "ChrW": 'Probe = ChrW(65)', "OnTime": 'Application.OnTime 1, "a"',
            "FreeFile": 'Probe = FreeFile', "Quit": 'Application.Quit', "Save": 'ThisWorkbook.Save',
            "Windows": 'Windows(1).Activate', "SavePicture": 'SavePicture a, "b"',
        }
        for name, line in smuggled.items():
            yield (f"Private Sub Unused({name})\nEnd Sub\n"
                   f"Public Function Probe(ByVal x As Long) As Long\n{line}\nEnd Function\n"), name
            yield (f"Public Function Probe(ByVal x As Long) As Long\nDim {name} As Long\n{line}\nEnd Function\n"), name
            yield f"Public Function Probe(ByVal x As Long) As Long\n{line}\nEnd Function\n", name
        yield 'Public Function Probe(ByVal x As Long) As Long\nName "a" As "b"\nEnd Function\n', "Name"
        yield 'Attribute VB_Name = "x"\nPublic Function Probe(ByVal x As Long) As Long\nEnd Function\n', "Attribute"
        yield '#If True Then\nPublic Function Probe(ByVal x As Long) As Long\nEnd Function\n#End If\n', "#"
        yield 'Public Function Probe(ByVal x As Long) As Long\n\u0405hell "calc"\nEnd Function\n', "\u0405"
        yield ('Public Function Probe(ByVal x As Long) As Long\nDim Name As String\n'
               'Name "a" As "b"\nEnd Function\n'), "Name"
        yield 'Private Sub Unused(# )\nEnd Sub\nPublic Function Probe(ByVal x As Long) As Long\nProbe = #\nEnd Function\n', "#"
        yield 'Public Function Probe(ByVal x As Long) As Long\nDim #\nProbe = #\nEnd Function\n', "#"
        yield ('Private Sub Other()\nDim local1 As Long\nEnd Sub\n'
               'Public Function Probe(ByVal x As Long) As Long\nProbe = local1\nEnd Function\n'), "local1"
        # rw15 finding 1: a cell write is data, never a formula, however it is built.
        cell = 'ThisWorkbook.Worksheets("Data").Range("A1")'
        body = lambda *lines: "Public Function Probe(ByVal x As Long) As Long\n" + "\n".join(lines) + "\nEnd Function\n"
        for expression in ('"=WEBSERVICE(""http://x"")"', '"=" & "IMAGE(1)"', '"+cmd|\'/c calc\'!A0"', '"-2+3"', '"@SUM(1)"',
                           'Left("=x", 1)', 'Trim(" =1")', 'Replace("a", "a", "=")', 'Mid("a=", 2)', 'Right("a=", 1)',
                           'UCase("=a")', 'CStr("=a")', cell + ".Value2", cell + ".Text", 'Array("=a")',
                           'x & "=" & "=1"'):
            for member in ("Value", "Value2"):
                yield body(f"{cell}.{member} = {expression}"), "not plainly data"
        for lines in (('Dim y As Variant', 'y = "=1"', f'{cell}.Value2 = y'),
                      ('Dim y As String', 'y = "=" & "1"', f'{cell}.Value2 = y'),
                      ('Dim y As Variant', 'y = Trim(" =1")', f'{cell}.Value2 = y & "z"'),
                      ('Dim d As Object', 'Set d = CreateObject("Scripting.Dictionary")', 'd.Add 1, "=1"',
                       f'{cell}.Value2 = d.Item(1)'),
                      ('Dim v As Variant', 'For Each v In Array("=1")', f'{cell}.Value2 = v', 'Next v')):
            yield body(*lines), "not plainly data"
        # taint flows through a module variable, a function result and a call argument
        yield ('Private shared\nPublic Function Probe(ByVal x As Long) As Long\nshared = "=1"\n'
               f'{cell}.Value2 = shared\nEnd Function\n'), "not plainly data"
        yield ('Private Function Make() As Variant\nMake = "=1"\nEnd Function\n'
               f'Public Function Probe(ByVal x As Long) As Long\n{cell}.Value2 = Make()\nEnd Function\n'), "not plainly data"
        yield ('Private Sub Store(ByVal v As Variant)\n' + cell + '.Value2 = v\nEnd Sub\n'
               'Public Function Probe(ByVal x As Long) As Long\nStore "=1"\nEnd Function\n'), "not plainly data"
        yield ('Private Sub Store(ByVal v As Variant)\n' + cell + '.Value2 = v\nEnd Sub\n'
               'Public Function Probe(ByVal x As Long) As Long\nStore "a" & "=" & ThisWorkbook.Worksheets("Data").Range("A2").Value\n'
               'End Function\n'), "not plainly data"
        yield body(f'{cell}.NumberFormat = x'), "NumberFormat"
        yield body(f'{cell}.Formula = "=1"'), "Formula"
        yield body(f'{cell}.FormulaR1C1 = "=1"'), "FormulaR1C1"
        yield body(f'{cell}.Formula2 = "=1"'), "Formula2"
        yield body(f'{cell}.Value2 = x', 'x = "=1"'), "x"          # a parameter is never assigned
        # rw15 finding 2 and 6: no sheet creation, no templates, no other workbook, no Application.
        for line in ('ThisWorkbook.Sheets.Add(, , 1, "\\\\host\\share\\t.xltm")', 'ThisWorkbook.Worksheets.Add',
                     'ThisWorkbook.Worksheets.Add Type:=3', 'ThisWorkbook.Sheets.Add', 'ThisWorkbook.Sheets.Add 1, 2, 3, "t"',
                     'ThisWorkbook.Worksheets("Data").Copy', 'ThisWorkbook.Worksheets("Data").Delete',
                     'ThisWorkbook.Worksheets("Data").Parent.Sheets.Add', 'ThisWorkbook.Worksheets("Data").Move',
                     'Workbooks.Add', 'Application.Workbooks.Add', 'Application.Range("[Book1]S!A1").Value2 = 1',
                     'Application.Range("A1").Value2 = 1', 'Application.Calculate', 'Application.EnableEvents = True',
                     'Application.ActiveWorkbook.Close', 'ActiveWorkbook.Close', 'ActiveSheet.Range("A1").Value2 = 1',
                     'Range("A1").Value2 = 1', 'Cells(1, 1).Value2 = 1', 'Selection.Value2 = 1', 'ActiveCell.Value2 = 1',
                     'Sheets(1).Range("A1").Value2 = 1', 'Worksheets(1).Range("A1").Value2 = 1', 'Names.Add "a", "=1"',
                     'ThisWorkbook.Names.Add "a", "=1"', 'ThisWorkbook.Close', 'ThisWorkbook.Application.Quit',
                     'ThisWorkbook.Worksheets("Data").Range("A1").Parent.Parent.Close',
                     'ThisWorkbook.Worksheets("Data").Range("A1").Worksheet.Delete',
                     'ThisWorkbook.Worksheets("Data").Range("A1").Hyperlinks.Add',
                     'ThisWorkbook.Worksheets("Data").QueryTables.Add',
                     'ThisWorkbook.Connections.Add', 'ThisWorkbook.VBProject.VBComponents.Add(1)',
                     'ThisWorkbook.ChangeFileAccess 1', 'ThisWorkbook.Windows(1).Close', 'Me.Save'):
            yield body(line), line.split("(")[0].split(" ")[0].split(".")[-1] or line
        yield 'Option Private Module\n' + body("Probe = x"), "Private"
        yield 'Option Explicit\nOption Explicit\n' + body("Probe = x"), "Explicit"
        yield 'Option Base 1\n' + body("Probe = x"), "Base"
        # rw14: the omitted checks, now each its own case
        for line in ('Probe = [A1]', 'Application.OnKey "a", "b"', 'Application.DDEInitiate "a", "b"',
                     'Application.DDEExecute 1, "a"', 'Probe = FileDateTime("a")', 'Set p = LoadPicture("a")',
                     'Probe = Evaluate("1")', 'Probe = Application.Evaluate("1")', 'Application.Run "x"',
                     'Application.ExecuteExcel4Macro "x"', 'Probe = ExecuteExcel4Macro("x")', 'Workbooks.Open "x"',
                     'Dim a As Long, Name As Long', 'Probe = x: Shell "x"', 'If x = 1 Then Shell "x"',
                     'If x = 1 Then x = 2 Else Shell "x"', 'On Error GoTo Nowhere', 'GoTo 0', 'Rem hidden',
                     'With ThisWorkbook\nEnd With', 'Probe = x ^ "a"; Shell "x"', 'Probe = x # 1', 'Probe = x $',
                     'Probe = x!y', 'Probe = x@', 'Probe = &H10&', 'Stop', 'End', 'DoEvents', 'Erase x',
                     'ReDim a(3)', 'Debug.Print x', 'Static y As Long', 'Dim y As New Collection', 'Let y = 1',
                     'Randomize', 'Probe = Rnd', 'Probe = Now', 'Probe = Timer', 'Probe = CallByName(x, "a", 1)',
                     'Probe = VBA.Shell("a")', 'Probe = Excel.Application.Run("a")', 'Probe = Application.Caller',
                     'x = 1 _\n: Shell "x"'):
            yield body(line), (line.split("\n")[0].split()[0] if line.split() else line)
        # a comment ending in an underscore does not continue the line; a continuation must follow a space
        yield body("Probe = x ' comment _", 'Shell "x"'), "Shell"
        yield body("Probe = x_"), "x_"
        yield body("Probe = x+_", "1"), "_"
        yield "Public Function Probe(ByVal x As Long) As Long\r\rShell \"x\"\nEnd Function\n", "carriage"
        yield body("Probe = x") + "Shell \"x\"\n", "Shell"
        yield body("Probe = x").replace("End Function", "End Sub"), "End Sub"
        yield "Public Function Probe(ByVal x As Long) As Long\nProbe = x\n", "End"
        # entry-point and event naming
        yield "Public Sub Auto_Open()\nEnd Sub\n", "Auto_Open"
        yield "Public Sub Workbook_Open()\nEnd Sub\n", "Workbook_Open"
        yield "Public Sub Worksheet_Change(ByVal Target As Range)\nEnd Sub\n", "Worksheet_Change"

    def refused_vba_cases(self):
        """(name, group): every rw14 and rw15 bypass that is not one source line, and unsafe layouts."""
        yield from ((name.encode("ascii", "backslashreplace").decode(), vba_group(source))
                    for source, name in self.refused_vba_sources())
        probe = "Public Function Probe(ByVal x As Long) As Long\nProbe = x\nEnd Function\n"
        evil = 'Public Function Probe(ByVal x As Long) As Long\nName "a" As "b"\nEnd Function\n'
        standard = lambda name, source=evil: {"name": name, "kind": "standard", "source": source}
        # rw15 finding 3: a module's name never adds to what a probe may say.
        for name in ("Name", "ThisWorkbook", "Sheet1", "Err", "Application", "CreateObject", "Open", "name", "A_b", "X" * 30):
            yield "module name", vba_group(modules=[standard("Probes", probe), standard(name)])
        yield "module name", vba_group(modules=[standard("Probes", probe),
                                                {"name": "Foo", "kind": "document", "source": "Option Explicit\n"}])
        yield "module name", vba_group(modules=[standard("Probes", probe),
                                                {"name": "Sheet2", "kind": "document", "source": "Option Explicit\n"}])
        # rw14 finding 3 and rw15 finding 5: no package travels with the case; the worker builds it.
        yield "case fields", vba_group(workbook="UEsDBA==")
        yield "case fields", vba_group(workbook=base64.b64encode(b"PK\x03\x04" + bytes(100)).decode())
        yield "case fields", {**vba_group(), "v": [{k: v for k, v in vba_group()["v"][0].items() if k != "event_log"}]}
        layouts = [
            ("function", {"A2": {"type": "formula", "formula": '=WEBSERVICE("http://x")'}}, {}),
            ("function", {"A2": {"type": "formula", "formula": '=IMAGE("http://x")'}}, {}),
            ("function", {"A2": {"type": "formula", "formula": '=Unknown(1)'}}, {}),
            ("function", {"A2": {"type": "formula", "formula": '=_xlfn.RTD("a","b","c")'}}, {}),
            ("function", {"A2": {"type": "formula", "formula": '=_xlfn._xlws.WEBSERVICE("a")'}}, {}),
            ("external", {"A2": {"type": "formula", "formula": "=[Book.xlsx]Sheet1!A1"}}, {}),
            ("quoted name", {"A2": {"type": "formula", "formula": "='C:\\x\\[b.xlsx]S'!A1"}}, {}),
            ("function", {"A2": {"type": "number", "bits": ONE}}, {"n": "=EXEC(\"calc\")"}),
            ("external", {"A2": {"type": "number", "bits": ONE}}, {"n": "=[Book.xlsx]Sheet1!A1"}),
            # rw15 finding 4: a quote inside a sheet name must not hide a call from the scan.
            ("quoted name", {"A2": {"type": "formula", "formula": "='a\"b'!A1+WEBSERVICE(\"http://x\")"}}, {}),
            ("quoted name", {"A2": {"type": "formula", "formula": "='a\"b'!A1+CALL(\"x\")"}}, {}),
            ("quoted name", {"A2": {"type": "formula", "formula": "='a\"b'!A1+[Book.xlsx]S!A1&\"x\""}}, {}),
            ("quoted name", {"A2": {"type": "formula", "formula": "=Data!A1+'it''s'!A1"}}, {}),
            ("unterminated", {"A2": {"type": "formula", "formula": "=Data!A1+\"x"}}, {}),
            ("quoted name", {"A2": {"type": "formula", "formula": "=cmd|'/c calc'!A0"}}, {}),
            ("external", {"A2": {"type": "formula", "formula": "=cmd|Data!A0"}}, {}),
            ("function", {"A2": {"type": "formula", "formula": '=IMPORTCSV("x")'}}, {}),
            ("function", {"A2": {"type": "formula", "formula": '=IMPORTTEXT("x")'}}, {}),
            ("function", {"A2": {"type": "formula", "formula": '=INDIRECT("[a]b!A1")'}}, {}),
            ("function", {"A2": {"type": "formula", "formula": '=CUBEVALUE("x")'}}, {}),
            ("function", {"A2": {"type": "formula", "formula": '=INFO("directory")'}}, {}),
            ("function", {"A2": {"type": "formula", "formula": '=CELL("filename")'}}, {}),
        ]
        for message, cells, names in layouts:
            yield message, vba_group(sheets=[{"name": "Data", "cells": cells}], names=names)
        for bad in ('a"b', "a'b", "a|b", "a!b", "a[b]", "a\\b", "a" * 32, "", " a"):
            yield "layout sheet", vba_group(sheets=[{"name": bad, "cells": {}}], readback=[], event_log=[])
        for name in ("Auto_Open", "a b", "[x]", "_xlnm.Print_Area"):
            yield "defined name", vba_group(names={name: "=1"})
        # rw15 finding 6 and the worker-parity gaps: extra keys and hidden positions.
        yield "layout sheet", vba_group(sheets=[{"name": "Data", "cells": {}, "extra": 1}], readback=[], event_log=[])
        yield "hidden", vba_group(sheets=[{"name": "Data", "cells": {}, "hidden_rows": [0]}], readback=[], event_log=[])
        yield "hidden", vba_group(sheets=[{"name": "Data", "cells": {}, "hidden_columns": [16385]}], readback=[], event_log=[])
        # Text arguments reach cells through the probe: data only.
        for text in ("=1+1", "+1", "-1", "@SUM(1)"):
            yield "text arguments", vba_group(calls=[{"procedure": "Probes.Probe", "args": [{"type": "text", "value": text}]}])

    def refused_rw16_cases(self):
        """rw16 (dc-5qqoz): the two bypasses and the parity differentials, in one list for both validators."""
        write = 'ThisWorkbook.Worksheets("Data").Range("B1").Value2 = '
        probe = lambda line, sig="ByVal x As Long": f"Public Function Probe({sig}) As Long\n{line}\nProbe = 1\nEnd Function\n"
        # 1: a probe function named like an admitted non-Excel worksheet function is callable from a layout formula.
        for name in ("FLATTEN", "HAS", "HASANY", "HASALL", "flatten", "Has"):
            hijack = f"Public Function {name}(ByVal x As Variant) As Variant\n{write}x\n{name} = x\nEnd Function\n"
            yield "collides", vba_group(hijack + probe(""),
                                        sheets=[{"name": "Data", "cells": {
                                            "A1": {"type": "formula", "formula": f'={name.upper()}("=WEBSERVICE(""http://x"")")'}}}])
            yield "collides", vba_group(hijack + probe(""))
        # 2: a number stringifies with a leading minus, which a cell takes as a formula.
        for expression in ('Left(-1, 1) & "WEBSERVICE(""http://x"")"', '(-1) & "*WEBSERVICE(""http://x"")"',
                           'xlUp & "x"', '(Not 0) & "x"', '&H8000 & "x"', '(1 - 2) & "x"', 'x & "x"',
                           'CStr(-1) & "x"', 'CStr(x) & "x"', 'Abs(x) & "x"', 'Round(x) & "x"', 'Left(x, 1) & "x"',
                           'UCase(CStr(-1)) & "x"', '-x & "x"', 'CLng(x) & "x"', 'Val("1") & "x"'):
            yield "plainly data", vba_group(probe(write + expression))
        # a Variant parameter takes a negative number argument from the case
        yield "plainly data", vba_group(probe(write + 'v & "x"', "ByVal v"),
                                        calls=[{"procedure": "Probes.Probe", "args": [number("bff0000000000000")]}])
        # parity: the budget, a null mode, document-module casing, layout booleans
        yield "frame budget", vba_group(sheets=[{"name": "Data", "cells": {"A1": {"type": "text", "value": "\n" * 7000}}}])
        yield "range cell", vba_group(calls=[{"procedure": "Probes.Probe", "args": [{"type": "text", "value": "a", "mode": None}]}])
        yield "range cell", vba_group(calls=[{"procedure": "Probes.Probe", "args": [{"type": "boolean", "value": True, "mode": None}]}])
        for name in ("thisworkbook", "THISWORKBOOK", "sheet1", "SHEET1"):
            yield "module name", vba_group(modules=[
                {"name": "Probes", "kind": "standard", "source": DEFAULT_VBA_SOURCE},
                {"name": name, "kind": "document", "source": "Option Explicit\n"}])
        for value in ("yes", 1, None):
            yield "layout boolean", vba_group(sheets=[{"name": "Data", "cells": {"A1": {"type": "boolean", "value": value}}}])
        yield "layout boolean", vba_group(sheets=[{"name": "Data", "cells": {"A1": {"type": "boolean"}}}])

    def template_group(self, **changes):
        smoke = json.loads((HERE / "vba-smoke.json").read_text(encoding="ascii"))
        smoke.update(changes)
        smoke = {"names": {}, **smoke}
        return {"f": "_VBA", "k": "V", "n": 1, "o": 0, "v": [smoke]}

    def rw18_free_source_group(self):
        """dc-45ndr: the review's request, no template and free module source."""
        return {"f": "_VBA", "k": "V", "n": 1, "o": 0, "v": [{
            "sheets": [{"name": "Data", "cells": {"A1": number(ONE)}}], "names": {},
            "modules": [{"name": "Probes", "kind": "standard",
                         "source": "Public Function Probe(ByVal x As Long) As Long\n    Probe = x + 1\nEnd Function\n"}],
            "calls": [{"procedure": "Probes.Probe", "args": [number(ONE)]}],
            "readback": ["Data!A1"], "event_log": []}]}

    def test_vba_refuses_a_request_without_a_template(self):
        group = self.rw18_free_source_group()
        with self.assertRaisesRegex(ValueError, "(?i)template"):
            self.admission.check(group)
        template_only = self.template_group()
        del template_only["v"][0]["params"]
        with self.assertRaisesRegex(ValueError, "(?i)template"):
            self.admission.check(template_only)
        with self.assertRaisesRegex(ValueError, "(?i)template"):
            protocol.vba_case(group["v"][0])
        with self.assertRaisesRegex(ValueError, "(?i)template"):
            protocol.vba_case({k: v for k, v in group["v"][0].items() if k != "modules"})

    @unittest.skipUnless(os.environ.get("TOUCHSTONE_VBA_POLICY_WORKER"), "set path to the built Excel worker")
    def test_vba_worker_refuses_a_request_without_a_template(self):
        result = subprocess.run([os.environ["TOUCHSTONE_VBA_POLICY_WORKER"], "--check-vba-policy"],
                                input=json.dumps(self.rw18_free_source_group()), text=True, capture_output=True, timeout=30)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("template", result.stderr.lower())

    @unittest.skipUnless(os.environ.get("TOUCHSTONE_VBA_POLICY_WORKER"), "set path to the built Excel worker")
    def test_vba_worker_digest_binds_the_template_name_and_parameters(self):
        worker = os.environ["TOUCHSTONE_VBA_POLICY_WORKER"]

        def digest(group):
            result = subprocess.run([worker, "--check-vba-policy"], input=json.dumps(group), text=True,
                                    capture_output=True, timeout=30)
            self.assertEqual(result.returncode, 0, result.stderr)
            return result.stdout.strip()
        self.assertNotEqual(digest(self.template_group()), "68d83307c26f4e147fab56ea90b4c72f4502523a51d4c93c91c1ba8fad4c57c3")
        other = self.template_group()
        other["v"][0]["params"]["message"] = "other"
        self.assertNotEqual(digest(other), digest(self.template_group()))

    def refused_template_cases(self):
        """dc-erfm3: the template route refuses anything the reviewed catalogue does not name."""
        params = json.loads((HERE / "vba-smoke.json").read_text(encoding="ascii"))["params"]
        yield "unknown", self.template_group(template="nosuch")
        yield "do not match", self.template_group(params={k: v for k, v in params.items() if k != "message"})
        yield "do not match", self.template_group(params={**params, "extra": "x"})
        for bad in ("=WEBSERVICE(1)", "+1", "-1", "@SUM(1)", "", "x" * 256, "caf\u00e9", "line\nbreak", 7, None):
            yield "text parameter", self.template_group(params={**params, "message": bad})
        yield "case sheet", self.template_group(params={**params, "target_sheet": "Nowhere"})
        yield "case sheet", self.template_group(params={**params, "event_sheet": 'Data"'})
        for bad in ("A0", "$A$1", "a1", "XFE1", "A1048577", "Data!A1", 'A1"', 5):
            yield "one a1 cell", self.template_group(params={**params, "target_cell": bad})
        yield "entry point", self.template_group(calls=[{"procedure": "Module1.Other", "args": []}])
        yield "case fields", self.template_group(modules=[])
        yield "case fields", {**self.template_group(), "v": [{k: v for k, v in self.template_group()["v"][0].items() if k != "sheets"}]}

    def test_vba_template_route_expands_the_smoke_case_to_the_reviewed_source(self):
        group = self.template_group()
        self.admission.check(group)
        import vba_templates
        smoke_source = json.loads((HERE / "vba-smoke-source.json").read_text(encoding="ascii"))
        expanded = vba_templates.expand(group["v"][0])
        self.assertEqual(expanded["modules"], smoke_source["modules"])
        self.assertEqual({k: v for k, v in expanded.items() if k != "modules"},
                         {"names": {}, **{k: v for k, v in smoke_source.items() if k != "modules"}})

    def test_vba_template_text_is_quoted_as_one_string_literal(self):
        import vba_templates
        group = self.template_group()
        group["v"][0]["params"]["message"] = 'say "hi" {{target_cell}}'
        self.admission.check(group)
        source = vba_templates.expand(group["v"][0])["modules"][0]["source"]
        self.assertIn('Err.Raise 5, "Module1", "say ""hi"" {{target_cell}}"', source)

    def test_vba_template_route_refuses_what_the_catalogue_does_not_name(self):
        for message, group in self.refused_template_cases():
            with self.subTest(message=message, case=json.dumps(group["v"][0].get("params"))[:120]), \
                    self.assertRaisesRegex(ValueError, "(?i)" + re.escape(message)):
                self.admission.check(group)

    def test_vba_template_files_match_their_pinned_hashes_and_a_changed_file_is_refused(self):
        import hashlib, vba_templates
        for name, entry in vba_templates.CATALOGUE["templates"].items():
            for module in entry["modules"]:
                data = (vba_templates.DIRECTORY / module["file"]).read_bytes()
                self.assertEqual(hashlib.sha256(data).hexdigest(), module["sha256"], module["file"])
        saved = vba_templates.CATALOGUE["templates"]["smoke"]["modules"][0]["sha256"]
        try:
            vba_templates.CATALOGUE["templates"]["smoke"]["modules"][0]["sha256"] = "0" * 64
            with self.assertRaisesRegex(ValueError, "pinned hash"):
                self.admission.check(self.template_group())
        finally:
            vba_templates.CATALOGUE["templates"]["smoke"]["modules"][0]["sha256"] = saved

    @unittest.skipUnless(os.environ.get("TOUCHSTONE_VBA_POLICY_WORKER"), "set path to the built Excel worker")
    def test_vba_worker_template_route_agrees_with_the_policy(self):
        worker = os.environ["TOUCHSTONE_VBA_POLICY_WORKER"]

        def run(group):
            return subprocess.run([worker, "--check-vba-policy"], input=json.dumps(group), text=True,
                                  capture_output=True, timeout=60)
        self.assertEqual(run(self.template_group()).returncode, 0)
        for message, group in self.refused_template_cases():
            with self.subTest(message=message, case=json.dumps(group["v"][0].get("params"))[:120]):
                result = run(group)
                self.assertEqual(result.returncode, 1, result.stderr)
                self.assertIn(message.lower(), result.stderr.lower())

    def refused_rw17_cases(self):
        """rw17 (dc-erfm3): the four bypasses, in one list for both validators."""
        write = 'ThisWorkbook.Worksheets("Data").Range("B1").Value2 = '
        probe = lambda line, sig="ByVal x As Long": f"Public Function Probe({sig}) As Long\n{line}\nProbe = 1\nEnd Function\n"
        # 1: a Private Function (and a dotted name's last part) is still callable from a worksheet formula.
        for name in ("FLATTEN", "HAS", "HASANY", "HASALL"):
            hijack = f"Private Function {name}(ByVal x As Variant) As Variant\n{write}x\n{name} = x\nEnd Function\n"
            yield "collides", vba_group(hijack + probe(""),
                                        sheets=[{"name": "Data", "cells": {
                                            "A1": {"type": "formula", "formula": f'={name}("=WEBSERVICE(""http://x"")")'}}}])
        yield "collides", vba_group(modules=[{"name": "Mode", "kind": "standard",
                                              "source": "Public Function Sngl(ByVal x As Variant) As Variant\nSngl = x\nEnd Function\n"
                                                        + probe("")}],
                                    calls=[{"procedure": "Mode.Probe", "args": [number(ONE)]}])
        # 2: a TEXT that can be empty at run time does not make the leading operand safe.
        for lead in ('""', 'vbNullString', 'Empty', 'Null', 'Left("a", 0)', 'e', 's', 'v'):
            for rest in ('(-1) & "x"', 'CStr(-1) & "x"', 'x & "x"'):
                decl = {"e": 'Const e = ""\n', "s": 'Dim s As String\n', "v": 'Dim v As Variant\n'}.get(lead, "")
                yield "plainly data", vba_group(probe(decl + write + lead + " & " + rest))
        yield "plainly data", vba_group(probe(write + 'Left("" & (-1), 1) & "x"'))
        # 3: + on a number, a boolean or a numeric string adds.
        for expression in ('True + 0 & "x"', 'Left(True + 0, 1) & "x"', '(True + True) & "x"', '(" -1" + 0) & "x"',
                           '("&H8000" + 0) & "x"', 'Left("&HFFFF" + 0, 1) & "x"', '("0" + True) & "x"'):
            yield "plainly data", vba_group(probe(write + expression))
        # 4: a negative hex Const and a numeric For variable are numbers.
        yield "plainly data", vba_group(probe('Const h = &H8000\n' + write + 'Left(h, 1) & "x"'))
        yield "plainly data", vba_group(probe('Const h = &HFFFF\n' + write + 'h & "x"'))
        yield "plainly data", vba_group(probe('Dim i As Variant\nFor i = -1 To -1\n' + write + 'i & "x"\nNext i'))
        yield "plainly data", vba_group(probe('Dim i As Variant\nFor i = -1 To -1\n' + write + 'Left(i, 1) & "x"\nNext i'))
        yield "plainly data", vba_group(probe('Dim s As String\ns = -1\n' + write + 's'))
        # parity: non-BMP text counts the way the worker counts it (UTF-16 units)
        yield "frame budget", vba_group(sheets=[{"name": "Data", "cells": {"A1": {"type": "text", "value": "\U0001F600" * 3500}}}])
        yield "invalid range", vba_group(calls=[{"procedure": "Probes.Probe", "args": [{"type": "text", "value": "\U0001F600" * 600}]}])

    def test_vba_refuses_the_rw17_bypasses(self):
        for message, group in self.refused_rw17_cases():
            with self.subTest(message=message, case=json.dumps(group["v"][0]["modules"])[:200]), \
                    self.assertRaisesRegex(ValueError, "(?i)" + re.escape(message)):
                self.check_source(group)

    def test_vba_refuses_the_rw16_bypasses_and_parity_differentials(self):
        for message, group in self.refused_rw16_cases():
            with self.subTest(message=message, case=json.dumps(group["v"][0]["modules"])[:200]), \
                    self.assertRaisesRegex(ValueError, "(?i)" + re.escape(message)):
                self.check_source(group)

    def test_vba_accepts_what_the_rw16_rules_leave_alone(self):
        write = 'ThisWorkbook.Worksheets("Data").Range("B1").Value2 = '
        for line in (write + 'x', write + '-x', write + 'x + 1', write + '"a" & x', write + '"a" & CStr(-1)',
                     write + 'CStr("a") & x', write + 'Len("a")', write + '"a" & xlUp'):
            with self.subTest(line=line):
                self.check_source(vba_group(
                    f"Public Function Probe(ByVal x As Long) As Long\n{line}\nProbe = 1\nEnd Function\n"))

    def test_vba_refuses_package_unsafe_layout_and_module_names(self):
        for message, group in self.refused_vba_cases():
            with self.subTest(message=message), self.assertRaisesRegex(ValueError, "(?i)" + re.escape(message)):
                self.check_source(group)
        self.check_source(vba_group(sheets=[{"name": "Data", "cells": {
            "A2": {"type": "formula", "formula": '=SUM(A1,1)&"[x]"'}}}]))
        self.check_source(vba_group(sheets=[{"name": "My Data", "cells": {
            "A2": {"type": "formula", "formula": "='My Data'!A1+Data!A1"}}}, {"name": "Data", "cells": {}}]))

    def test_the_reviewed_smoke_case_is_accepted(self):
        smoke = json.loads((HERE / "vba-smoke.json").read_text(encoding="ascii"))
        group = {"f": "_VBA", "k": "V", "n": 1, "o": 0, "v": [protocol.vba_case(smoke)["value"]]}
        self.admission.check(group)

    @staticmethod
    def accepted_cell_writes():
        return ('ThisWorkbook.Worksheets("Data").Range("A1").Value2 = "plain"',
                     'ThisWorkbook.Worksheets("Data").Range("A1").Value = 1.5',
                     'ThisWorkbook.Worksheets("Data").Range("A1").Value2 = x',
                     'ThisWorkbook.Worksheets("Data").Range("A1").Value2 = "a" & CStr(x)',

                     'ThisWorkbook.Worksheets("Data").Cells(1, 2).Value2 = CDbl(x) + 1',
                     'ThisWorkbook.Worksheets(1).Range("A1").Offset(1, 0).Resize(1, 1).Value2 = True')

    def test_text_that_is_data_is_accepted_as_a_cell_write(self):
        for line in self.accepted_cell_writes():
            with self.subTest(line=line):
                self.check_source(vba_group(
                    f"Public Function Probe(ByVal x As Long) As Long\n{line}\nProbe = x\nEnd Function\n"))

    def test_vba_refuses_a_case_larger_than_one_submit_frame(self):
        big = {"type": "text", "value": "x" * 1000}
        calls = [{"procedure": "Probes.Probe", "args": [big] * 16} for _ in range(8)]
        with self.assertRaisesRegex(ValueError, "frame budget"):
            self.check_source(vba_group(calls=calls))
        protocol.submit_frames("0" * 32, [vba_group()])      # an admitted case always fits

    def test_vba_accepts_multiple_declarations_module_variables_and_tab_continuations(self):
        for source in (MULTI_DECLARATION_VBA_SOURCE, MODULE_VARIABLE_VBA_SOURCE, TAB_CONTINUATION_VBA_SOURCE):
            with self.subTest(source=source):
                self.check_source(vba_group(source))

    def test_vba_checks_the_case_shape_limits_and_call_destination(self):
        source = "Public Function Probe(ByVal x As Long) As Long\nProbe = x\nEnd Function\n"
        for detail, group in [
            ("one VBA case", None),
            ("module source", vba_group(source + "'" + "x" * 20000)),
            ("procedure", vba_group(calls=[{"procedure": "Other.Probe", "args": []}])),
            ("readback", vba_group(readback=["Other!A1"])),
            ("event_log", vba_group(event_log=["Data!A:A"])),
        ]:
            with self.subTest(detail=detail), self.assertRaisesRegex(ValueError, detail):
                if group is None:
                    self.admission.check(dict(self.template_group(), n=2))
                else:
                    self.check_source(group)

    @unittest.skipUnless(os.environ.get("TOUCHSTONE_VBA_POLICY_WORKER"), "set path to the built Excel worker")
    def test_vba_worker_refuses_what_the_policy_refuses_and_accepts_what_it_accepts(self):
        worker = os.environ["TOUCHSTONE_VBA_POLICY_WORKER"]

        def run(group):
            return subprocess.run([worker, worker_flag(group)], input=json.dumps(group), text=True,
                                  capture_output=True, timeout=30)
        smoke = json.loads((HERE / "vba-smoke.json").read_text(encoding="ascii"))
        accepted = [vba_group(source) for source in (None, *ACCEPTED_VBA_SOURCES)]
        accepted.append({"f": "_VBA", "k": "V", "n": 1, "o": 0, "v": [protocol.vba_case(smoke)["value"]]})
        for line in self.accepted_cell_writes():
            accepted.append(vba_group(
                f"Public Function Probe(ByVal x As Long) As Long\n{line}\nProbe = x\nEnd Function\n"))
        accepted.append(vba_group(sheets=[{"name": "My Data", "cells": {
            "A2": {"type": "formula", "formula": "='My Data'!A1+Data!A1"}}}, {"name": "Data", "cells": {}}]))
        for group in accepted:
            with self.subTest(accepted=str(group["v"][0].get("template") or group["v"][0]["modules"][0]["source"][:40])):
                result = run(group)
                self.assertEqual(result.returncode, 0, result.stderr)
        for message, group in (*self.refused_vba_cases(), *self.refused_rw16_cases(), *self.refused_rw17_cases()):
            with self.subTest(message=message):
                result = run(group)
                self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
                self.assertIn(message.lower(), result.stderr.lower())
        long = run(vba_group('Public Sub Probe()\n' + "'" + "x" * 20000))
        self.assertEqual(long.returncode, 1)
        self.assertIn("module source", long.stderr)

    @unittest.skipUnless(os.environ.get("TOUCHSTONE_VBA_POLICY_WORKER"), "set path to the built Excel worker")
    def test_vba_worker_refuses_a_duplicate_json_key(self):
        text = json.dumps(vba_group())
        duplicated = text.replace('"names": {}', '"names": {"Auto_Open": "=1"}, "names": {}')
        self.assertNotEqual(text, duplicated)
        result = subprocess.run([os.environ["TOUCHSTONE_VBA_POLICY_WORKER"], "--check-vba-expanded"], input=duplicated,
                                text=True, capture_output=True, timeout=30)
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)

    @unittest.skipUnless(os.environ.get("TOUCHSTONE_VBA_POLICY_WORKER"), "set path to the built Excel worker")
    def test_vba_worker_and_policy_agree_on_mutated_sources(self):
        """The two implementations of the grammar accept and refuse the same sources (messages may differ)."""
        import random
        worker = os.environ["TOUCHSTONE_VBA_POLICY_WORKER"]
        pool = ["", "x", "1", '"=a"', '"a"', "Value2", ".", "(", ")", ",", "&", "+", "-", "=", "Then", "End If", "Dim", "As", "Long",
                "Set", "Not", "If", "Else", "For", "Next", "To", "Err", "ThisWorkbook", "Worksheets", "Range", "Shell", "_", ":", "'",
                "Sub", "Function", "Probe", "CStr", "Left", "True", "Nothing", "Call", "Exit", "GoTo", "100", "On Error Resume Next"]
        seeds = [DEFAULT_VBA_SOURCE, IMPLICIT_VBA_PARAMETERS, DICTIONARY_VBA_SOURCE, MULTI_DECLARATION_VBA_SOURCE,
                 MODULE_VARIABLE_VBA_SOURCE, TAB_CONTINUATION_VBA_SOURCE,
                 'Public Function Probe(ByVal x As Long) As Long\nDim i As Long\nFor i = 1 To 3\nIf i = 2 Then\nProbe = Probe + i\n'
                 'Else\nProbe = Probe - 1\nEnd If\nNext i\nOn Error GoTo 9\nErr.Raise 5\n9:\nSelect Case x\nCase 1, 2\nProbe = 1\n'
                 'Case Else\nProbe = 2\nEnd Select\nEnd Function\n']
        rng = random.Random(20261126)
        cases = []
        for _ in range(240):
            tokens = rng.choice(seeds).replace("\r\n", "\n").split(" ")
            for _ in range(rng.choice((1, 1, 2, 3))):
                at = rng.randrange(len(tokens))
                action = rng.choice(("replace", "insert", "delete"))
                if action == "replace":
                    tokens[at] = rng.choice(pool)
                elif action == "insert":
                    tokens.insert(at, rng.choice(pool))
                elif len(tokens) > 3:
                    del tokens[at]
            cases.append(" ".join(tokens))
        disagreements, accepted = [], 0
        for source in cases:
            group = vba_group(source)
            try:
                self.check_source(group)
                expected = 0
            except ValueError:
                expected = 1
            result = subprocess.run([worker, "--check-vba-expanded"], input=json.dumps(group), text=True,
                                    capture_output=True, timeout=30)
            accepted += expected == 0
            if result.returncode != expected:
                disagreements.append((source, expected, result.returncode, result.stderr))
        self.assertEqual(disagreements, [])
        self.assertGreater(accepted, 5)          # the mutations are not all refused

    @unittest.skipUnless(os.environ.get("TOUCHSTONE_VBA_POLICY_WORKER"), "set path to the built Excel worker")
    def test_vba_worker_accepts_only_the_exact_approved_case(self):
        worker = os.environ["TOUCHSTONE_VBA_POLICY_WORKER"]
        with tempfile.TemporaryDirectory() as directory:
            def check(group, verify=True):
                command = [worker, "--check-vba-policy"] + ([directory] if verify else [])
                return subprocess.run(command, input=json.dumps(group), text=True, capture_output=True, timeout=30)

            group = template_vba_group()
            digest = check(group, verify=False)
            self.assertEqual(digest.returncode, 0, digest.stderr)
            self.assertRegex(digest.stdout.strip(), r"^[0-9a-f]{64}$")
            self.assertIn("unapproved", check(group).stderr)
            (Path(directory) / (digest.stdout.strip() + ".approved")).touch()
            self.assertEqual(check(group).returncode, 0)
            reordered = json.loads(json.dumps(group, sort_keys=True))
            self.assertEqual(check(reordered).returncode, 0)
            other_param = template_vba_group()
            other_param["v"][0]["params"]["message"] = "other"
            for changed in (template_vba_group(readback=["Data!A2"]),
                            template_vba_group(calls=[{"procedure": "Module1.Probe", "args": [number(TWO)]}]),
                            other_param):
                self.assertIn("unapproved", check(changed).stderr)

    @unittest.skipUnless(os.environ.get("TOUCHSTONE_VBA_POLICY_WORKER"), "set path to the built Excel worker")
    def test_vba_worker_builds_the_package_from_the_modules(self):
        import vba_package_reader as reader
        worker = os.environ["TOUCHSTONE_VBA_POLICY_WORKER"]

        def build(group):
            with tempfile.TemporaryDirectory() as directory:
                out = Path(directory) / "probe.xlsm"
                result = subprocess.run([worker, worker_flag(group, "build"), str(out)], input=json.dumps(group),
                                        text=True, capture_output=True, timeout=30)
                self.assertEqual(result.returncode, 0, result.stderr)
                return reader.read_package(out.read_bytes())
        # a long module: one full chunk and a final chunk of 3642..4095 bytes decompress to exactly the source
        for length in (100, 3700, 4095, 4096, 4097, 5000, 8000):
            filler = "".join(f"' line {i % 97} of the probe\r\n" for i in range(400))[:length]
            source = "Public Function Probe(ByVal x As Long) As Long\r\nProbe = x\r\nEnd Function\r\n" + filler.rsplit("\r\n", 1)[0] + "\r\n"
            with self.subTest(length=length):
                group = vba_group(source)
                parts = build(group)
                self.assertEqual(sorted(parts), ["[Content_Types].xml", "_rels/.rels", "xl/_rels/workbook.xml.rels",
                                                 "xl/sharedStrings.xml", "xl/styles.xml", "xl/vbaProject.bin",
                                                 "xl/workbook.xml", "xl/worksheets/sheet1.xml"])
                streams = reader.read_compound_file(parts["xl/vbaProject.bin"])
                self.assertEqual(sorted(streams), ["PROJECT", "PROJECTwm", "VBA/Probes", "VBA/Sheet1",
                                                   "VBA/ThisWorkbook", "VBA/_VBA_PROJECT", "VBA/dir"])
                self.assertEqual(reader.decompress_vba_stream(streams["VBA/Probes"]),
                                 b'Attribute VB_Name = "Probes"\r\n' + source.replace("\r\n", "\n").replace("\n", "\r\n").encode())
                self.assertEqual(reader.decompress_vba_stream(streams["VBA/Sheet1"]).startswith(b'Attribute VB_Name = "Sheet1"'), True)
                self.assertNotIn(b"Shell", parts["xl/vbaProject.bin"])
        # two sheets, a document module for each, and the smoke case's own modules
        smoke = json.loads((HERE / "vba-smoke.json").read_text(encoding="ascii"))
        parts = build({"f": "_VBA", "k": "V", "n": 1, "o": 0, "v": [protocol.vba_case(smoke)["value"]]})
        streams = reader.read_compound_file(parts["xl/vbaProject.bin"])
        self.assertEqual(sorted(streams), ["PROJECT", "PROJECTwm", "VBA/Module1", "VBA/Sheet1", "VBA/Sheet2",
                                           "VBA/ThisWorkbook", "VBA/_VBA_PROJECT", "VBA/dir"])
        self.assertIn(b'<sheet name="Data" sheetId="1" r:id="rId1"/><sheet name="Events" sheetId="2" r:id="rId5"/>',
                      parts["xl/workbook.xml"])
        self.assertIn(b"Worksheet_Change", reader.decompress_vba_stream(streams["VBA/Sheet1"]))

    def test_the_package_reader_refuses_the_rw15_parser_differentials(self):
        import vba_package_reader as reader
        # a chunk that decompresses to 4099 bytes
        with self.assertRaisesRegex(ValueError, "4096"):
            reader.decompress_vba_stream(b"\x01\x03\xb0\x02a\xff\x0f")
        with self.assertRaisesRegex(ValueError, "compound file"):
            reader.read_compound_file(b"x" * 4096)

    def test_sheet_cases_travel_as_one_token_and_are_bounded(self):
        case = protocol.sheet_case([{"name": "Data", "cells": {"B2": number(ONE), "C3": {"type": "text", "value": "x"},
                                                             "D4": {"type": "formula", "formula": "=B2*2"}},
                                     "hidden_rows": [3]}], [{"at": "Data!F6", "formula": "=ROW(B2:B3)"}])
        [group] = protocol.rows_to_groups(protocol.CASE_FUNCTION, [[case], [case]])
        self.assertEqual((group["f"], group["k"], group["n"]), ("_CASE", "L", 2))
        self.admission.check(group)
        self.assertEqual(protocol.group_to_rows(group)[0], [case])
        for bad in ({**case["value"], "probes": []}, {**case["value"], "probes": [{"at": "F6", "formula": "=1"}]},
                    {**case["value"], "sheets": [{"name": "S", "cells": {"A1": {"type": "array"}}}]},
                    {**case["value"], "sheets": [{"name": "S", "cells": {"A1": {"type": "error", "code": "#SPILL!"}}}]}):
            with self.assertRaises(ValueError):
                self.admission.check({"f": "_CASE", "k": "L", "n": 1, "v": [bad]})
        answer = protocol.answer_to_typed("c", {"probes": [
            {"ty": "n", "v": ONE, "spill": {"r": 2, "c": 1, "ty": "nn", "v": [ONE, TWO]}}, {"refused": "formula"}]})
        self.assertEqual(answer["probes"][0]["value"], number(ONE))
        self.assertEqual(answer["probes"][0]["spill"]["rows"], 2)
        self.assertEqual(answer["probes"][1], {"type": "refused", "detail": "formula"})

    def test_a_typed_entry_probe_is_any_text_and_answers_its_number_format(self):
        sheets = [{"name": "S", "cells": {}}]
        for probes in ([{"at": "S!A1", "formula": "5%", "entry": "local"}, {"at": "S!B1", "formula": "=A1"}],
                       [{"at": "S!A1", "formula": "+abc", "entry": "local"}]):
            self.admission.check({"f": "_CASE", "k": "L", "n": 1, "v": [protocol.sheet_case(sheets, probes)["value"]]})
        for bad in ([{"at": "S!A1", "formula": "5%"}], [{"at": "S!A1", "formula": "5%", "entry": "value"}],
                    [{"at": "S!A1", "formula": "", "entry": "local"}], [{"at": "S!A1", "formula": "x" * 256, "entry": "local"}]):
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                self.admission.check({"f": "_CASE", "k": "L", "n": 1, "v": [protocol.sheet_case(sheets, bad)["value"]]})
        answer = protocol.answer_to_typed("c", {"probes": [{"ty": "n", "v": ONE, "spill": None, "nf": "0%"},
                                                          {"ty": "n", "v": ONE, "spill": None}]})
        self.assertEqual(answer["probes"][0]["number_format"], "0%")
        self.assertNotIn("number_format", answer["probes"][1])

    def group(self, **changes):
        g = {"f": "ERF.PRECISE", "k": "n", "n": 2, "v": [ONE, TWO]}
        g.update(changes)
        return g

    def test_admits_every_function_but_remote_services_and_refuses_malformed_groups(self):
        # Owner 2026-11-12: a test service on our own Excel admits every function, text and array argument;
        # only the remote service functions (deferred in CalcFunc) are skipped.
        self.admission.check(self.group())
        for good in (self.group(f="CALL"), self.group(f="IMAGE"), self.group(f="WEBSERVICE"), self.group(f="INDIRECT"),
                     self.group(f="NOW", k="", n=1, v=[]),
                     self.group(f="LEN", k="s", n=1, v=["C:/secret.xlsx"]), self.group(f="LEN", k="s", n=1, v=["[book]A1"]),
                     self.group(f="LEN", k="s", n=1, v=["https://x.example"]), self.group(f="LEN", k="s", n=1, v=["a\u0001b"]),
                     self.group(f="SUMIF", k="a1x2,S", n=1, v=[ONE, TWO, ">1"]),
                     self.group(f="FILTER", k="a2x1,a2x1", n=1, v=[ONE, TWO, ONE, ONE], spill=True)):
            with self.subTest(good=good):
                self.admission.check(good)
        for bad in (self.group(f="TRANSLATE"), self.group(f="STOCKHISTORY"), self.group(f="PY"), self.group(f="SUM(1)"),
                    self.group(v=["7ff8000000000000", ONE]), self.group(v=[ONE]), self.group(k="x"),
                    self.group(k="n,n", v=[ONE] * 4, f="LEN", x=[1, 1]), self.group(n=0, v=[]),
                    self.group(f="LEN", k="s", n=1, v=["a\u0000b"]), self.group(k="a40x40", f="LINEST", n=1, v=[ONE] * 1600)):
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                self.admission.check(bad)

    def test_new_value_only_families_and_nullary_calls(self):
        for name in ("TRUE", "FALSE", "NA", "PI", "ROW", "COLUMN", "RAND"):
            with self.subTest(name=name):
                self.admission.check(self.group(f=name, k="", n=1, v=[]))
                self.admission.check(self.group(f=name, k="", n=1, v=[], spill=True))
        for name in ("IF", "IFERROR", "IFNA", "CHOOSE"):
            with self.subTest(name=name):
                self.admission.check(self.group(f=name, k="n,n", n=1, v=[ONE, TWO]))
        for name, arity in (("TEXTAFTER", 2), ("TEXTBEFORE", 2), ("REGEXTEST", 2), ("REGEXEXTRACT", 2),
                            ("REGEXREPLACE", 3), ("ENCODEURL", 1), ("JIS", 1), ("PHONETIC", 1)):
            with self.subTest(name=name):
                self.admission.check(self.group(f=name, k=",".join(["s"] * arity), n=1, v=["a"] * arity))
        for name, arity in (("VLOOKUP", 3), ("HLOOKUP", 3), ("LOOKUP", 2), ("MATCH", 2), ("XLOOKUP", 3),
                            ("XMATCH", 2), ("INDEX", 2), ("ROWS", 1), ("COLUMNS", 1), ("ARRAYTOTEXT", 1)):
            with self.subTest(name=name):
                self.admission.check(self.group(f=name, k=",".join(["a1x2"] * arity), n=1, v=[ONE, TWO] * arity))
        for name, arity in (("FILTER", 2), ("CELL", 1), ("IMPORTCSV", 1), ("RTD", 3), ("BYROW", 2)):
            with self.subTest(name=name):
                group = self.group(f=name, k=",".join(["s"] * arity), n=1, v=["a"] * arity)
                self.admission.check(group)
                self.admission.check({**group, "spill": True})

    def test_regression_needs_a_bounded_result_index_and_arrays(self):
        g = {"f": "LINEST", "k": "a3x1,a3x1,b,b", "n": 1, "x": [4, 2],
             "v": [ONE, TWO, ONE, ONE, TWO, ONE, False, True]}
        self.admission.check(g)
        for bad in ({**g, "x": None}, {**g, "x": [9, 9]}, {**g, "f": "SUM"}):
            if bad["x"] is None:
                del bad["x"]
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                self.admission.check(bad)

    def test_refuses_entry_invalid_arities_and_overlong_formulas_before_excel(self):
        # GETPIVOTDATA: Excel accepts 2, 3, or even 4..=254; refuses 0, 1, and odd >= 5 at formula entry
        # (and Range.Formula2R1C1 = object[,] surfaces entry refusal as OutOfMemoryException).
        for good_n in (2, 3, 4, 6, 12):
            k = ",".join(["n"] * good_n)
            with self.subTest(fn="GETPIVOTDATA", good_n=good_n):
                self.admission.check(self.group(f="GETPIVOTDATA", k=k, n=1, v=[ONE] * good_n))
        for bad_n in (0, 1, 5, 7, 9, 11):
            k = ",".join(["n"] * bad_n)
            with self.subTest(fn="GETPIVOTDATA", bad_n=bad_n), self.assertRaises(ValueError):
                self.admission.check(self.group(f="GETPIVOTDATA", k=k, n=1, v=[ONE] * bad_n))
        # Paired conditional functions: Excel refuses incomplete pairs at formula entry.
        self.admission.check(self.group(f="COUNTIFS", k="n,n", n=1, v=[ONE] * 2))
        self.admission.check(self.group(f="IFS", k="b,n", n=1, v=[True, ONE]))
        self.admission.check(self.group(f="SUMIFS", k="n,n,n", n=1, v=[ONE] * 3))
        for fn, bad_n in (("COUNTIFS", 1), ("COUNTIFS", 3), ("IFS", 3), ("SUMIFS", 2), ("SUMIFS", 4), ("MAXIFS", 4)):
            k = ",".join(["n"] * bad_n)
            with self.subTest(fn=fn, bad_n=bad_n), self.assertRaises(ValueError):
                self.admission.check(self.group(f=fn, k=k, n=1, v=[ONE] * bad_n))
        # Excel's 8192-character formula limit: a batch of many 128-cell CHOOSE arrays overflows it.
        huge_k = ",".join(["a16x8"] * 8)
        with self.assertRaises(ValueError):
            self.admission.check(self.group(f="SUM", k=huge_k, n=1, v=[ONE] * (128 * 8)))

    def test_entry_arity_applies_to_grouped_and_fixed_calls_including_file_spellings(self):
        for function, good, bad in (
            ("SORTBY", (2, 3, 5), (1, 4, 6)),
            ("LET", (3, 5), (2, 4, 6)),
            ("ABS", (1,), (0, 2)),
            ("_XLFN.IFS", (2, 4), (1, 3)),
            ("_XLFN.MAXIFS", (3, 5), (2, 4)),
            ("_XLFN.MINIFS", (3, 5), (2, 4)),
        ):
            def call(arity):
                return self.group(f=function, k=",".join(["n"] * arity), n=1, v=[ONE] * arity)

            for arity in good:
                with self.subTest(function=function, arity=arity):
                    self.admission.check(call(arity))
            for arity in bad:
                with self.subTest(function=function, arity=arity), self.assertRaises(ValueError):
                    self.admission.check(call(arity))

    def test_sheet_case_formula_limit_covers_inputs_and_probes(self):
        formula = "=" + "1+" * 4095 + "1"
        sheets = [{"name": "S", "cells": {"A1": {"type": "formula", "formula": formula}}}]
        probes = [{"at": "S!B1", "formula": formula}]
        case = protocol.sheet_case(sheets, probes)
        [group] = protocol.rows_to_groups(protocol.CASE_FUNCTION, [[case]])
        self.admission.check(group)
        for place in (sheets[0]["cells"]["A1"], probes[0]):
            with self.assertRaisesRegex(ValueError, "8192"):
                place["formula"] = formula + "+1"
                self.admission.check(group)
            place["formula"] = formula

    def test_formula_limit_counts_utf16_code_units(self):
        group = self.group(f="LEN", k="S", n=1, v=["\ud800😀"])
        self.admission.check(group)
        from policy import formula_chars
        self.assertEqual(formula_chars(group["v"][0]), 3)

    def test_broker_refuses_odd_getpivotdata_before_reaching_worker(self):
        from touchstone import client as touchstone_client
        broker, state, stop = start_broker(workers=1, extra={"admission": "policy:admission"})
        try:
            conn = touchstone_client.Connection.open(
                endpoint=state / "endpoint.json", socket_path=state / "none.sock", name="t"
            )
            try:
                with self.assertRaises(touchstone_client.ServiceError) as caught:
                    conn.submit_rows("GETPIVOTDATA", [[{"type": "number", "bits": ONE}] * 5]).result(timeout=10)
                self.assertIn("refused", str(caught.exception))
                self.assertEqual(broker.counters["batches"], 0)
                self.assertEqual(broker.counters["refused"], 1)
            finally:
                conn.close()
        finally:
            stop()


def start_broker(workers=2, extra=None):
    """A broker over fake workers, in a thread.  Returns (broker, state_dir, stop_function)."""
    import broker as broker_module
    temporary = tempfile.TemporaryDirectory()
    state = Path(temporary.name)
    options = SimpleNamespace(state=state, workers=workers, local_queue=state / "queue", hosts=[],
                              worker_command=[sys.executable, str(HERE / "fake_worker.py")],
                              **{"admission": None, "sweep_orphans": False, **(extra or {})})
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
        if thread.is_alive() or failure:
            raise RuntimeError(f"fake broker shutdown failed: {failure}")

    return instance, state, stop


class StopAndStallTests(unittest.TestCase):
    """dc-ztvv.26.131: stop must end the run process; a stalled isolated queue fails and heals."""

    def command(self, state, command, *extra):
        import broker as broker_module
        with patch.object(sys, "argv", ["broker.py", command, "--state", str(state), *extra]):
            broker_module.main()

    def test_stop_ends_a_run_process_that_outlives_its_lock(self):
        import subprocess as sp
        with tempfile.TemporaryDirectory() as folder:
            state = Path(folder)
            child = sp.Popen([sys.executable, "-c", "import time; time.sleep(120)"])
            self.addCleanup(lambda: child.poll() is None and child.kill())
            (state / "run.pid").write_text(str(child.pid))
            out = io.StringIO()
            with patch("sys.stdout", out):
                self.command(state, "stop", "--stop-grace", "0.5")
            self.assertIsNotNone(child.wait(10))
            self.assertIn("ended", out.getvalue())
            self.assertFalse((state / "run.pid").exists())

    def test_stop_leaves_a_run_process_that_exits_by_itself_alone(self):
        with tempfile.TemporaryDirectory() as folder:
            state = Path(folder)
            (state / "run.pid").write_text("999999999")
            out = io.StringIO()
            with patch("sys.stdout", out):
                self.command(state, "stop", "--stop-grace", "0.5")
            self.assertIn("stopped", out.getvalue())

    def test_a_running_broker_records_its_pid_and_removes_it_on_a_clean_stop(self):
        _, state, stop = start_broker(workers=1)
        self.assertEqual((state / "run.pid").read_text().strip(), str(os.getpid()))
        stop_dir = state
        (stop_dir / "stop").touch()
        deadline = time.monotonic() + 20
        while (state / "run.pid").exists() and time.monotonic() < deadline:
            time.sleep(0.1)
        self.assertFalse((state / "run.pid").exists())
        stop()

    def test_a_stalled_isolated_queue_fails_its_calls_restarts_the_worker_and_status_says_so(self):
        broker, state, stop = start_broker(3, {"isolation_stall_seconds": 0, "isolation_fail_seconds": 0.5})
        self.addCleanup(stop)
        from touchstone import client
        conn = client.Connection.open(endpoint=state / "endpoint.json", socket_path=state / "none.sock", name="stall")
        self.addCleanup(conn.close)
        isolation = broker.isolation_slot()
        before = isolation.restarts

        async def hold():
            broker.batcher.isolate.add("ID")
            isolation.state = "busy"          # the worker holds a call that never returns
            isolation.running = "interactive"
        asyncio.run_coroutine_threadsafe(hold(), broker.loop).result(3)
        pending = conn.submit_groups([{"f": "ID", "k": "n", "n": 2, "v": [ONE, TWO]}])
        with self.assertRaises(Exception) as caught:
            pending.raw(15)
        self.assertIn("isolation-stalled", str(caught.exception))
        deadline = time.monotonic() + 15
        while isolation.state != "idle" and time.monotonic() < deadline:
            time.sleep(0.1)
        self.assertEqual(isolation.state, "idle")
        self.assertGreater(isolation.restarts, before)
        status = json.loads((state / "status.json").read_text())
        self.assertEqual(status["broker_flags"], [], status["broker_flags"])
        self.assertTrue(status["health"].startswith("OK "), status["health"])
        self.assertTrue(any("isolation worker restarted" in note for note in status["broker_notes"]), status["broker_notes"])
        again = conn.submit_groups([{"f": "ID", "k": "n", "n": 1, "v": [ONE]}])
        self.assertEqual(again.raw(15)[1], [ONE])

    def test_stop_ends_real_broker_run_process_whose_non_daemon_thread_outlives_lock(self):
        import subprocess as sp
        with tempfile.TemporaryDirectory() as folder:
            state = Path(folder)
            runner = (
                "import asyncio, pathlib, sys, threading, time\n"
                "from types import SimpleNamespace\n"
                f"sys.path.insert(0, {str(HERE)!r})\n"
                "import broker\n"
                "state = pathlib.Path(sys.argv[1])\n"
                "opts = SimpleNamespace(state=state, workers=1, local_queue=state / 'queue', hosts=[], "
                f"worker_command=[sys.executable, {str(HERE / 'fake_worker.py')!r}], "
                "admission=None, sweep_orphans=False)\n"
                "instance = broker.Broker(opts)\n"
                "instance.cpu_name = lambda: 'test cpu'\n"
                "threading.Thread(target=time.sleep, args=(120,), daemon=False).start()\n"
                "asyncio.run(instance.run())\n"
            )
            child = sp.Popen([sys.executable, "-c", runner, str(state)],
                             stdout=sp.PIPE, stderr=sp.PIPE)
            def cleanup_child():
                if child.poll() is None:
                    child.kill()
                    child.wait(5)
                child.stdout.close()
                child.stderr.close()
            self.addCleanup(cleanup_child)
            deadline = time.monotonic() + 20
            while not ((state / "endpoint.json").exists() and (state / "run.pid").exists()):
                if child.poll() is not None or time.monotonic() > deadline:
                    self.fail(f"real broker subprocess failed to start: rc={child.poll()} err={child.stderr.read().decode()[-400:]}")
                time.sleep(0.05)
            out = io.StringIO()
            with patch("sys.stdout", out):
                self.command(state, "stop", "--stop-grace", "0.5")
            self.assertIsNotNone(child.wait(10), "stop left the live Broker.run process running")
            self.assertIn("ended", out.getvalue())
            self.assertFalse((state / "run.pid").exists())

    def test_heal_isolation_stall_does_not_race_live_run_batch_or_pin_degraded(self):
        import broker as broker_module
        import contextlib
        broker, state, stop = start_broker(
            3, {"isolation_stall_seconds": 0, "isolation_fail_seconds": 0.5, "restart_cap": 1, "cooldown": 60.0}
        )
        self.addCleanup(stop)
        from touchstone import client
        conn = client.Connection.open(endpoint=state / "endpoint.json", socket_path=state / "none.sock", name="stall-live")
        self.addCleanup(conn.close)
        isolation = broker.isolation_slot()
        before = isolation.restarts

        async def isolate():
            broker.batcher.isolate.update({"HANG", "ID"})
        asyncio.run_coroutine_threadsafe(isolate(), broker.loop).result(3)

        hung = conn.submit_groups([{"f": "HANG", "k": "n", "n": 1, "v": [ONE]}])
        deadline = time.monotonic() + 10
        while isolation.state != "busy" and time.monotonic() < deadline:
            time.sleep(0.05)
        self.assertEqual(isolation.state, "busy")

        queued = conn.submit_groups([{"f": "ID", "k": "n", "n": 2, "v": [ONE, TWO]}])
        with self.assertRaises(Exception) as caught:
            queued.raw(15)
        self.assertIn("isolation-stalled", str(caught.exception))
        with self.assertRaises(Exception):
            hung.raw(15)

        deadline = time.monotonic() + 15
        while isolation.state != "idle" and time.monotonic() < deadline:
            time.sleep(0.1)
        time.sleep(0.7)  # allow any racing run_batch cleanup and a status.json tick to settle
        self.assertEqual(isolation.state, "idle")
        self.assertEqual(isolation.restarts, before + 1, "heal_isolation_stall must not race run_batch into double restart")
        self.assertEqual(broker.window_attempts(isolation), 0, "stall healing must not burn restart cap")

        status = json.loads((state / "status.json").read_text())
        self.assertEqual(status["broker_flags"], [], status["broker_flags"])
        self.assertTrue(status["health"].startswith("OK "), status["health"])
        self.assertIn("isolation worker restarted", status["health"])
        self.assertTrue(any("isolation worker restarted" in n for n in status.get("broker_notes", [])), status)

        out = io.StringIO()
        with contextlib.redirect_stdout(out), self.assertRaises(SystemExit) as raised:
            broker_module.command_status(SimpleNamespace(state=state, json=False))
        self.assertEqual(raised.exception.code, 0, out.getvalue())

        again = conn.submit_groups([{"f": "ID", "k": "n", "n": 1, "v": [ONE]}])
        self.assertEqual(again.raw(15)[1], [ONE])

    def test_heal_isolation_stall_does_not_race_recycle_after_worker_event(self):
        broker, state, stop = start_broker(
            3, {"isolation_stall_seconds": 0, "isolation_fail_seconds": 0.3, "restart_cap": 1, "cooldown": 60.0}
        )
        self.addCleanup(stop)
        from touchstone import client
        conn = client.Connection.open(endpoint=state / "endpoint.json", socket_path=state / "none.sock", name="stall-recycle")
        self.addCleanup(conn.close)
        isolation = broker.isolation_slot()
        old_proc = isolation.proc
        in_await = threading.Event()
        release_await = threading.Event()

        def slow_await_restart():
            in_await.set()
            release_await.wait(15)
            raise RuntimeError("old worker killed while awaiting restart")

        old_proc.await_restart = slow_await_restart

        async def isolate():
            broker.batcher.isolate.update({"CRASHON", "ID"})
        asyncio.run_coroutine_threadsafe(isolate(), broker.loop).result(3)

        crashed = conn.submit_groups([{"f": "CRASHON", "k": "n", "n": 1, "v": ["402a000000000000"]}])
        self.assertTrue(in_await.wait(10), "isolation slot never entered recycle_after_worker_event")
        letters, answers = crashed.raw(10)
        self.assertEqual(letters, ["x"])
        self.assertEqual(answers[0]["reason"], "excel-crashed")

        queued = conn.submit_groups([{"f": "ID", "k": "n", "n": 1, "v": [ONE]}])
        with self.assertRaises(Exception) as caught:
            queued.raw(15)
        self.assertIn("isolation-stalled", str(caught.exception))

        deadline = time.monotonic() + 15
        while (isolation.proc is old_proc or isolation.state != "idle") and time.monotonic() < deadline:
            time.sleep(0.05)
        self.assertIsNot(isolation.proc, old_proc)
        self.assertEqual(isolation.state, "idle")
        new_proc = isolation.proc
        new_hello = isolation.hello

        # Now let the old recycle_after_worker_event wake up and fail; it must not kill or down the new worker.
        release_await.set()
        time.sleep(0.5)
        self.assertIs(isolation.proc, new_proc)
        self.assertEqual(isolation.hello, new_hello)
        self.assertEqual(isolation.state, "idle", f"stale recycle_after_worker_event marked slot {isolation.state}: {isolation.down_reason}")
        self.assertIsNone(new_proc.process.poll(), "stale recycle_after_worker_event killed the new healthy worker")
        self.assertEqual(conn.submit_groups([{"f": "ID", "k": "n", "n": 1, "v": [ONE]}]).raw(15)[1], [ONE])


class DiskFullTests(unittest.TestCase):
    def test_status_and_batch_writes_recover_while_the_broker_serves_answers(self):
        from contextlib import closing
        import broker as broker_module
        from touchstone import client

        full = threading.Event()
        failed = {name: threading.Event() for name in ("status", "batch")}
        open_path = Path.open

        def open_with_full_disk(path, mode="r", *args, **kwargs):
            if full.is_set() and path.parent.name == "log" and path.suffix == ".gz" and "a" in mode:
                failed["batch"].set()
                raise OSError(errno.ENOSPC, "simulated full disk")
            if full.is_set() and path.name == "status.json.tmp" and "w" in mode:
                failed["status"].set()
                raise OSError(errno.ENOSPC, "simulated full disk")
            return open_path(path, mode, *args, **kwargs)

        instance, state, stop = start_broker(workers=1, extra={"health_seconds": 0.2})
        self.addCleanup(stop)
        with closing(client.Connection.open(endpoint=state / "endpoint.json", socket_path=state / "none.sock")) as conn:
            with patch.object(Path, "open", open_with_full_disk):
                full.set()
                try:
                    groups = [{"f": "ID", "k": "n", "n": 100, "v": [TWO] * 100}]
                    self.assertEqual(conn.submit_groups(groups).raw(5), (["n"] * 100, [TWO] * 100))
                    for failure in failed.values():
                        self.assertTrue(failure.wait(5))
                    self.assertTrue(instance.health_line().startswith("DEGRADED"), instance.health_line())
                    self.assertEqual(conn.submit_groups(groups).raw(5), (["n"] * 100, [TWO] * 100))
                finally:
                    full.clear()
            deadline = time.monotonic() + 5
            while time.monotonic() < deadline:
                try:
                    status = json.loads((state / "status.json").read_text())
                    if status["counters"]["calls"] == 200 and status["health"].startswith("OK"):
                        break
                except FileNotFoundError:
                    pass
                time.sleep(0.05)
            else:
                self.fail("status did not recover with both batches counted")
            self.assertEqual(sum(row["calls"] for row in broker_module.logs.BatchLog.read(state / "log")
                                 if row["kind"] == "batch"), 200)


class RelayOutageTests(unittest.TestCase):
    @unittest.skipUnless(os.name == "posix", "uses a Linux lane socket")
    def test_missing_relay_socket_reports_broker_down(self):
        from touchstone import client
        with tempfile.TemporaryDirectory() as folder, patch.object(client.time, "sleep"):
            with patch.dict(os.environ, {"TOUCHSTONE_SOCKET": str(Path(folder) / "excel.sock")}), \
                 self.assertRaisesRegex(ConnectionError, "broker down"):
                client.Connection.open()


class WatchdogTests(unittest.TestCase):
    def command(self, state, command="ensure"):
        import broker as broker_module
        with patch.object(sys, "argv", ["broker.py", command, "--state", str(state)]):
            broker_module.main()

    def test_a_live_broker_keeps_its_endpoint_even_when_status_writes_fail(self):
        import broker as broker_module
        instance, state, stop = start_broker(workers=1)
        self.addCleanup(stop)
        endpoint = state / "endpoint.json"
        before = endpoint.stat().st_mtime_ns
        with patch.object(instance, "write_status"), patch.object(broker_module, "launch_broker", create=True) as launch:
            (state / "status.json").unlink(missing_ok=True)
            self.command(state)
        self.assertEqual(endpoint.stat().st_mtime_ns, before)
        launch.assert_not_called()

    def test_a_stale_status_does_not_block_restart_and_a_second_start_does_not_launch(self):
        import broker as broker_module
        with tempfile.TemporaryDirectory() as folder:
            state = Path(folder)
            (state / "status.json").write_text('{"running": true, "pid": 1}')
            with patch.object(broker_module, "launch_broker", create=True) as launch:
                launch.side_effect = lambda options: self.command(state, "start")
                self.command(state)
                launch.assert_called_once()

    def test_a_manual_stop_holds_until_an_explicit_start(self):
        import broker as broker_module
        _, state, cleanup = start_broker(workers=1)
        self.addCleanup(cleanup)
        self.command(state, "stop")
        self.assertTrue((state / "stop").exists())
        self.assertFalse(json.loads((state / "status.json").read_text())["running"])
        with patch.object(broker_module, "launch_broker", create=True) as launch:
            self.command(state)
            launch.assert_not_called()
            self.command(state, "start")
            launch.assert_called_once()


class BrokerTests(unittest.TestCase):
    def connect(self, state, name="t"):
        from touchstone import client
        return client.Connection.open(endpoint=state / "endpoint.json", socket_path=state / "none.sock", name=name)

    def test_vba_round_trip_is_one_case_per_batch_on_the_designated_slot(self):
        from touchstone import client

        broker, state, stop = start_broker(workers=3, extra={"admission": "policy:admission"})
        self.addCleanup(stop)
        isolated = next(slot for slot in broker.slots if slot.isolation)
        with closing(self.connect(state)) as conn, patch.object(isolated.proc, "run", wraps=isolated.proc.run) as run:
            first = conn.submit_groups([template_vba_group()])
            second = conn.submit_groups([template_vba_group(calls=[{"procedure": "Module1.Probe", "args": [number(TWO)]}])])
            for pending, bits, calls in ((first, ONE, [{"return": number(ONE)}]),
                                         (second, TWO, [{"err": {"number": 5, "description": "Fake VBA error"}}])):
                letters, values = pending.raw(10)
                self.assertEqual(letters, ["v"])
                answer = protocol.answer_to_typed(letters[0], values[0])
                self.assertEqual(answer["calls"], calls)
                self.assertEqual(answer["sheet_state"], [{"at": "Data!A1", "value": number(bits)}])
                self.assertEqual(answer["event_log"], [{"at": "Data!B1", "value": {"type": "text", "value": "Change"}}])
            self.assertEqual(run.call_count, 2)
            for call in run.call_args_list:
                message = call.args[0]
                self.assertEqual([(g["f"], g["n"]) for g in message["groups"]], [("_VBA", 1)])
                self.assertGreater(message["deadline_ms"], 0)
            self.assertFalse(broker.recent)
            self.assertFalse(broker.pending_checks)
            for options in ({"order": "serial"}, {"priority": "sweep"}, {"echo": True}):
                with self.subTest(options=options), self.assertRaisesRegex(client.ServiceError, "refused:.*VBA"):
                    conn.submit_groups([template_vba_group()], **options).raw(10)
            with self.assertRaisesRegex(client.ServiceError, "refused:.*VBA"):
                conn.submit_groups([template_vba_group(), {"f": "ABS", "k": "n", "n": 1, "v": [ONE], "o": 1}]).raw(10)
            self.assertEqual(run.call_count, 2)

    def test_vba_refuses_a_missing_isolation_worker_or_admission_policy(self):
        from touchstone import client

        for workers, admission, failed, detail in ((1, "policy:admission", "", "isolation"),
                                                    (3, None, "", "admission"),
                                                    (3, "policy:admission", "w2", "isolation")):
            with self.subTest(workers=workers, admission=admission, failed=failed):
                with patch.dict(os.environ, {"TOUCHSTONE_TEST_FAIL_START": failed}):
                    _, state, stop = start_broker(workers=workers, extra={"admission": admission})
                try:
                    with closing(self.connect(state)) as conn:
                        with self.assertRaisesRegex(client.ServiceError, f"refused:.*{detail}"):
                            conn.submit_groups([template_vba_group()]).raw(10)
                        self.assertEqual(conn.submit_groups([{"f": "ABS", "k": "n", "n": 1, "v": [ONE]}]).raw(10),
                                         (["n"], [ONE]))
                finally:
                    stop()

    def test_vba_watchdog_failure_uses_the_deadline_budget_on_the_isolated_slot(self):
        broker, state, stop = start_broker(workers=3, extra={"admission": "policy:admission"})
        self.addCleanup(stop)
        with closing(self.connect(state)) as conn:
            group = template_vba_group(calls=[{"procedure": "Module1.Probe", "args": [number("402a000000000000")]}])
            letters, answers = conn.submit_groups([group]).raw(10)
            self.assertEqual(letters, ["x"])
            self.assertEqual(answers[0]["reason"], "excel-deadline")
            isolated = next(slot for slot in broker.slots if slot.isolation)
            self.assertEqual(answers[0]["worker"], isolated.name)
            self.assertEqual(broker.window_attempts(isolated), 0)
            self.assertEqual(broker.crash_times, {})

    def test_weeknum_sweep_preserves_serial_positions_in_one_calculation_pass(self):
        broker, state, stop = start_broker(workers=3)
        try:
            conn = self.connect(state)
            self.addCleanup(conn.close)
            slot = next(slot for slot in broker.slots if slot.serial)
            worker_run = slot.proc.run
            worker_batches = []

            def record(message, timeout):
                worker_batches.append(message)
                return worker_run(message, timeout)

            # Interleave argument shapes across the ordinary sweep's 32-call batch boundary.
            rows = [[number(ONE), number(TWO)] if i % 2 == 0 else [number(ONE)] for i in range(40)]
            with patch.object(slot.proc, "run", side_effect=record):
                reply = conn.submit_rows("WEEKNUM", rows, priority="sweep", echo=True).result(10)
            self.assertEqual(reply["order"], "serial")
            self.assertEqual(reply["rows"], [number(ONE)] * len(rows))
            self.assertEqual(len(worker_batches), 1)
            message = worker_batches[0]
            self.assertTrue(message["serial"])
            self.assertEqual([list(protocol.positions(group)) for group in message["groups"]],
                             [list(range(0, len(rows), 2)), list(range(1, len(rows), 2))])
        finally:
            stop()

    def test_sweep_uses_one_worker_until_idle_expansion(self):
        broker, state, stop = start_broker(workers=4)
        try:
            conn = self.connect(state)
            # Cost-size these into separate dispatches even though they arrived in one request.
            rows = [[number(ONE)]] * 24
            def busy_workers(priority):
                reply = conn.submit_rows("ID", rows, priority=priority).result(15)
                self.assertEqual(len(reply["rows"]), len(rows))
                return [slot.name for slot in broker.slots if slot.calls]
            broker.batcher.costs["ID"] = 300_000
            self.assertEqual(len(busy_workers("sweep")), 1)
            for slot in broker.slots:
                slot.calls = 0
            broker.options.sweep_workers = 3
            broker.batcher.costs["ID"] = 300_000
            self.assertGreater(len(busy_workers("sweep")), 1)
        finally:
            stop()

    def test_sweep_gives_the_worker_back_to_foreground_between_batches(self):
        broker, state, stop = start_broker(workers=1)
        try:
            conn = self.connect(state)
            sweep = conn.submit_rows("SLEEP", [[number(ONE)]] * 96, priority="sweep")
            until = time.monotonic() + 3
            while not any(slot.state == "busy" and slot.running == "sweep" for slot in broker.slots):
                self.assertLess(time.monotonic(), until)
                time.sleep(0.01)
            foreground = conn.submit_rows("ID", [[number(TWO)]], priority="bulk")
            self.assertEqual(foreground.result(5)["rows"], [number(TWO)])
            self.assertFalse(sweep.done())
            self.assertEqual(len(sweep.result(10)["rows"]), 96)
        finally:
            stop()

    def test_sweep_does_not_dispatch_while_foreground_bulk_is_busy(self):
        broker, state, stop = start_broker(workers=4, extra={"reserve": 1, "sweep_workers": 2})
        try:
            conn = self.connect(state)
            # Occupy 1 worker with foreground bulk (3 idle > reserve=1).
            fg = conn.submit_rows("SLEEP", [[number(ONE)]], priority="bulk")
            until = time.monotonic() + 3
            while not any(slot.state == "busy" and slot.running == "bulk" for slot in broker.slots):
                self.assertLess(time.monotonic(), until)
                time.sleep(0.005)
            sweep = conn.submit_rows("ID", [[number(TWO)]] * 4, priority="sweep")
            # While foreground bulk is running, zero sweep workers may dispatch.
            saw_sweep_during_fg = False
            while any(slot.state == "busy" and slot.running == "bulk" for slot in broker.slots):
                if any(slot.state == "busy" and slot.running == "sweep" for slot in broker.slots):
                    saw_sweep_during_fg = True
                    break
                if sweep.done():
                    saw_sweep_during_fg = True
                    break
                time.sleep(0.005)
            self.assertEqual(len(fg.result(5)["rows"]), 1)
            self.assertEqual(len(sweep.result(5)["rows"]), 4)
            self.assertFalse(
                saw_sweep_during_fg,
                "sweep must not dispatch onto an idle worker while foreground bulk is active",
            )
        finally:
            stop()

    def test_expire_sweeps_scopes_discard_by_origin_and_evicts_expired_request(self):
        import broker as broker_module
        import batcher as batcher_module

        class DummyConn:
            def __init__(self, origin):
                self.origin = origin
                self.sent = []
            def send(self, kind, body, urgent=False):
                self.sent.append((kind, body, urgent))

        temporary = tempfile.TemporaryDirectory()
        try:
            state = Path(temporary.name)
            options = SimpleNamespace(
                state=state, workers=1, local_queue=state / "queue", hosts=[], admission=None,
                worker_command=[sys.executable, str(HERE / "fake_worker.py")], sweep_orphans=False,
            )
            instance = broker_module.Broker(options)
            instance.prepare()
            conn_a = DummyConn("origin-a")
            conn_b = DummyConn("origin-b")
            rid = "shared-rid-001"
            grp = [{"f": "ID", "k": "n", "n": 1, "v": [ONE], "o": 0}]

            piece_a = batcher_module.Piece(conn_a, rid, 0, grp, True, "sweep")
            piece_a.t_received = time.monotonic() - (broker_module.SWEEP_QUEUE_SECONDS + 5.0)
            req_a = broker_module.Request()
            req_a.priority = "sweep"
            req_a.last_seq = 0
            req_a.pieces[0] = piece_a
            instance.requests[("origin-a", rid)] = req_a
            instance.batcher.add_piece(piece_a)

            piece_b = batcher_module.Piece(conn_b, rid, 0, grp, True, "bulk")
            req_b = broker_module.Request()
            req_b.priority = "bulk"
            req_b.last_seq = 0
            req_b.pieces[0] = piece_b
            instance.requests[("origin-b", rid)] = req_b
            instance.batcher.add_piece(piece_b)

            instance.expire_sweeps()

            # origin-a's expired sweep must be failed and immediately evicted from broker.requests.
            self.assertNotIn(("origin-a", rid), instance.requests)
            self.assertTrue(any(body.get("code") == "sweep-deferred" for _, body, _ in conn_a.sent))
            # origin-b's request sharing the same bare rid must remain intact in both batcher and requests.
            self.assertIn(("origin-b", rid), instance.requests)
            waiting_keys = {(p.origin, p.rid) for p, _, _ in instance.batcher.waiting()}
            self.assertIn(("origin-b", rid), waiting_keys)
            self.assertNotIn(("origin-a", rid), waiting_keys)
        finally:
            temporary.cleanup()

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

    def test_an_add_in_request_runs_only_on_the_add_in_worker_and_others_never_do(self):
        broker, state, stop = start_broker(workers=1, extra={"addin_workers": ["EUROTOOL.XLAM"]})
        try:
            from touchstone import client
            self.assertEqual([(s.name, s.addins) for s in broker.slots], [("w1", ()), ("x1", ("EUROTOOL.XLAM",))])
            conn = self.connect(state)
            pending = conn.submit_rows("ID", [[number(ONE)]] * 3, addins=["eurotool.xlam"])
            self.assertEqual([r["bits"] for r in pending.result(30)["rows"]], [ONE] * 3)
            self.assertEqual(broker.slots[1].calls, 3)
            plain = conn.submit_rows("ID", [[number(TWO)]] * 4)
            self.assertEqual([r["bits"] for r in plain.result(30)["rows"]], [TWO] * 4)
            self.assertEqual((broker.slots[0].calls, broker.slots[1].calls), (4, 3))
            with self.assertRaises(client.ServiceError):
                conn.submit_rows("ID", [[number(ONE)]], addins=["OTHER.XLAM"]).raw(10)
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

    def test_a_call_that_crashed_excel_alone_is_answered_as_excel_crashed_and_the_worker_comes_back_once(self):
        broker, state, stop = start_broker(workers=1)
        try:
            from touchstone import client
            conn = self.connect(state)
            letters, values = conn.submit_groups([{"f": "CRASH", "k": "n", "n": 1, "v": [ONE]}]).raw(30)
            self.assertEqual(letters, ["x"])
            self.assertTrue(client.is_crashed(letters[0]))
            self.assertEqual((values[0]["reason"], values[0]["function"], values[0]["args"]), ("excel-crashed", "CRASH", [ONE]))
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
            row = c.ask("CRASH", [[self.top.number(1.0)]])["rows"][0]        # a crash is an answer, with its arguments
            self.assertEqual((row["type"], row["reason"], row["function"]), ("crashed", "excel-crashed", "CRASH"))
            self.assertEqual(row["arguments"], [self.top.number(1.0)])
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


@unittest.skipUnless(sys.platform == "linux" or _wsl_python(), "requires Linux or WSL")
class RelayTests(unittest.TestCase):
    """The Linux relay, with this test playing the broker on its stdin and stdout."""

    def setUp(self):
        import subprocess
        import peers
        self.subprocess = subprocess
        self.linux = [] if sys.platform == "linux" else ["wsl", "-e"]
        self.dir = "/tmp/touchstone-relay-test-%d-%d" % (os.getpid(), time.time_ns())
        command, first = peers.bundle({"socket": self.dir + "/excel.sock", "v1_queue": self.dir + "/queue"})
        self.relay = subprocess.Popen([*self.linux, "bash", "-c", command], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
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
            self.relay.wait(10)
        self.relay.stdout.close()
        self.relay.stderr.close()
        self.subprocess.run([*self.linux, "rm", "-rf", self.dir], capture_output=True)

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

    def test_relay_publishes_broker_flags_locally_and_clears_them(self):
        flag = "isolated queue stalled"
        for flags in ([flag], []):
            self.send(protocol.PING, {"t": time.time(), "broker_flags": flags})
            self.assertEqual(self.read_frame()[0], protocol.PONG)
            result = self.subprocess.run([*self.linux, "cat", self.dir + "/excel.sock.health.json"],
                                         capture_output=True, check=True, timeout=5)
            self.assertEqual(json.loads(result.stdout)["broker_flags"], flags)

    def test_a_lane_client_on_the_socket_is_answered_through_the_broker_stream(self):
        import peers
        runner = ("import json" + chr(10) +
                  "import os; os.environ['DNACALC_LANE'] = 'relay-test'" + chr(10) +
                  "conn = sys.modules['client'].Connection.open(socket_path=%r, name='lane')" + chr(10) +
                  "p = conn.submit_groups([{'f': 'ABS', 'k': 'n', 'n': 3, 'v': ['%s', '%s', '%s']}])" + chr(10) +
                  "letters, values = p.raw(30)" + chr(10) +
                  "print(json.dumps({'values': values, 'server': conn.machine}))" + chr(10)) % (
                      self.dir + "/excel.sock", TWO, TWO, TWO)
        command, first = peers.bundle_program(("protocol", "v1", "client"), runner)
        lane = self.subprocess.Popen([*self.linux, "bash", "-c", command], stdin=self.subprocess.PIPE,
                                     stdout=self.subprocess.PIPE, stderr=self.subprocess.PIPE)
        lane.stdin.write(first)
        lane.stdin.close()
        kind, body = self.read_frame()
        self.send(protocol.ANSWER, {"id": body["id"], "seq": 0, "c": body["c"], "env": "e1",
                                    "g": [{"ty": "nnn", "o": 0, "v": [ONE] * 3}]})
        self.send(protocol.DONE, {"id": body["id"], "c": body["c"], "calls": 3})
        out, err = lane.communicate(timeout=60)
        self.assertEqual(lane.returncode, 0, err.decode()[-800:])
        result = json.loads(out.decode().strip().splitlines()[-1])
        self.assertEqual(result, {"values": [ONE, ONE, ONE], "server": "broker-host"})
        self.assertEqual(kind, protocol.SUBMIT)
        self.assertEqual(body.get("origin"), {"client": "lane", "lane": "relay-test"})

    def test_a_v1_request_file_is_answered_in_the_v1_reply_shape(self):
        rid = "ab" * 16
        request = {"version": 1, "id": rid, "function": "ABS", "rows": [[number(TWO)], [number(ONE)]],
                   "created": time.time(), "expires": time.time() + 120}
        self.subprocess.run([*self.linux, "bash", "-c", "mkdir -p %s/queue/in && cat > %s/queue/in/%s.json" % (self.dir, self.dir, rid)],
                            input=json.dumps(request).encode(), check=True, timeout=60)
        self.answer_submits(1)
        deadline = time.monotonic() + 30
        reply = None
        while time.monotonic() < deadline:
            done = self.subprocess.run([*self.linux, "cat", "%s/queue/out/%s.json" % (self.dir, rid)], capture_output=True)
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
        gone = self.subprocess.run([*self.linux, "ls", self.dir + "/excel.sock"], capture_output=True)
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

    CRASH_MARK = "402a000000000000"        # 13.0: the fake worker's CRASHON crashes on a batch holding it

    def test_twice_failing_request_is_logged_and_quarantined_across_connections(self):
        import logs
        from touchstone.client import ServiceError

        with patch.dict(os.environ, {"DNACALC_LANE": "kt2-test"}):
            broker, state, stop = start_broker(3, {"health_seconds": 0.1})
            self.addCleanup(stop)
            conn = self.connect(state, name="incident-client")
            self.addCleanup(conn.close)
        broker.loop.call_soon_threadsafe(broker.batcher.isolate.update, {"EXCELERROR", "ID"})
        rid = "17" * 16
        with self.assertRaisesRegex(ServiceError, "excel-crashed"):
            conn.submit_rows("EXCELERROR", [[number(TWO)]], rid=rid).raw(5)
        reconnected = self.connect(state, name="incident-client")
        self.addCleanup(reconnected.close)
        with self.assertRaisesRegex(ServiceError, "excel-crashed"):
            reconnected.submit_rows("EXCELERROR", [[number(TWO)]], rid=rid).raw(5)
        other = self.connect(state, name="another-client")
        self.addCleanup(other.close)
        self.assertEqual(other.submit_rows("ID", [[number(ONE)]], rid=rid).raw(5), (["n"], [ONE]))
        broker.batch_log.flush()
        records = list(logs.BatchLog.read(state / "log"))
        batches = [r for r in records if "EXCELERROR" in r.get("fns", {})]
        self.assertEqual([r.get("error") for r in batches], ["excel-error", "excel-error"])
        self.assertEqual([r["worker"] for r in batches], ["w2", "w2"])
        self.assertEqual(sum(r.get("event") == "drift-before" for r in records), 2)
        self.assertEqual(broker.slots[1].restarts, 2)
        expected_origin = {"transport": "tcp", "host": broker.machine, "client": "incident-client", "lane": "kt2-test"}
        self.assertEqual([r["requests"] for r in batches], [[{"id": rid, "origin": expected_origin}]] * 2)
        crash_list = state / "crash-list.jsonl"
        self.assertTrue(self.wait_for(crash_list.exists, 3))
        crashes = [json.loads(line) for line in crash_list.read_text().splitlines()]
        self.assertEqual(len(crashes), 1)
        self.assertEqual((crashes[0]["id"], crashes[0]["origin"], crashes[0]["failures"]), (rid, expected_origin, 2))
        self.assertEqual(crashes[0]["pieces"][0]["groups"][0]["v"], [TWO])
        from broker import Broker
        restarted = Broker(broker.options)
        restarted.load_persisted()
        self.assertGreater(restarted.poisoned[("tcp:incident-client", rid)], time.time())

    def test_crash_quarantine_scopes_poisoned_by_origin(self):
        from touchstone.client import ServiceError
        from broker import Broker

        broker, state, stop = start_broker(3, {"health_seconds": 0.1})
        self.addCleanup(stop)
        conn_a = self.connect(state, name="lane-a")
        conn_b = self.connect(state, name="lane-b")
        self.addCleanup(conn_a.close)
        self.addCleanup(conn_b.close)
        rid = "22" * 16
        with self.assertRaisesRegex(ServiceError, "excel-crashed"):
            conn_a.submit_rows("EXCELERROR", [[number(TWO)]], rid=rid).raw(10)
        # Reconnecting with the same client name (same origin) remains quarantined.
        conn_a_reconnected = self.connect(state, name="lane-a")
        self.addCleanup(conn_a_reconnected.close)
        with self.assertRaisesRegex(ServiceError, "excel-crashed"):
            conn_a_reconnected.submit_rows("EXCELERROR", [[number(TWO)]], rid=rid).raw(10)
        # A different origin sharing the same rid must not be blocked by lane-a's crash quarantine.
        self.assertEqual(conn_b.submit_rows("ID", [[number(ONE)]], rid=rid).raw(10), (["n"], [ONE]))
        restarted = Broker(broker.options)
        restarted.load_persisted()
        self.assertGreater(restarted.poisoned[("tcp:lane-a", rid)], time.time())
        self.assertNotIn(("tcp:lane-b", rid), restarted.poisoned)

    def test_capped_isolation_worker_does_not_hold_its_queue(self):
        with patch.dict(os.environ, {"TOUCHSTONE_TEST_FAIL_START": "w2"}):
            broker, state, stop = start_broker(3, {"cooldown": 0.1, "restart_cap": 1})
        self.addCleanup(stop)
        self.assertTrue(self.wait_for(lambda: "restart cap" in (broker.slots[1].down_reason or ""), 3))
        broker.loop.call_soon_threadsafe(broker.batcher.isolate.add, "ID")
        conn = self.connect(state)
        self.addCleanup(conn.close)
        pending = conn.submit_rows("ID", [[number(ONE)], [number(TWO)]])
        self.assertEqual(pending.raw(3), (["n", "n"], [ONE, TWO]))
        self.assertEqual(broker.slots[0].calls, 2)
        self.assertEqual(broker.slots[1].state, "down")
        self.assertEqual(broker.slots[2].calls, 0)

    def test_status_identifies_oldest_isolated_work_and_why_it_waits(self):
        broker, state, stop = start_broker(3, {"isolation_stall_seconds": 0})
        self.addCleanup(stop)
        conn = self.connect(state, name="status-client")
        self.addCleanup(conn.close)
        rid = "19" * 16

        async def status_while_queued():
            broker.batcher.isolate.add("SLEEP")
            for question, calls in (("18" * 16, 1), (rid, 2)):
                broker.on_submit(broker.conns[0], {"id": question, "seq": 0, "last": True,
                                 "groups": [{"f": "SLEEP", "k": "n", "n": calls, "v": [ONE] * calls, "o": 0}],
                                 "opts": {"priority": "interactive"}})
            broker.write_status()
            return json.loads((state / "status.json").read_text())

        status = asyncio.run_coroutine_threadsafe(status_while_queued(), broker.loop).result(3)
        worker_queue = status["workers"][1]["queue"]
        self.assertEqual(worker_queue, status["queue_by_function"]["SLEEP"])
        self.assertEqual(worker_queue["calls"], 2)
        oldest = worker_queue["oldest"]
        self.assertEqual((oldest["id"], oldest["function"], oldest["origin"]["client"], oldest["reason"]),
                         (rid, "SLEEP", "status-client", "isolation worker busy"))
        self.assertIn("isolated queue stalled", status["broker_flags"])
        self.assertIn("isolated queue stalled", status["health"])

    def test_a_marked_call_comes_back_as_excel_crashed_the_others_are_answered_and_no_worker_hits_the_cap(self):
        broker, state, stop = start_broker(workers=3, extra={"cooldown": 0.3})
        try:
            from touchstone import client
            conn = self.connect(state)
            marked_at, n = 17, 40
            values = [ONE] * n
            values[marked_at] = self.CRASH_MARK
            pending = conn.submit_groups([{"f": "CRASHON", "k": "n", "n": n, "v": values}])
            other = conn.submit_groups([{"f": "ID", "k": "n", "n": 3, "v": [ONE, TWO, ONE]}])
            letters, answers = pending.raw(60)
            self.assertEqual([i for i, l in enumerate(letters) if l == "x"], [marked_at])
            self.assertEqual([l for i, l in enumerate(letters) if i != marked_at], ["n"] * (n - 1))
            crashed = answers[marked_at]
            self.assertEqual((crashed["reason"], crashed["function"], crashed["args"]), ("excel-crashed", "CRASHON", [self.CRASH_MARK]))
            self.assertEqual(other.raw(60)[1], [ONE, TWO, ONE])
            # never retried: the crash list holds the call once, and the shared workers saw at most the first crash
            listed = [json.loads(line) for line in (state / "crash-list.jsonl").read_text().splitlines()]
            self.assertEqual([(r["function"], r["args"]) for r in listed], [("CRASHON", [self.CRASH_MARK])])
            self.assertTrue(self.wait_for(lambda: all(s.state == "idle" for s in broker.slots), 20))
            cap = broker.opt("restart_cap", 3)
            self.assertEqual([s.name for s in broker.slots if s.isolation], ["w2"])
            self.assertTrue(all(broker.window_attempts(s) < cap for s in broker.slots))
            self.assertLessEqual(sum(broker.window_attempts(s) for s in broker.slots), 1)    # only the first, unattributed crash counts
            self.assertGreaterEqual(broker.slots[1].crashes, 1)
            self.assertTrue(broker.health_line().startswith("OK "), broker.health_line())
            # the function is isolated from the second crash on; its later calls run on the isolation worker only
            again = conn.submit_groups([{"f": "CRASHON", "k": "n", "n": 3, "v": [ONE, self.CRASH_MARK, TWO]}])
            letters, _ = again.raw(60)
            self.assertEqual(letters, ["n", "x", "n"])
            self.assertEqual(sum(s.crashes for s in broker.slots if not s.isolation), 1)
        finally:
            stop()

    def test_a_deadline_kill_is_a_timeout_answer_with_its_own_budget_never_a_crash(self):
        broker, state, stop = start_broker(workers=3, extra={"cooldown": 0.3, "crash_budget": 3})
        try:
            from touchstone import client
            conn = self.connect(state)
            # a single call that runs past its deadline is its own timeout row, with its arguments
            pending = conn.submit_groups([{"f": "DEADLINEON", "k": "n", "n": 1, "v": [self.CRASH_MARK]}])
            letters, answers = pending.raw(90)
            self.assertEqual(letters, ["x"])
            self.assertEqual((answers[0]["reason"], answers[0]["function"], tuple(answers[0]["args"])),
                             ("excel-deadline", "DEADLINEON", (self.CRASH_MARK,)))
            self.assertFalse((state / "crash-list.jsonl").exists() and (state / "crash-list.jsonl").read_text().strip())
            self.assertEqual(broker.crash_times, {})
            self.assertEqual(len(broker.deadline_times["DEADLINEON"]), 1)
            self.assertEqual(len((state / "deadline-list.jsonl").read_text().splitlines()), 1)
            self.assertTrue(self.wait_for(lambda: all(s.state == "idle" for s in broker.slots), 20))
            # a real crash of another function is still counted as a crash
            crash = conn.submit_groups([{"f": "CRASHON", "k": "n", "n": 1, "v": [self.CRASH_MARK]}]).raw(60)
            self.assertEqual(crash[1][0]["reason"], "excel-crashed")
            self.assertEqual(len(broker.crash_times["CRASHON"]), 1)
        finally:
            stop()

    def test_a_request_that_ran_past_its_deadline_twice_is_quarantined_and_costs_no_restart_cap(self):
        broker, state, stop = start_broker(workers=3, extra={"cooldown": 0.3, "restart_cap": 1})
        try:
            from touchstone import client
            conn = self.connect(state)
            n = 12
            pending = conn.submit_groups([{"f": "DEADLINEON", "k": "n", "n": n, "v": [self.CRASH_MARK] * n}])
            with self.assertRaises(client.ServiceError) as caught:
                pending.raw(90)
            self.assertIn("excel-deadline", str(caught.exception))
            # the request was killed twice at most, not once per call
            self.assertLessEqual(len(broker.deadline_times.get("DEADLINEON", [])) + len(
                [1 for line in (state / "deadline-list.jsonl").read_text().splitlines() if '"quarantined"' in line]), 4)
            rows = [json.loads(line) for line in (state / "deadline-list.jsonl").read_text().splitlines()]
            self.assertTrue(any(r.get("quarantined") and r["function"] == "DEADLINEON" and r["calls"] == n for r in rows), rows)
            self.assertTrue(self.wait_for(lambda: all(s.state == "idle" for s in broker.slots), 20))
            # deadline restarts do not count against the cap (cap is 1; every worker is still up and uncapped)
            self.assertEqual([broker.window_attempts(s) for s in broker.slots], [0, 0, 0])
            self.assertEqual(conn.submit_groups([{"f": "ID", "k": "n", "n": 1, "v": [TWO]}]).raw(30)[1], [TWO])
        finally:
            stop()

    def test_quarantined_deadline_requests_enforce_budget_reply_excel_deadline_and_respect_origin(self):
        from broker import Conn

        broker, state, stop = start_broker(workers=3, extra={"cooldown": 0.3, "restart_cap": 1, "deadline_budget": 2})
        try:
            from touchstone import client
            conn = self.connect(state, name="lane-a")
            other = self.connect(state, name="lane-b")
            self.addCleanup(conn.close)
            self.addCleanup(other.close)
            rid = "aa" * 16
            sent, send = [], Conn.send

            def capture(conn, kind, body, high=False):
                if body.get("id") == rid and kind in (protocol.ERROR, protocol.DONE):
                    sent.append((kind, "seq" in body))
                send(conn, kind, body, high)

            with patch.object(Conn, "send", capture), self.assertRaises(client.ServiceError) as caught:
                conn.submit_rows("DEADLINEON", [[number(self.CRASH_MARK)]] * 12, rid=rid).raw(90)
            self.assertIn("excel-deadline", str(caught.exception))
            self.assertEqual(sent, [(protocol.ERROR, False)])
            # Resubmission by the same origin must reply excel-deadline, not excel-crashed.
            with self.assertRaises(client.ServiceError) as resub:
                conn.submit_rows("DEADLINEON", [[number(self.CRASH_MARK)]] * 12, rid=rid).raw(30)
            self.assertIn("excel-deadline", str(resub.exception))
            self.assertNotIn("excel-crashed", str(resub.exception))
            # A different origin with the same rid must not be blocked by lane-a's deadline quarantine.
            self.assertEqual(other.submit_rows("ID", [[number(TWO)]], rid=rid).raw(30)[1], [TWO])
            # Multi-call quarantined request must count against deadline_budget so a fresh rid is refused immediately.
            restarts_before = sum(s.restarts for s in broker.slots)
            with self.assertRaises(client.ServiceError) as budgeted:
                conn.submit_rows("DEADLINEON", [[number(self.CRASH_MARK)]] * 12).raw(30)
            self.assertIn("excel-deadline-budget", str(budgeted.exception))
            self.assertEqual(sum(s.restarts for s in broker.slots), restarts_before)
            # Deadlines never pollute crash counters or isolate the function as a crasher.
            self.assertEqual(sum(s.crashes for s in broker.slots), 0)
            self.assertEqual(broker.crash_counts, {})
            self.assertNotIn("DEADLINEON", broker.batcher.isolate)
        finally:
            stop()

    def test_cobatched_deadline_kill_does_not_charge_or_misattribute_innocent_request(self):
        broker, state, stop = start_broker(workers=3, extra={"cooldown": 0.3})
        try:
            from touchstone import client
            conn_ok = self.connect(state, name="innocent")
            conn_bad = self.connect(state, name="slow")
            self.addCleanup(conn_ok.close)
            self.addCleanup(conn_bad.close)
            # Pre-seed costs so ID and DEADLINEON share one bulk batch; hold kick until both pieces arrive.
            done_flag = threading.Event()
            async def setup_mixed():
                broker.batcher.costs["ID"] = 25.0
                broker.batcher.costs["DEADLINEON"] = 25.0
                orig_kick = broker.kick
                def hold_kick():
                    if len(broker.batcher.bulk.order) >= 2:
                        broker.kick = orig_kick
                        orig_kick()
                        done_flag.set()
                broker.kick = hold_kick
            asyncio.run_coroutine_threadsafe(setup_mixed(), broker.loop).result(5)
            p_ok = conn_ok.submit_rows("ID", [[number(ONE)]] * 4, priority="bulk")
            p_bad = conn_bad.submit_rows("DEADLINEON", [[number(self.CRASH_MARK)]] * 4, priority="bulk")
            self.assertTrue(done_flag.wait(10))
            self.assertEqual(p_ok.raw(60)[1], [ONE] * 4)
            ok_req = broker.requests[("tcp:innocent", p_ok.rid)]
            self.assertEqual(ok_req.deadline_kills, 0, "innocent co-batched request must not be charged a deadline kill")
            with self.assertRaises(client.ServiceError) as caught:
                p_bad.raw(60)
            self.assertIn("excel-deadline", str(caught.exception))
            self.assertIn("DEADLINEON", str(caught.exception))
            self.assertNotIn("ID", str(caught.exception))
            rows = [json.loads(line) for line in (state / "deadline-list.jsonl").read_text().splitlines()]
            self.assertTrue(rows)
            self.assertTrue(all(r["function"] == "DEADLINEON" for r in rows), rows)
            self.assertNotIn("ID", broker.deadline_times)
            self.assertEqual(broker.crash_counts, {})
        finally:
            stop()

    def test_serial_and_addin_deadline_kills_stay_in_deadline_path_not_crash_path(self):
        broker, state, stop = start_broker(
            workers=3, extra={"cooldown": 60.0, "restart_cap": 1, "addin_workers": ["TEST.XLAM"]}
        )
        try:
            from touchstone import client
            conn = self.connect(state, name="special")
            self.addCleanup(conn.close)
            serial_slot = next(s for s in broker.slots if s.serial and not s.addins)
            addin_slot = next(s for s in broker.slots if s.addins == ("TEST.XLAM",))

            with self.assertRaises(client.ServiceError) as serial_err:
                conn.submit_rows("DEADLINEON", [[number(self.CRASH_MARK)]], order="serial").raw(30)
            self.assertIn("excel-deadline", str(serial_err.exception))
            self.assertNotIn("excel-crashed", str(serial_err.exception))
            self.assertTrue(self.wait_for(lambda: serial_slot.state == "idle", 15))
            self.assertEqual(serial_slot.crashes, 0)
            self.assertEqual(broker.window_attempts(serial_slot), 0)

            with self.assertRaises(client.ServiceError) as addin_err:
                conn.submit_rows("DEADLINEON", [[number(self.CRASH_MARK)]], addins=["TEST.XLAM"]).raw(30)
            self.assertIn("excel-deadline", str(addin_err.exception))
            self.assertNotIn("excel-crashed", str(addin_err.exception))
            self.assertTrue(self.wait_for(lambda: addin_slot.state == "idle", 15))
            self.assertEqual(addin_slot.crashes, 0)
            self.assertEqual(broker.window_attempts(addin_slot), 0)

            self.assertEqual(broker.crash_counts, {})
            self.assertNotIn("DEADLINEON", broker.batcher.isolate)
            self.assertGreaterEqual(len(broker.deadline_times.get("DEADLINEON", [])), 2)
        finally:
            stop()

    def test_a_restart_after_a_stall_or_a_missed_deadline_does_not_count_against_the_cap(self):
        broker, state, stop = start_broker(workers=3, extra={"cooldown": 0.3, "restart_cap": 1})
        try:
            slot = broker.slots[0]
            fut = asyncio.run_coroutine_threadsafe(broker.restart_slot(slot, counted=False), broker.loop)
            fut.result(30)
            self.assertEqual(broker.window_attempts(slot), 0)
            self.assertIn(slot.state, ("idle", "new"))
        finally:
            stop()

    def test_a_function_whose_every_call_crashes_stops_at_the_budget_and_the_workers_stay_up(self):
        broker, state, stop = start_broker(workers=3, extra={"cooldown": 0.3, "crash_budget": 3})
        try:
            from touchstone import client
            conn = self.connect(state)
            pending = conn.submit_groups([{"f": "CRASHON", "k": "n", "n": 12, "v": [self.CRASH_MARK] * 12}])
            with self.assertRaises(client.ServiceError) as caught:
                pending.raw(90)
            self.assertIn("excel-crash-budget", str(caught.exception))
            listed = (state / "crash-list.jsonl").read_text().splitlines()
            self.assertEqual(len(listed), 3)
            self.assertTrue(self.wait_for(lambda: all(s.state == "idle" for s in broker.slots), 20))
            self.assertTrue(all(broker.window_attempts(s) < broker.opt("restart_cap", 3) for s in broker.slots))
            self.assertEqual(conn.submit_groups([{"f": "ID", "k": "n", "n": 1, "v": [TWO]}]).raw(30)[1], [TWO])
        finally:
            stop()

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

    def test_repeated_lost_workers_hit_the_cap_then_down_but_the_last_worker_stays_up_until_heal_now(self):
        broker, state, stop = start_broker(workers=2, extra={"cooldown": 0.3, "restart_cap": 2})
        try:
            from touchstone import client
            conn = self.connect(state)
            for _ in range(8):
                self.assertTrue(self.wait_for(lambda: any(s.state == "idle" for s in broker.slots)))
                if any(s.state == "down" for s in broker.slots):
                    break
                with self.assertRaises(client.ServiceError):
                    conn.submit_groups([{"f": "DIE", "k": "n", "n": 1, "v": [ONE]}]).raw(30)
            self.assertTrue(self.wait_for(lambda: any(s.state == "down" for s in broker.slots)))
            slot = next(s for s in broker.slots if s.state == "down")
            self.assertIn("restart cap", slot.down_reason)
            self.assertTrue(broker.health_line().startswith("DEGRADED "), broker.health_line())
            time.sleep(1.0)
            self.assertEqual(slot.state, "down")                      # the cap holds even after the cool-down
            self.assertIn("restart cap", (state / "log" / "waits.jsonl").read_text())
            # the last worker up is never taken down by the cap: it keeps restarting and nothing waits forever
            other = next(s for s in broker.slots if s is not slot)
            for _ in range(6):
                self.assertTrue(self.wait_for(lambda: other.state in ("idle", "down")))
                with self.assertRaises(client.ServiceError):
                    conn.submit_groups([{"f": "DIE", "k": "n", "n": 1, "v": [ONE]}]).raw(30)
            self.assertTrue(self.wait_for(lambda: other.state == "idle", 20))
            self.assertEqual(conn.submit_groups([{"f": "ID", "k": "n", "n": 1, "v": [TWO]}]).raw(30)[1], [TWO])
            (state / "heal-now").touch()
            self.assertTrue(self.wait_for(lambda: slot.state == "idle"))
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
            # A busy machine can stretch one round trip; the reserved worker shows in the typical wait, and no
            # wait may approach a bulk batch (a bulk call holds its worker for seconds).
            self.assertLess(sorted(waited)[len(waited) // 2], 0.5)
            self.assertLess(max(waited), 2.0)
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

    @unittest.skipUnless(os.name == "nt", "uses Windows process identities")
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


FAKE_RELAY = r"""
import sys, time
sys.path.insert(0, %r)
import protocol
mode, lives = sys.argv[1], float(sys.argv[2])
start = time.time()
reader = protocol.FrameReader()
out = sys.stdout.buffer
while True:
    data = sys.stdin.buffer.read1(65536)
    if not data:
        break
    for kind, body in reader.feed(data):
        if kind == protocol.PING and (mode != "silent" or time.time() - start < lives):
            out.write(protocol.pack(protocol.PONG, {**body, "sock": mode != "nosock"}, True)); out.flush()
    if mode == "exit" and time.time() - start >= lives:
        break
"""


class FakeRelayBroker:
    """The part of the broker the peer manager uses: serve a connection, log events."""

    def __init__(self):
        self.stopping = False
        self.events = []
        self.sessions = 0

    def log_event(self, event, **data):
        self.events.append((event, data))

    def broker_flags(self):
        return []

    async def serve(self, conn):
        import broker
        await broker.Broker.serve(self, conn)

    def on_frame(self, conn, kind, body):
        import broker
        broker.Broker.on_frame(self, conn, kind, body)

    conns = None


class RelaySupervisionTests(unittest.TestCase):
    """The broker restarts a relay that dies, goes silent or loses its lane socket, and logs and shows each (dc-us1ji)."""

    def run_peers(self, modes, seconds, **timing):
        import tempfile
        script = Path(tempfile.mkdtemp()) / "fake_relay.py"
        script.write_text(FAKE_RELAY % str(HERE), encoding="utf-8")
        broker_like = FakeRelayBroker()
        broker_like.conns = []
        launches = []

        async def spawn(host):
            mode = modes[min(len(launches), len(modes) - 1)]
            launches.append(mode)
            return await asyncio.create_subprocess_exec(sys.executable, "-I", str(script), mode, "0.6",
                                                        stdin=asyncio.subprocess.PIPE, stdout=asyncio.subprocess.PIPE)

        manager = peers.PeerManager(broker_like, ["lane1"], spawn=spawn, max_backoff=0.4, first_backoff=0.1, **timing)

        async def go():
            task = asyncio.create_task(manager.run())
            await asyncio.sleep(seconds)
            snapshots = manager.snapshot()
            summary = manager.summary()
            broker_like.stopping = True
            task.cancel()
            try:
                await task
            except asyncio.CancelledError:
                pass
            return snapshots, summary

        snapshots, summary = asyncio.run(go())
        return broker_like, launches, snapshots, summary

    TIMING = dict(ping_seconds=0.1, silent_seconds=0.5, socket_gone_seconds=0.5)

    def test_a_healthy_relay_stays_up_and_is_reported_up(self):
        events, launches, snapshots, summary = self.run_peers(["ok"], 1.5, **self.TIMING)
        self.assertEqual(launches, ["ok"])
        self.assertEqual(snapshots[0]["state"], "up")
        self.assertEqual(snapshots[0]["restarts"], 0)
        self.assertEqual(summary, "")

    def test_a_relay_that_stops_answering_is_restarted_and_the_drop_is_logged_with_time_and_host(self):
        events, launches, snapshots, summary = self.run_peers(["silent", "ok"], 3.0, **self.TIMING)
        self.assertEqual(launches[:2], ["silent", "ok"])
        down = [d for e, d in events.events if e == "peer-down"]
        self.assertTrue(down)
        self.assertEqual(down[0]["host"], "lane1")
        self.assertIn("silent", down[0]["reason"])
        self.assertTrue(down[0]["lost_utc"])
        self.assertEqual(snapshots[0]["state"], "up")
        self.assertGreaterEqual(snapshots[0]["restarts"], 1)

    def test_a_relay_whose_socket_is_gone_is_restarted(self):
        events, launches, snapshots, summary = self.run_peers(["nosock", "ok"], 3.0, **self.TIMING)
        self.assertEqual(launches[:2], ["nosock", "ok"])
        reasons = [d["reason"] for e, d in events.events if e == "peer-down"]
        self.assertIn("socket gone", reasons[0])

    def test_a_relay_whose_session_ends_is_restarted_with_backoff_and_shown_down_meanwhile(self):
        events, launches, snapshots, summary = self.run_peers(["exit"], 3.0, **self.TIMING)
        self.assertGreaterEqual(len(launches), 3)
        downs = [d for e, d in events.events if e == "peer-down"]
        self.assertEqual(downs[0]["reason"], "ssh session ended")
        self.assertGreater(downs[1]["retry_in_s"], downs[0]["retry_in_s"])

    def test_a_relay_that_cannot_start_is_shown_down_with_the_reason(self):
        broker_like = FakeRelayBroker()
        broker_like.conns = []

        async def spawn(host):
            raise OSError("ssh not found")

        manager = peers.PeerManager(broker_like, ["h"], spawn=spawn, max_backoff=0.1)

        async def go():
            task = asyncio.create_task(manager.run())
            await asyncio.sleep(0.4)
            broker_like.stopping = True
            task.cancel()
            try:
                await task
            except asyncio.CancelledError:
                pass

        asyncio.run(go())
        snap = manager.snapshot()[0]
        self.assertEqual(snap["state"], "down")
        self.assertIn("ssh not found", snap["reason"])
        self.assertIn("relays down: h since", manager.summary())

    def test_status_prints_one_line_per_relay_with_state_and_reason(self):
        import broker
        line = broker.format_relay({"host": "lane1", "state": "down", "since_utc": "2026-11-16T10:40:00+00:00", "sessions": 3,
                                    "restarts": 2, "last_rx_utc": None, "reason": "relay silent for 21s", "retry_in_s": 4.0})
        self.assertIn("relay lane1: DOWN since 2026-11-16T10:40:00", line)
        self.assertIn("relay silent for 21s", line)
        self.assertIn("retry_in=4.0s", line)
