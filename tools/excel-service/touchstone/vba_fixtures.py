"""Committed CalcDoc builds of the VBA test workbooks, keyed by the sheets and modules they carry.

The admission policy checks a case's workbook against its modules, so a test needs a real package.
`python vba_fixtures.py` rebuilds vba-workbooks.json through the CalcDoc harness (`cargo xtask vba-case
build`); the tests read it without building anything.
"""
import base64
import hashlib
import json
from pathlib import Path

FIXTURES = Path(__file__).with_name("vba-workbooks.json")


def key(case):
    shape = {"sheets": [sheet["name"] for sheet in case["sheets"]], "modules": case["modules"]}
    return hashlib.sha256(json.dumps(shape, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def workbook(case):
    """The committed base64 workbook for this case's sheets and modules, or None."""
    return json.loads(FIXTURES.read_text(encoding="ascii")).get(key(case))


def _main():
    import sys
    import client
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import test_touchstone
    fixtures = {}
    for source in test_touchstone.ACCEPTED_VBA_SOURCES:
        case = test_touchstone.vba_group(source)["v"][0]
        built = client._build_vba_workbook({k: case[k] for k in ("sheets", "modules")})
        fixtures[key(case)] = base64.b64encode(built).decode("ascii")
    FIXTURES.write_text(json.dumps(fixtures, indent=1, sort_keys=True) + "\n", encoding="ascii")


if __name__ == "__main__":
    _main()
