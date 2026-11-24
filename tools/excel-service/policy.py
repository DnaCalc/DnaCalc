"""Fail-closed admission: catalogue declarations, scalar values, no request formulas."""
import json
import math
from pathlib import Path
import re
import time

from client import from_bits

MAX_BYTES = 16 * 1024 * 1024
MAX_CALLS = 20000
MAX_FORMULA_CHARS = 8192
ERRORS = {"#NULL!": 2000, "#DIV/0!": 2007, "#VALUE!": 2015, "#REF!": 2023,
          "#NAME?": 2029, "#NUM!": 2036, "#N/A": 2042}
REGRESSION = {"LINEST", "LOGEST"}
# Functions whose paired-argument arities Excel refuses at formula entry; passing a refused
# formula inside a SAFEARRAY to Range.Formula2R1C1 makes Excel COM return E_OUTOFMEMORY.
EVEN_PAIR_FUNCTIONS = {"COUNTIFS", "IFS"}
ODD_PAIR_FUNCTIONS = {"SUMIFS", "AVERAGEIFS", "MAXIFS", "MINIFS"}
# Pure numeric questions are needed before their worksheet kernels exist.
ETS = {"FORECAST.ETS", "FORECAST.ETS.CONFINT", "FORECAST.ETS.SEASONALITY",
       "FORECAST.ETS.STAT"}


def check_formula_arity(name, arity):
    """Refuse arities that Excel refuses at formula entry (and would fail Range.Formula2R1C1)."""
    if name == "GETPIVOTDATA":
        if arity < 2 or arity > 254 or (arity >= 5 and arity % 2 == 1):
            raise ValueError(
                "GETPIVOTDATA requires 2, 3, or an even number of arguments (4..=254); "
                "odd arities >= 5 are refused by Excel formula entry"
            )
    elif name in EVEN_PAIR_FUNCTIONS:
        if arity < 2 or arity % 2 == 1:
            raise ValueError(f"{name} requires an even number of arguments (>= 2)")
    elif name in ODD_PAIR_FUNCTIONS:
        if arity < 3 or arity % 2 == 0:
            raise ValueError(f"{name} requires an odd number of arguments (>= 3)")


def _choose_arg_chars(rows, cols, at):
    """Exact character count of Program.cs Formula() CHOOSE({1,2;...},RC[...]) expansion."""
    # "CHOOSE({" + indices + "}," + cells + ")"
    count = rows * cols
    # sum of digit lengths for 1..=count plus separators (count - 1 commas/semicolons)
    idx_chars = sum(len(str(k)) for k in range(1, count + 1)) + max(0, count - 1)
    cell_chars = sum(4 + len(str(at + k + 1)) for k in range(count)) + max(0, count - 1)
    return 10 + idx_chars + cell_chars


def check_group_formula_limits(name, tokens, widths, values, n, result_index=None):
    """Refuse groups whose generated R1C1 formula exceeds Excel's 8192-character limit."""
    check_formula_arity(name, len(tokens))
    # Fixed overhead: "=" + name + "(" + commas + ")" + optional INDEX(...,r,c)
    base_len = 1 + len(name) + 2 + max(0, len(tokens) - 1)
    if result_index is not None:
        base_len += 8 + len(str(result_index[0])) + len(str(result_index[1]))
    at = 0
    literal_positions = []
    for tok, w in zip(tokens, widths):
        if tok[0] == "a":
            r, c = (int(x) for x in tok[1:].split("x"))
            base_len += _choose_arg_chars(r, c, at)
        elif tok == "S":
            literal_positions.append(at)
        elif tok == "B":
            base_len += 5  # worst case FALSE
        else:
            base_len += 4 + len(str(at + 1))  # "RC[N]"
        at += w
    if not literal_positions:
        if base_len > MAX_FORMULA_CHARS:
            raise ValueError(
                f"generated Excel formula ({base_len} chars) exceeds {MAX_FORMULA_CHARS}-character limit"
            )
        return
    width = sum(widths)
    for i in range(n):
        row_len = base_len
        for pos in literal_positions:
            text = values[i * width + pos]
            row_len += 2 + len(text) + text.count('"')
        if row_len > MAX_FORMULA_CHARS:
            raise ValueError(
                f"generated Excel formula ({row_len} chars) exceeds {MAX_FORMULA_CHARS}-character limit"
            )


def catalogue_names(root):
    """Names the admission policy exposes, read from the data-only calc-catalogue crate's export.

    The export (crates/calc-catalogue/tests/fixtures/admission-families.tsv) is generated from the
    declarations and checked by the calc-catalogue test admission_export_is_current.
    """
    export = Path(root) / "crates/calc-catalogue/tests/fixtures/admission-families.tsv"
    names = set()
    for line in export.read_text(encoding="utf-8").splitlines():
        family, _, name = line.partition("	")
        if family and name:
            names.add(name)
    if "ERF.PRECISE" not in names or "SUM" not in names:
        raise ValueError("Catalogue declaration export changed; regenerate admission-families.tsv")
    return AnyFunction(names | ETS)


# Owner 2026-11-12: this is a test service on our own Excel, so every function is admitted, with any
# argument types, arrays and spills. Only the remote service functions (deferred in CalcFunc) are skipped.
# IMAGE and WEBSERVICE are host-supported (owner 2026-11-07 and 2026-11-12, dc-fetch-f0tp), so they are admitted.
REMOTE_SERVICE = {"STOCKHISTORY", "TRANSLATE", "DETECTLANGUAGE", "PY", "COPILOT"}
FUNCTION_NAME = re.compile(r"(?:_[A-Z]+\.)?[A-Z][A-Z0-9_]*(?:\.[A-Z0-9_]+)*")


class AnyFunction:
    """Every well-formed function name except the remote service functions; iterates the catalogue families."""

    def __init__(self, listed):
        self.listed = set(listed)

    def __contains__(self, name):
        return isinstance(name, str) and name not in REMOTE_SERVICE and FUNCTION_NAME.fullmatch(name) is not None

    def __iter__(self):
        return iter(self.listed)


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
        raise ValueError("function is not admitted (remote service functions are skipped)")
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
        check_formula_arity(name, len(row))
        total += len(row)
        for arg in row:
            if isinstance(arg, dict) and arg.get("type") == "array":
                values = arg.get("values")
                if (set(arg) != {"type", "values"}
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
                if not isinstance(value, str) or len(value) > 1024 or "\x00" in value:
                    raise ValueError("text must be at most 1024 characters without NUL")
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
SPILL_FUNCTIONS = {"TEXTSPLIT", "MINVERSE", "MMULT", "MUNIT", "MODE.MULT", "FREQUENCY", "TREND", "GROWTH", "LINEST", "LOGEST",
                   "TRANSPOSE", "SEQUENCE", "SORT", "UNIQUE", "FILTER", "SORTBY", "TAKE", "DROP",
                   "CHOOSECOLS", "CHOOSEROWS", "HSTACK", "VSTACK", "TOCOL", "TOROW",
                   "WRAPCOLS", "WRAPROWS", "EXPAND", "REGEXEXTRACT", "TRIMRANGE", "FLATTEN",
                   "RANDARRAY", "GROUPBY", "PIVOTBY", "BYROW", "BYCOL", "MAP", "MAKEARRAY", "SCAN"}
TOKENS = re.compile(r"n|s|S|b|B|e|z|f|[arm][0-9]+x[0-9]+")
MAX_CASE_SHEETS = 16
MAX_CASE_CELLS = 10000
MAX_CASE_PROBES = 256
CASE_CELL_TYPES = {"number", "text", "boolean", "error", "formula"}
BAD_TEXT = re.compile(r'\x00')


class Admission:
    """The same fail-closed rules as validate(), for columnar groups."""

    def __init__(self, root):
        self.names = catalogue_names(root)

    def v1(self, raw, request_id):
        request = validate(decode(raw), self.names, request_id)
        return request["function"], request["rows"], request.get("result_index")

    def _check_cases(self, group):
        """Sheet cases (token L): bounded layouts of typed cells and probe formulas, nothing else."""
        n = group["n"]
        if group["k"] != "L" or group.get("spill") or group.get("x") is not None:
            raise ValueError("a sheet-case group has the one token L")
        if type(n) is not int or not 1 <= n <= MAX_GROUP_CALLS or not isinstance(group["v"], list) or len(group["v"]) != n:
            raise ValueError("expected 1..%d sheet cases" % MAX_GROUP_CALLS)
        for case in group["v"]:
            sheets, probes = case.get("sheets"), case.get("probes")
            if not isinstance(sheets, list) or not 1 <= len(sheets) <= MAX_CASE_SHEETS:
                raise ValueError("a sheet case has 1..%d sheets" % MAX_CASE_SHEETS)
            if not isinstance(probes, list) or not 1 <= len(probes) <= MAX_CASE_PROBES:
                raise ValueError("a sheet case has 1..%d probes" % MAX_CASE_PROBES)
            cells = 0
            for sheet in sheets:
                if not isinstance(sheet.get("name"), str) or not isinstance(sheet.get("cells", {}), dict):
                    raise ValueError("a sheet has a name and cells")
                for cell in sheet.get("cells", {}).values():
                    if not isinstance(cell, dict) or cell.get("type") not in CASE_CELL_TYPES:
                        raise ValueError("unsupported layout cell")
                    if cell["type"] == "number":
                        self._numbers([cell["bits"]])
                    if cell["type"] == "error" and cell.get("code") not in ("#NULL!", "#DIV/0!", "#VALUE!", "#REF!", "#NAME?", "#NUM!", "#N/A"):
                        raise ValueError("unsupported error cell")
                cells += len(sheet.get("cells", {}))
            if cells > MAX_CASE_CELLS:
                raise ValueError("a sheet case has at most %d cells" % MAX_CASE_CELLS)
            for probe in probes:
                entry = probe.get("entry")
                if entry not in (None, "local"):
                    raise ValueError("a probe's entry is \"local\" or absent")
                if not isinstance(probe.get("at"), str) or "!" not in probe["at"] or not isinstance(probe.get("formula"), str):
                    raise ValueError("a probe has a Sheet!A1 place and a formula")
                # A typed entry ("entry": "local") may be any text; otherwise the probe is a formula.
                if entry is None and not probe["formula"].startswith("="):
                    raise ValueError("a probe has a Sheet!A1 place and a formula")
                if entry is not None and not 0 < len(probe["formula"]) <= 255:
                    raise ValueError("a typed entry has 1..255 characters")

    def check(self, group):
        name = group["f"]
        if name == "_CASE":
            return self._check_cases(group)
        spill = bool(group.get("spill"))
        if spill:
            if name not in self.names:
                raise ValueError("function is not admitted as an array-returning question")
        elif not isinstance(name, str) or name not in self.names:
            raise ValueError("function is not admitted (remote service functions are skipped)")
        n = group["n"]
        limit = MAX_SPILL_CALLS if spill else MAX_GROUP_CALLS
        if type(n) is not int or not 1 <= n <= limit:
            raise ValueError("expected 1..%d calls per group" % limit)
        tokens = group["k"].split(",") if group["k"] else []
        if len(tokens) > 32:
            raise ValueError("expected at most 32 arguments")
        index = group.get("x")
        if name in REGRESSION and not (spill and index is None):
            if (not isinstance(index, list) or len(index) != 2 or any(type(v) is not int for v in index)
                    or not 1 <= index[0] <= 5 or not 1 <= index[1] <= 33):
                raise ValueError("regression requires a bounded scalar result_index")
        elif index is not None:
            raise ValueError("result_index is only supported for regression")
        widths = []
        for token in tokens:
            if not TOKENS.fullmatch(token):
                raise ValueError("unsupported argument type")
            if token[0] in "arm":
                r, c = (int(x) for x in token[1:].split("x"))
                if not (1 <= r <= 32 and 1 <= c <= 32 and r * c <= 128):
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
            elif token[0] in "rm":
                for i in range(n):
                    for cell in values[i * width + at:i * width + at + w]:
                        self._range_cell(cell)
            else:
                column = values[at::width]
                if token == "n":
                    self._numbers(column)
                elif token in ("s", "S"):
                    for v in column:
                        if not isinstance(v, str) or len(v) > 1024 or BAD_TEXT.search(v):
                            raise ValueError("text must be at most 1024 characters without NUL")
                elif token in ("b", "B"):
                    if any(type(v) is not bool for v in column):
                        raise ValueError("boolean must be true or false")
                elif token == "e":
                    if any(v not in ERRORS for v in column):
                        raise ValueError("unsupported worksheet error")
                elif token == "z":
                    if any(v is not None for v in column):
                        raise ValueError("blank carries no value")
                elif token == "f":
                    if any(not isinstance(v, str) or len(v.encode('utf-8')) > 65536 for v in column):
                        raise ValueError("file content must be UTF-8 text of at most 65536 bytes")
            at += w
        check_group_formula_limits(name, tokens, widths, values, n, index)

    @classmethod
    def _range_cell(cls, cell):
        if not isinstance(cell, dict):
            raise ValueError("range cells must be typed")
        kind = cell.get("type")
        if kind == "number" and set(cell) == {"type", "bits"}:
            cls._numbers([cell["bits"]])
        elif kind == "text" and set(cell) <= {"type", "value", "mode"} and cell.get("mode", "cell") == "cell":
            v = cell.get("value")
            if not isinstance(v, str) or len(v) > 1024 or BAD_TEXT.search(v):
                raise ValueError("invalid range text")
        elif kind == "boolean" and set(cell) <= {"type", "value", "mode"} and cell.get("mode", "cell") == "cell":
            if type(cell.get("value")) is not bool:
                raise ValueError("invalid range boolean")
        elif kind == "error" and set(cell) == {"type", "code"} and cell.get("code") in ERRORS:
            pass
        elif kind == "blank" and set(cell) == {"type"}:
            pass
        else:
            raise ValueError("invalid range cell")

    @staticmethod
    def _numbers(cells):
        for v in cells:
            if not isinstance(v, str) or len(v) != 16 or v.startswith(("7ff", "fff")):
                raise ValueError("number must be finite binary64 hex, passed through a cell")
        if not re.fullmatch("[0-9a-f]*", "".join(cells)):
            raise ValueError("number must be finite binary64 hex, passed through a cell")


def admission(root):
    return Admission(root)
