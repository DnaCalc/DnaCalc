"""Fail-closed admission: catalogue declarations, scalar values, no request formulas."""
import json
import math
from pathlib import Path
import re
import time

from client import from_bits

MAX_BYTES = 16 * 1024 * 1024
MAX_CALLS = 20000
ERRORS = {"#NULL!": 2000, "#DIV/0!": 2007, "#VALUE!": 2015, "#REF!": 2023,
          "#NAME?": 2029, "#NUM!": 2036, "#N/A": 2042}
# Positive selection of scalar computation families. Dynamic references, callable
# helpers, pivots, host facts, external data and random functions are not exposed.
FAMILIES = "math statistics distributions engineering rounding_more financial bonds dates text_functions logical_more".split()
DENY = {"CALL", "REGISTER", "REGISTER.ID", "RTD", "DDE", "WEBSERVICE", "INFO", "CELL",
        "INDIRECT", "OFFSET", "NOW", "TODAY", "RAND", "RANDBETWEEN", "RANDARRAY",
        "TREND", "GROWTH", "FREQUENCY", "MINVERSE", "MMULT", "MUNIT", "MODE.MULT"}

REGRESSION = {"LINEST", "LOGEST"}
# Pure numeric questions are needed before their worksheet kernels exist.
ETS = {"FORECAST.ETS", "FORECAST.ETS.CONFINT", "FORECAST.ETS.SEASONALITY",
       "FORECAST.ETS.STAT"}
NUMERIC_ARRAYS = REGRESSION | ETS


def catalogue_names(root):
    directory = Path(root) / "crates/calc-func/src/catalogue"
    names = set()
    for family in FAMILIES:
        source = (directory / (family + ".rs")).read_text(encoding="utf-8")
        # Only ported_family! declaration rows, never comments, test strings or kernels.
        source = source.split("ported_family! {", 1)[-1] if "ported_family! {" in source else ""
        names.update(re.findall(r'^\s*\d+\s+\w+\s+"([A-Z][A-Z0-9.]*)"\s+\(', source, re.M))
    # Numeric text/formatting kernels are scalar for scalar arguments. Select only
    # these entries from their family, which also contains array and hyperlink calls.
    source = (directory / "text_format.rs").read_text(encoding="utf-8")
    declared = set(re.findall(r'^\s*\d+\s+\w+\s+"([A-Z][A-Z0-9.]*)"\s+\(', source, re.M))
    names.update(declared & {"FIXED", "DOLLAR", "TEXT", "VALUE", "NUMBERVALUE",
                             "VALUETOTEXT", "ASC", "DBCS"})
    # The base rounding family uses explicit call declarations, rather than the
    # ported_family rows above. Its functions also return scalar values.
    source = (directory / "rounding.rs").read_text(encoding="utf-8")
    names.update(re.findall(r'^\s*call\(\w+,\s*"([A-Z][A-Z0-9.]*)",', source, re.M))
    source = (directory / "aggregate.rs").read_text(encoding="utf-8")
    names.update(re.findall(r'aggregate\(\w+, "([A-Z]+)"', source))
    if "ERF.PRECISE" not in names or "SUM" not in names:
        raise ValueError("Catalogue declaration format changed; review admission")
    return (names | ETS) - DENY


def decode(raw):
    if len(raw) > MAX_BYTES:
        raise ValueError("request exceeds 16 MiB")
    def unique(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError("duplicate JSON key")
            result[key] = value
        return result
    return json.loads(raw, object_pairs_hook=unique,
                      parse_constant=lambda _: (_ for _ in ()).throw(ValueError("nonfinite JSON")))


def validate(request, names, request_id):
    if not isinstance(request, dict) or set(request) - {"result_index"} != {"version", "id", "function", "rows", "created", "expires"}:
        raise ValueError("expected only version, id, function, rows, created, expires")
    if type(request["version"]) is not int or request["version"] != 1 or request["id"] != request_id:
        raise ValueError("request identity/version mismatch")
    for key in ("created", "expires"):
        if type(request[key]) not in (float, int) or not math.isfinite(request[key]):
            raise ValueError("invalid request timestamp")
    now = time.time()
    if not request["created"] - 60 <= now < request["expires"] or not 0 < request["expires"] - request["created"] <= 901:
        raise ValueError("request expired or invalid lifetime (maximum 900 seconds)")
    name = request["function"]
    if not isinstance(name, str) or name not in names:
        raise ValueError("function is outside the service's safe catalogue subset")
    if name in REGRESSION:
        index = request.get("result_index")
        if (not isinstance(index, list) or len(index) != 2
                or any(type(v) is not int for v in index)
                or not 1 <= index[0] <= 5 or not 1 <= index[1] <= 33):
            raise ValueError("regression requires a bounded scalar result_index")
    elif "result_index" in request:
        raise ValueError("result_index is only supported for regression")
    rows = request["rows"]
    if not isinstance(rows, list) or not 1 <= len(rows) <= MAX_CALLS:
        raise ValueError("expected 1..20000 calls")
    total = 0
    for row in rows:
        if not isinstance(row, list) or len(row) > 32:
            raise ValueError("expected rows of at most 32 scalar arguments")
        total += len(row)
        for arg in row:
            if isinstance(arg, dict) and arg.get("type") == "array":
                values = arg.get("values")
                if (name not in NUMERIC_ARRAYS or set(arg) != {"type", "values"}
                        or not isinstance(values, list) or not 1 <= len(values) <= 32
                        or any(not isinstance(r, list) for r in values)
                        or not 1 <= len(values[0]) <= 32
                        or any(len(r) != len(values[0]) for r in values)
                        or len(values)*len(values[0]) > 128):
                    raise ValueError("expected a bounded rectangular numeric array")
                leaves = [v for r in values for v in r]
                if any(not isinstance(v, dict) or set(v) != {"type", "bits"}
                       or v["type"] != "number" or not isinstance(v["bits"], str)
                       or not re.fullmatch("[0-9a-f]{16}", v["bits"])
                       or not math.isfinite(from_bits(v["bits"])) for v in leaves):
                    raise ValueError("arrays contain only finite numeric cells")
                total += len(leaves)-1
                continue
            if not isinstance(arg, dict) or arg.get("mode", "cell") not in ("cell", "literal"):
                raise ValueError("expected typed cell or literal argument")
            kind = arg.get("type")
            fields = {"type", "mode"}
            if kind == "number":
                fields.add("bits")
                if arg.get("mode", "cell") != "cell" or not isinstance(arg.get("bits"), str) or not re.fullmatch("[0-9a-f]{16}", arg["bits"]) or not math.isfinite(from_bits(arg["bits"])):
                    raise ValueError("number must be finite binary64 hex, passed through a cell")
            elif kind == "text":
                fields.add("value")
                value = arg.get("value")
                if not isinstance(value, str) or len(value) > 1024 or re.search(r'[\\/:\[\]!|\x00-\x1f]', value):
                    raise ValueError("text contains a path, reference, URL or control character")
            elif kind == "boolean":
                fields.add("value")
                if type(arg.get("value")) is not bool:
                    raise ValueError("boolean must be true or false")
            elif kind == "error":
                fields.add("code")
                if arg.get("code") not in ERRORS:
                    raise ValueError("unsupported worksheet error")
            elif kind == "blank":
                if arg.get("mode", "cell") != "cell":
                    raise ValueError("blank must be a cell")
            else:
                raise ValueError("unsupported argument type")
            if set(arg) - fields:
                raise ValueError("unexpected argument field")
    if total > 200000:
        raise ValueError("batch exceeds 200000 scalar arguments")
    return request


# --- touchstone/1 admission plug-in (broker --admission policy:admission) -----------------------------------
MAX_GROUP_CALLS = 250000
MAX_SPILL_CALLS = 1000
# Array-returning functions, admitted only as spill groups (arguments in cells, the whole spill read back).
SPILL_FUNCTIONS = {"TEXTSPLIT", "MINVERSE", "MMULT", "MUNIT", "MODE.MULT", "FREQUENCY", "TREND", "GROWTH",
                   "TRANSPOSE", "SEQUENCE", "SORT", "UNIQUE"}
TOKENS = re.compile(r"n|s|S|b|B|e|z|a[0-9]+x[0-9]+")
BAD_TEXT = re.compile(r'[\\/:\[\]!|\x00-\x1f]')


class Admission:
    """The same fail-closed rules as validate(), for columnar groups."""

    def __init__(self, root):
        self.names = catalogue_names(root)

    def v1(self, raw, request_id):
        request = validate(decode(raw), self.names, request_id)
        return request["function"], request["rows"], request.get("result_index")

    def check(self, group):
        name = group["f"]
        spill = bool(group.get("spill"))
        if spill:
            if name not in SPILL_FUNCTIONS:
                raise ValueError("function is not admitted as an array-returning question")
        elif not isinstance(name, str) or name not in self.names:
            raise ValueError("function is outside the service's safe catalogue subset")
        n = group["n"]
        limit = MAX_SPILL_CALLS if spill else MAX_GROUP_CALLS
        if type(n) is not int or not 1 <= n <= limit:
            raise ValueError("expected 1..%d calls per group" % limit)
        tokens = group["k"].split(",") if group["k"] else []
        if len(tokens) > 32:
            raise ValueError("expected at most 32 arguments")
        index = group.get("x")
        if name in REGRESSION:
            if (not isinstance(index, list) or len(index) != 2 or any(type(v) is not int for v in index)
                    or not 1 <= index[0] <= 5 or not 1 <= index[1] <= 33):
                raise ValueError("regression requires a bounded scalar result_index")
        elif index is not None:
            raise ValueError("result_index is only supported for regression")
        widths = []
        for token in tokens:
            if not TOKENS.fullmatch(token):
                raise ValueError("unsupported argument type")
            if token[0] == "a":
                r, c = (int(x) for x in token[1:].split("x"))
                if (name not in NUMERIC_ARRAYS and not spill) or not (1 <= r <= 32 and 1 <= c <= 32 and r * c <= 128):
                    raise ValueError("expected a bounded rectangular numeric array")
                widths.append(r * c)
            else:
                widths.append(1)
        width = sum(widths)
        values = group["v"]
        if not isinstance(values, list) or len(values) != n * width:
            raise ValueError("value count does not match the group's shape")
        at = 0
        for token, w in zip(tokens, widths):
            if token[0] == "a":
                self._numbers([v for i in range(n) for v in values[i * width + at:i * width + at + w]])
            else:
                column = values[at::width]
                if token == "n":
                    self._numbers(column)
                elif token in ("s", "S"):
                    for v in column:
                        if not isinstance(v, str) or len(v) > 1024 or BAD_TEXT.search(v):
                            raise ValueError("text contains a path, reference, URL or control character")
                elif token in ("b", "B"):
                    if any(type(v) is not bool for v in column):
                        raise ValueError("boolean must be true or false")
                elif token == "e":
                    if any(v not in ERRORS for v in column):
                        raise ValueError("unsupported worksheet error")
                elif token == "z":
                    if any(v is not None for v in column):
                        raise ValueError("blank carries no value")
            at += w

    @staticmethod
    def _numbers(cells):
        for v in cells:
            if not isinstance(v, str) or len(v) != 16 or v.startswith(("7ff", "fff")):
                raise ValueError("number must be finite binary64 hex, passed through a cell")
        if not re.fullmatch("[0-9a-f]*", "".join(cells)):
            raise ValueError("number must be finite binary64 hex, passed through a cell")


def admission(root):
    return Admission(root)
