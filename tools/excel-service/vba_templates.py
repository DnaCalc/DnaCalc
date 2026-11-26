"""The catalogue of reviewed VBA probe templates (dc-erfm3).

A VBA case no longer carries VBA source: it names a template in vba-probes/catalogue.json and gives typed
parameters. Each template's module files (vba-probes/*.bas) are reviewed once and pinned here by SHA-256; the
worker (engines/excel/VbaTemplates.cs, same algorithm, same files) builds the .xlsm from the pinned text and the
parameters only. Parameter types: sheet (a case sheet's name), address (one A1 cell, no $), text (printable
ASCII, 1..255 characters, not starting with = + - or @), long (an integer in the Long range), and
enum:a|b|c (one listed word). Every {{name}} in a module is a declared parameter and every parameter is given.
"""
import hashlib
import json
import re
from pathlib import Path

DIRECTORY = Path(__file__).with_name("vba-probes")
CATALOGUE = json.loads((DIRECTORY / "catalogue.json").read_text(encoding="ascii"))
PLACEHOLDER = re.compile(r"\{\{([a-z][a-z_]*)\}\}")
ADDRESS = re.compile(r"([A-Z]{1,3})([1-9][0-9]{0,6})")
TEXT_FIRST_REFUSED = "=+-@"


def template_source(entry):
    """The pinned text of one module file; a file that differs from its pin is refused."""
    data = (DIRECTORY / entry["file"]).read_bytes()
    if hashlib.sha256(data).hexdigest() != entry["sha256"]:
        raise ValueError(f"VBA template file differs from its pinned hash: {entry['file']}")
    return data.decode("ascii")


def _address_ok(value):
    match = ADDRESS.fullmatch(value) if isinstance(value, str) else None
    if not match:
        return False
    column = 0
    for ch in match.group(1):
        column = column * 26 + ord(ch) - 64
    return column <= 16384 and int(match.group(2)) <= 1048576


def render(kind, value, sheet_names):
    """One parameter value as VBA source text, or a ValueError."""
    if kind == "sheet":
        if not isinstance(value, str) or value.lower() not in sheet_names:
            raise ValueError("VBA template parameter must name a case sheet")
        return '"' + value + '"'            # a case sheet name is letters, digits, spaces and underscores
    if kind == "address":
        if not _address_ok(value):
            raise ValueError("VBA template parameter must be one A1 cell")
        return '"' + value + '"'
    if kind == "text":
        if (not isinstance(value, str) or not 1 <= len(value) <= CATALOGUE["limits"]["text"]
                or any(ch < " " or ch > "~" for ch in value) or value[0] in TEXT_FIRST_REFUSED):
            raise ValueError("VBA template text parameter must be 1..255 printable ASCII characters not starting with = + - or @")
        return '"' + value.replace('"', '""') + '"'
    if kind == "long":
        limits = CATALOGUE["limits"]
        if type(value) is not int or not limits["long_min"] <= value <= limits["long_max"]:
            raise ValueError("VBA template parameter must be a Long integer")
        return str(value) if value >= 0 else f"({value})"
    if kind.startswith("enum:"):
        if value not in kind[5:].split("|"):
            raise ValueError("VBA template parameter must be one of the listed words")
        return value
    raise ValueError("unknown VBA template parameter type")


def expand(case):
    """A template case {sheets, template, params, calls, readback, event_log, names?} -> the same case with the
    modules the template builds, in the field set the validators check. Raises ValueError."""
    allowed = {"sheets", "template", "params", "calls", "readback", "event_log", "names"}
    if not isinstance(case, dict) or not {"sheets", "template", "params", "calls", "readback", "event_log"} <= case.keys() <= allowed:
        raise ValueError("invalid VBA case fields")
    entry = CATALOGUE["templates"].get(case["template"]) if isinstance(case["template"], str) else None
    if entry is None:
        raise ValueError("unknown VBA probe template")
    params = case["params"]
    if not isinstance(params, dict) or set(params) != set(entry["params"]):
        raise ValueError("VBA template parameters do not match the template")
    sheets = case["sheets"]
    if not isinstance(sheets, list) or any(not isinstance(s, dict) or not isinstance(s.get("name"), str) for s in sheets):
        raise ValueError("invalid VBA sheet count")
    sheet_names = {s["name"].lower() for s in sheets}
    rendered = {name: render(kind, params[name], sheet_names) for name, kind in entry["params"].items()}
    modules = []
    for module in entry["modules"]:
        source = PLACEHOLDER.sub(lambda m: rendered[m.group(1)] if m.group(1) in rendered else _unknown(m.group(1)),
                                 template_source(module))
        modules.append({"name": module["name"], "kind": module["kind"], "source": source})
    calls = case["calls"]
    if not isinstance(calls, list) or any(not isinstance(c, dict) or c.get("procedure") not in entry["entry_points"] for c in calls):
        raise ValueError("a VBA call must name an entry point of the template")
    expanded = {key: case[key] for key in ("sheets", "calls", "readback", "event_log")}
    expanded["names"] = case.get("names", {})
    expanded["modules"] = modules
    return expanded


def _unknown(name):
    raise ValueError(f"VBA template uses an undeclared parameter: {name}")
