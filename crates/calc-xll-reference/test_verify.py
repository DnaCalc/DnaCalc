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


if __name__ == "__main__":
    unittest.main()
