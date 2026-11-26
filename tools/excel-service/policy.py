"""Fail-closed admission: catalogue declarations, scalar values, no request formulas."""
import base64
import binascii
import json
import math
from pathlib import Path
import re
import time

from client import from_bits
from vba_grammar import check_modules, RULES as VBA_RULES, escape as vba_escape

MAX_BYTES = 16 * 1024 * 1024
MAX_CALLS = 20000
MAX_FORMULA_CHARS = 8192
ERRORS = {"#NULL!": 2000, "#DIV/0!": 2007, "#VALUE!": 2015, "#REF!": 2023,
          "#NAME?": 2029, "#NUM!": 2036, "#N/A": 2042}
REGRESSION = {"LINEST", "LOGEST"}


def formula_chars(text):
    return len(text.encode("utf-16-le", errors="surrogatepass")) // 2


def check_formula_length(size):
    if size > MAX_FORMULA_CHARS:
        raise ValueError(f"Excel formula ({size} chars) exceeds {MAX_FORMULA_CHARS}-character limit")


def _choose_arg_chars(rows, cols, at):
    """Exact character count of Program.cs Formula() CHOOSE({1,2;...},RC[...]) expansion."""
    # "CHOOSE({" + indices + "}," + cells + ")"
    count = rows * cols
    # sum of digit lengths for 1..=count plus separators (count - 1 commas/semicolons)
    idx_chars = sum(len(str(k)) for k in range(1, count + 1)) + max(0, count - 1)
    cell_chars = sum(4 + len(str(at + k + 1)) for k in range(count)) + max(0, count - 1)
    return 11 + idx_chars + cell_chars


def check_group_formula_limits(name, tokens, widths, values, n, result_index=None):
    """Refuse groups whose generated R1C1 formula exceeds Excel's 8192-character limit."""
    # Fixed overhead: "=" + name + "(" + commas + ")" + optional INDEX(...,r,c)
    base_len = 1 + len(name) + 2 + max(0, len(tokens) - 1)
    if result_index is not None:
        base_len += 8 + len(str(result_index[0])) + len(str(result_index[1]))
    at = 0
    literals = []
    for tok, w in zip(tokens, widths):
        if tok[0] == "a":
            r, c = (int(x) for x in tok[1:].split("x"))
            base_len += _choose_arg_chars(r, c, at)
        elif tok in ("S", "B"):
            literals.append((at, tok))
        else:
            base_len += 4 + len(str(at + 1))  # "RC[N]"
        at += w
    check_formula_length(base_len)
    if not literals:
        return
    width = sum(widths)
    for i in range(n):
        row_len = base_len
        for pos, token in literals:
            value = values[i * width + pos]
            if token == "S":
                row_len += 2 + formula_chars(value) + value.count('"')
            else:
                row_len += 4 if value else 5
        check_formula_length(row_len)


def catalogue_names(root):
    """Names the admission policy exposes, read from the data-only calc-catalogue crate's export.

    The export (crates/calc-catalogue/tests/fixtures/admission-families.tsv) is generated from the
    declarations and checked by the calc-catalogue test admission_export_is_current.
    """
    export = Path(root) / "crates/calc-catalogue/tests/fixtures/admission-families.tsv"
    arities = {}
    for line in export.read_text(encoding="utf-8").splitlines():
        _, name, low, high, *fields = line.split("\t")
        excluded = frozenset(map(int, fields[0].split(","))) if fields else frozenset()
        arities[name] = (int(low), int(high), excluded)
    if "ERF.PRECISE" not in arities or "SUM" not in arities:
        raise ValueError("Catalogue declaration export changed; regenerate admission-families.tsv")
    return AnyFunction(arities)


# Owner 2026-11-12: this is a test service on our own Excel, so every function is admitted, with any
# argument types, arrays and spills. Only the remote service functions (deferred in CalcFunc) are skipped.
# IMAGE and WEBSERVICE are host-supported (owner 2026-11-07 and 2026-11-12, dc-fetch-f0tp), so they are admitted.
REMOTE_SERVICE = {"STOCKHISTORY", "TRANSLATE", "DETECTLANGUAGE", "PY", "COPILOT"}
FUNCTION_NAME = re.compile(r"(?:_[A-Z]+\.)?[A-Z][A-Z0-9_]*(?:\.[A-Z0-9_]+)*")


def function_name(name):
    return name.removeprefix("_XLFN.").removeprefix("_XLWS.")


class AnyFunction:
    """Every well-formed function name except the remote service functions; iterates the catalogue families."""

    def __init__(self, arities):
        self.arities = arities

    def __contains__(self, name):
        return (isinstance(name, str) and FUNCTION_NAME.fullmatch(name) is not None
                and function_name(name) not in REMOTE_SERVICE)

    def __iter__(self):
        return iter(self.arities)

    def check_arity(self, name, count):
        """Decline entry-invalid shapes before Excel receives a batch."""
        shape = self.arities.get(function_name(name))
        if shape is not None:
            low, high, excluded = shape
            if not low <= count <= high or count in excluded:
                raise ValueError(f"{name} refuses {count} arguments at Excel formula entry")


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
        names.check_arity(name, len(row))
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

VBA_FRAME_BUDGET = 40 * 1024     # touchstone.protocol.FRAME_BUDGET: one VBA case is one group of one call
VBA_IDENTIFIER = re.compile(VBA_RULES["identifier"])
VBA_SHEET_NAME = re.compile(VBA_RULES["sheet_name"])
VBA_DEFINED_NAME = re.compile(VBA_RULES["defined_name"])
VBA_FORMULA_START = VBA_RULES["formula_chars"]
VBA_BLOCKED_FUNCTIONS = {name.upper() for name in VBA_RULES["blocked_functions"].split()}
VBA_BLOCKED_PREFIXES = tuple(name.upper() for name in VBA_RULES["blocked_function_prefixes"].split())
# A layout formula, read left to right: a quoted sheet name or a string literal (whichever starts first), then code.
VBA_FORMULA_TOKEN = re.compile(r"'(?:[^']|'')*'|\"(?:[^\"]|\"\")*\"")
def vba_wire_length(value):
    """The compact JSON length of a case as the worker measures it (Json.Write): every control or non-ASCII
    character is a six-character \\u escape, so the two validators agree on the 40 KiB budget (rw16)."""
    if isinstance(value, str):
        return 2 + sum(2 if ch in '"\\' else 6 if ch < " " or ch > "~" else 1 for ch in value)
    if isinstance(value, dict):
        return 2 + max(len(value) - 1, 0) + sum(vba_wire_length(k) + 1 + vba_wire_length(v) for k, v in value.items())
    if isinstance(value, list):
        return 2 + max(len(value) - 1, 0) + sum(vba_wire_length(v) for v in value)
    return len(json.dumps(value))


def escape_name(name):
    return name.encode("ascii", "backslashreplace").decode("ascii")


VBA_FUNCTION_CALL = re.compile(r"([A-Za-z_][A-Za-z0-9_.]*)\s*\(")
VBA_ADDRESS = re.compile(r"\$?[A-Za-z]{1,3}\$?[1-9][0-9]{0,6}")


def vba_layout_scan(formula):
    """The code of a layout formula with quoted sheet names and string literals set aside, or a ValueError."""
    code, at = [], 0
    while at < len(formula):
        quote = min((i for i in (formula.find("'", at), formula.find('"', at)) if i >= 0), default=-1)
        if quote < 0:
            code.append(formula[at:])
            break
        code.append(formula[at:quote])
        match = VBA_FORMULA_TOKEN.match(formula, quote)
        if not match:
            raise ValueError("VBA layout formulas refuse an unterminated quote")
        if match.group()[0] == "'":
            name = match.group()[1:-1]
            if not VBA_SHEET_NAME.fullmatch(name):
                raise ValueError("VBA layout formulas refuse a quoted name that is not a plain sheet name")
            code.append("Q!")
            at = match.end()
            if formula[at:at + 1] != "!":
                raise ValueError("VBA layout formulas refuse a quoted name that is not a sheet reference")
            at += 1
        else:
            code.append('""')
            at = match.end()
    return "".join(code)


def _vba_address(address):
    if not isinstance(address, str) or not VBA_ADDRESS.fullmatch(address):
        return False
    address = address.replace("$", "")
    column = 0
    i = 0
    while address[i].isalpha():
        column = column * 26 + ord(address[i].upper()) - ord("A") + 1
        i += 1
    return column <= 16384 and int(address[i:]) <= 1048576


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
            self._check_case_layout(sheets, case.get("names", {}))
            if not isinstance(probes, list) or not 1 <= len(probes) <= MAX_CASE_PROBES:
                raise ValueError("a sheet case has 1..%d probes" % MAX_CASE_PROBES)
            for probe in probes:
                entry = probe.get("entry")
                if entry not in (None, "local"):
                    raise ValueError("a probe's entry is \"local\" or absent")
                if not isinstance(probe.get("at"), str) or "!" not in probe["at"] or not isinstance(probe.get("formula"), str):
                    raise ValueError("a probe has a Sheet!A1 place and a formula")
                # A typed entry ("entry": "local") may be any text; otherwise the probe is a formula.
                if entry is None and not probe["formula"].startswith("="):
                    raise ValueError("a probe has a Sheet!A1 place and a formula")
                if entry is None:
                    check_formula_length(formula_chars(probe["formula"]))
                if entry is not None and not 0 < len(probe["formula"]) <= 255:
                    raise ValueError("a typed entry has 1..255 characters")

    def _check_case_layout(self, sheets, names):
        if not isinstance(names, dict) or any(not isinstance(formula, str) for formula in names.values()):
            raise ValueError("defined names need formula text")
        for formula in names.values():
            check_formula_length(formula_chars(formula))
        if not isinstance(sheets, list) or not 1 <= len(sheets) <= MAX_CASE_SHEETS:
            raise ValueError("a sheet case has 1..%d sheets" % MAX_CASE_SHEETS)
        cells = 0
        for sheet in sheets:
            if not isinstance(sheet.get("name"), str) or not isinstance(sheet.get("cells", {}), dict):
                raise ValueError("a sheet has a name and cells")
            for field, limit in (("hidden_rows", 1048576), ("hidden_columns", 16384)):
                hidden = sheet.get(field, [])
                if (not isinstance(hidden, list) or len(hidden) > 1000
                        or any(type(n) is not int or not 1 <= n <= limit for n in hidden)):
                    raise ValueError(f"{field} are at most 1000 positions inside the worksheet")
            for cell in sheet.get("cells", {}).values():
                if not isinstance(cell, dict) or cell.get("type") not in CASE_CELL_TYPES:
                    raise ValueError("unsupported layout cell")
                if cell["type"] == "formula":
                    if not isinstance(cell.get("formula"), str):
                        raise ValueError("a formula cell needs formula text")
                    check_formula_length(formula_chars(cell["formula"]))
                if cell["type"] == "number":
                    self._numbers([cell["bits"]])
                if cell["type"] == "error" and cell.get("code") not in ("#NULL!", "#DIV/0!", "#VALUE!", "#REF!", "#NAME?", "#NUM!", "#N/A"):
                    raise ValueError("unsupported error cell")
            cells += len(sheet.get("cells", {}))
        if cells > MAX_CASE_CELLS:
            raise ValueError("a sheet case has at most %d cells" % MAX_CASE_CELLS)

    def check(self, group):
        name = group["f"]
        if name == "_VBA" or "V" in group["k"].split(","):
            return self._check_vba_case(group)
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
        self.names.check_arity(name, len(tokens))
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

    def _check_vba_case(self, group):
        values = group.get("v")
        if (group["f"] != "_VBA" or group["k"] != "V" or type(group.get("n")) is not int
                or group["n"] != 1 or not isinstance(values, list) or len(values) != 1
                or set(group) - {"f", "k", "n", "v", "o", "i"}):
            raise ValueError("a VBA group contains one VBA case")
        case = values[0]
        # No workbook travels with the case: the worker builds the package from the modules it checked.
        if not isinstance(case, dict) or set(case) != {"sheets", "names", "modules", "calls", "readback", "event_log"}:
            raise ValueError("invalid VBA case fields")
        if vba_wire_length(group) > VBA_FRAME_BUDGET:
            raise ValueError("a VBA case must fit one submit frame (the 40 KiB frame budget)")
        modules = case["modules"]
        if not isinstance(modules, list) or not 1 <= len(modules) <= VBA_RULES["max_modules"]:
            raise ValueError("invalid VBA module count")
        sheets = case["sheets"]
        if not isinstance(sheets, list) or not 1 <= len(sheets) <= MAX_CASE_SHEETS:
            raise ValueError("invalid VBA sheet count")
        triples, source_chars = [], 0
        for module in modules:
            if not isinstance(module, dict) or set(module) != {"name", "kind", "source"}:
                raise ValueError("invalid VBA module fields")
            name, kind, source = module["name"], module["kind"], module["source"]
            if not isinstance(name, str) or kind not in ("standard", "document"):
                raise ValueError("invalid or duplicate VBA module name or kind")
            if not isinstance(source, str) or not 1 <= formula_chars(source) <= VBA_RULES["max_module_chars"]:
                raise ValueError("invalid or oversized VBA module source")
            for ch in source:
                if ch < " " and ch not in "\t\r\n" or ord(ch) > 0x7e:
                    raise ValueError(f"VBA construct refused: {vba_escape(ch)} (a control or non-ASCII character)")
            source_chars += formula_chars(source)
            triples.append((name, kind, source))
        if source_chars > VBA_RULES["max_source_chars"]:
            raise ValueError("oversized VBA module source")
        entry_points = check_modules(triples, len(sheets))
        for entry in entry_points:
            # A layout formula may call only worksheet functions; a probe function with such a name would answer
            # it on an Excel build that lacks the function, with a parameter the taint check never saw.
            name = function_name(entry.rpartition(".")[2].upper())
            if name in self.names.arities or name in VBA_BLOCKED_FUNCTIONS or name.startswith(VBA_BLOCKED_PREFIXES):
                raise ValueError(f"a VBA procedure name collides with a worksheet function: {escape_name(name)}")
        calls = case["calls"]
        if not isinstance(calls, list) or not 1 <= len(calls) <= VBA_RULES["max_calls"]:
            raise ValueError("invalid VBA call count")
        for call in calls:
            if not isinstance(call, dict) or set(call) != {"procedure", "args"}:
                raise ValueError("invalid VBA call fields")
            procedure, args = call["procedure"], call["args"]
            if not isinstance(procedure, str) or procedure.lower() not in entry_points:
                raise ValueError("VBA call must name a public standard-module procedure in the case")
            if not isinstance(args, list) or len(args) > VBA_RULES["max_args"]:
                raise ValueError("invalid VBA argument count")
            for arg in args:
                self._range_cell(arg)
                # Case text reaches cells through the probe's own writes: it must be data, never a formula.
                if arg.get("type") == "text" and arg.get("value", "")[:1] in tuple(VBA_FORMULA_START):
                    raise ValueError("VBA text arguments must not start with = + - or @")
        sheet_names = set()
        for sheet in sheets:
            if (not isinstance(sheet, dict) or not isinstance(sheet.get("name"), str)
                    or not VBA_SHEET_NAME.fullmatch(sheet["name"]) or sheet["name"].lower() in sheet_names
                    or not isinstance(sheet.get("cells", {}), dict)
                    or set(sheet) - {"name", "cells", "hidden_rows", "hidden_columns"}
                    or not all(_vba_address(at) for at in sheet.get("cells", {}))):
                raise ValueError("invalid or duplicate VBA layout sheet")
            sheet_names.add(sheet["name"].lower())
        # Reuse the sheet layout checks; VBA readbacks are addresses, with no formula entry.
        try:
            self._check_case_layout(sheets, case["names"])
        except (AttributeError, KeyError, TypeError) as error:
            raise ValueError("invalid VBA worksheet layout") from error
        for sheet in sheets:
            for cell in sheet.get("cells", {}).values():
                if cell["type"] == "formula":
                    self._check_vba_layout_formula(cell["formula"])
                elif cell["type"] == "text" and (not isinstance(cell.get("value"), str) or "\x00" in cell["value"]):
                    raise ValueError("invalid VBA layout text")
                elif cell["type"] == "boolean" and type(cell.get("value")) is not bool:
                    raise ValueError("invalid VBA layout boolean")
        for name, formula in case["names"].items():
            if not VBA_DEFINED_NAME.fullmatch(name) or name.lower().startswith("auto_"):
                raise ValueError("invalid VBA defined name")
            self._check_vba_layout_formula(formula)
        for field in ("readback", "event_log"):
            addresses = case[field]
            if not isinstance(addresses, list) or len(addresses) > VBA_RULES["max_reads"]:
                raise ValueError(f"invalid VBA {field} count")
            for place in addresses:
                sheet, bang, cell = place.rpartition("!") if isinstance(place, str) else ("", "", "")
                if not bang or sheet.lower() not in sheet_names or not _vba_address(cell):
                    raise ValueError(f"VBA {field} must name a case sheet and one A1 cell")

    def _check_vba_layout_formula(self, formula):
        """A layout formula runs under CalculateFull: catalogue functions only, none that reach out."""
        body = vba_layout_scan(formula)
        if any(ch in body for ch in "[]\\|"):
            raise ValueError("VBA layout formulas refuse external references")
        for call in VBA_FUNCTION_CALL.findall(body):
            name = call.upper()
            while name.startswith(("_XLFN.", "_XLWS.")):
                name = name[6:]
            name = function_name(name)
            if (name in VBA_BLOCKED_FUNCTIONS or name.startswith(VBA_BLOCKED_PREFIXES)
                    or name not in self.names.arities):
                raise ValueError(f"VBA layout formulas refuse the function {call}")

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
