"""Synthetic ABI records test the existing boundary verifier, not Excel answers."""
import importlib.util
from pathlib import Path
import struct
import unittest

spec = importlib.util.spec_from_file_location("boundary_verify", Path(__file__).with_name("verify.py"))
verify = importlib.util.module_from_spec(spec)
spec.loader.exec_module(verify)


def big_data(count=0, pointer=0x12345678, padding=b"\xa5" * 4):
    raw = bytearray(b"\x5a" * 32)
    struct.pack_into("<Qi", raw, 0, pointer, count)
    raw[12:16] = padding
    struct.pack_into("<I", raw, 24, 2050)
    return {"type": 2050, "value": {"opaque_handle": hex(pointer), "byte_count": count},
            "abi_storage": {"layout": "xloper12-bigdata-long32/1", "raw32": raw.hex(),
                            "padding_after_cbData": padding.hex()}}


class BigDataBoundaryTests(unittest.TestCase):
    def test_signed_count_and_every_declared_field_survive(self):
        for count in [-(2**31), -1, 0, 1, 2**31 - 1]:
            with self.subTest(count=count):
                value = big_data(count)
                self.assertEqual(verify.defined_boundary(value), {"type": 2050, "value": value["value"]})
                self.assertEqual(verify.defined_boundary(value)["value"]["byte_count"], count)

    def test_padding_difference_remains_in_raw_records(self):
        left = big_data(padding=b"\xa5" * 4)
        right = big_data(padding=b"\x00" * 4)
        self.assertEqual(verify.defined_boundary(left), verify.defined_boundary(right))
        self.assertNotEqual(verify.normalize_x2(left), verify.normalize_x2(right))
        self.assertEqual(left["abi_storage"]["padding_after_cbData"], "a5a5a5a5")

    def test_different_defined_counts_cannot_match(self):
        self.assertNotEqual(verify.defined_boundary(big_data(0)), verify.defined_boundary(big_data(1)))
        changed = big_data()
        changed["value"]["byte_count"] = 1
        with self.assertRaisesRegex(AssertionError, "cbData differs"):
            verify.defined_boundary(changed)

    def test_raw_handle_identity_is_normalized_without_changing_input(self):
        left, right = big_data(pointer=0x1234), big_data(pointer=0x87654321)
        normalized = verify.normalize_x2(left)
        self.assertEqual(normalized, verify.normalize_x2(right))
        self.assertEqual(normalized, verify.normalize_x2(normalized))
        self.assertEqual(left["value"]["opaque_handle"], "0x1234")
        self.assertEqual(left["abi_storage"]["raw32"][:16], "3412000000000000")
        self.assertEqual(normalized["abi_storage"]["raw32"][16:], left["abi_storage"]["raw32"][16:])

    def test_raw_normalization_rejects_a_conflicting_handle(self):
        value = big_data()
        value["value"]["opaque_handle"] = "0x9999"
        with self.assertRaisesRegex(AssertionError, "pointer differs"):
            verify.normalize_x2(value)
        normalized = verify.normalize_x2(big_data())
        normalized["value"]["byte_count"] = 1
        with self.assertRaisesRegex(AssertionError, "cbData differs"):
            verify.normalize_x2(normalized)

    def test_raw_pointer_type_layout_and_padding_are_checked(self):
        changes = [
            (lambda v: v["value"].update(opaque_handle="0x9999"), "pointer differs"),
            (lambda v: v.update(type=2050 | 4096), "type differs"),
            (lambda v: v["abi_storage"].update(layout="unknown"), "Unknown BigData"),
            (lambda v: v["abi_storage"].update(padding_after_cbData="00000000"), "padding differs"),
            (lambda v: v["abi_storage"].update(raw32="00"), "Invalid BigData"),
        ]
        for change, reason in changes:
            with self.subTest(reason=reason):
                value = big_data()
                change(value)
                with self.assertRaisesRegex(AssertionError, reason):
                    verify.defined_boundary(value)

    def test_old_records_keep_their_original_count(self):
        old = {"type": 2050, "value": {"opaque_handle": "0x1234", "byte_count": 0x4b04534e00000000}}
        self.assertEqual(verify.defined_boundary(old), old)
        self.assertNotEqual(verify.defined_boundary(old)["value"]["byte_count"], 0)


class FourCountTests(unittest.TestCase):
    """C.1 of answer 13: four lanes, each pass vs mismatched + refused + missing."""

    def test_lanes_stay_apart_and_refusals_fail(self):
        rows = [{"lane": "value_cases", "outcome": "pass"}] * 289 + [
            {"lane": "ordered_boundary_cases", "outcome": "refused"},
            {"lane": "ordered_boundary_cases", "outcome": "pass"},
            {"lane": "lifecycle_ownership_cases", "outcome": "missing"},
        ]
        counts = verify.four_counts(rows)
        self.assertEqual(set(counts), set(verify.COUNT_LANES))
        self.assertEqual(counts["value_cases"]["pass_rate"], 1.0)
        self.assertEqual((counts["ordered_boundary_cases"]["pass"], counts["ordered_boundary_cases"]["fail"]), (1, 1))
        self.assertEqual(counts["lifecycle_ownership_cases"]["fail"], 1)
        self.assertIsNone(counts["framework_feature_lanes"]["pass_rate"])

    def test_manifest_records_the_profile(self):
        env = {"excel_file_version": "16.0.20527.20150", "process_bits": 64, "culture": "en-ZA",
               "multi_threaded_enabled": True, "multi_thread_count": 16, "fixture_files": {"X1.xll": "AB"},
               "workbook_name": "Book2", "mode": "control without debugger", "repository_commit": "c1"}
        rows = [{"operation": "RegisterXLL managed", "value": True},
                {"operation": "calculation mode =A()", "value": {"requested": -4105, "actual": -4105}},
                {"operation": "calculation mode =B()", "value": {"requested": -4105, "actual": -4105}}]
        manifest = verify.comparison_manifest(env, rows, [{"formula": "=A()"}], [{"executable_sha256": "E", "repository_commit": "c2"}])
        self.assertEqual(manifest["calculation_modes"], [{"requested": -4105, "actual": -4105}])
        self.assertEqual(manifest["load_order"], ["RegisterXLL managed"])
        self.assertEqual(manifest["mtr"], {"enabled": True, "threads": 16})
        self.assertEqual(manifest["repository_commits"], ["c1", "c2"])
        self.assertIn("automatic", manifest["trigger"])


if __name__ == "__main__":
    unittest.main()
