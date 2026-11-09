#!/usr/bin/env python3
"""Two-layer readout of a saved workbook (vol-1): per formula cell the leading PtgAttrSemi (the
syntactic volatile token) and the always-calculate bit.

  readout.py FILE.xlsb|FILE.xlsx [...]   -> JSON on stdout: {file: {sheetN: {A1: {...}}}}

.xlsb: BrtFmlaNum/String/Bool/Error (ids 9, 8, 10, 11) carry a 16-bit flag word then a
CellParsedFormula. Measured against `ca` in .xlsx on the calibration set (cal1): bit 1 (0x2) of that
word is the cell's always-calculate bit (`ca`). A formula stored as an array (dynamic-array entry, any
formula calling a user function) also has a BrtArrFmla (426) whose flag byte has the same bit at
bit 0, and whose formula holds the token stream; the cell's own record then starts with PtgExp.
The token is the first byte pair of the formula: 0x19 0x01 is PtgAttrSemi. Names (BrtName, 39) are
read the same way: first token of the name's formula. Nothing else is decoded.
.xlsx: `<f ca="1">`; the token does not exist in this format.
"""
import json, re, struct, sys, zipfile

def col_name(c):
    s = ""
    c += 1
    while c:
        c, r = divmod(c - 1, 26)
        s = chr(65 + r) + s
    return s

def records(data):
    pos = 0
    n = len(data)
    while pos < n:
        rid = data[pos]; pos += 1
        if rid & 0x80:
            rid = (rid & 0x7F) | (data[pos] << 7); pos += 1
        size = 0; shift = 0
        for _ in range(4):
            b = data[pos]; pos += 1
            size |= (b & 0x7F) << shift
            shift += 7
            if not b & 0x80:
                break
        yield rid, data[pos:pos + size]
        pos += size

def formula_info(body, off):
    cce = struct.unpack_from("<I", body, off)[0]
    rgce = body[off + 4: off + 4 + cce]
    first = rgce[:2].hex()
    return {"token": "semi" if rgce[:2] == b"\x19\x01" else None, "first": first, "cce": cce}

def read_xlsb(z):
    out = {}
    for name in sorted(z.namelist()):
        m = re.match(r"xl/worksheets/sheet(\d+)\.bin$", name)
        if not m:
            continue
        cells = {}
        row = 0
        for rid, body in records(z.read(name)):
            if rid == 0 and len(body) >= 4:
                row = struct.unpack_from("<I", body, 0)[0]
            elif rid in (8, 9, 10, 11):
                col = struct.unpack_from("<I", body, 0)[0]
                if rid == 9:
                    off = 8 + 8
                elif rid == 8:
                    ln = struct.unpack_from("<I", body, 8)[0]
                    off = 8 + 4 + 2 * ln
                else:
                    off = 8 + 1
                flags = struct.unpack_from("<H", body, off)[0]
                info = formula_info(body, off + 2)
                info.update({"ca": (flags >> 1) & 1, "flags": flags, "rec": rid})
                cells.setdefault(col_name(col) + str(row + 1), {}).update(info)
            elif rid == 426:  # BrtArrFmla: RfX (16) + flags byte + formula
                r1, r2, c1, c2 = struct.unpack_from("<iiii", body, 0)
                flags = body[16]
                info = formula_info(body, 17)
                info.update({"ca": flags & 1, "flags": flags, "rec": rid, "array": True})
                cells.setdefault(col_name(c1) + str(r1 + 1), {})["arr"] = info
        out["sheet" + m.group(1)] = cells
    return out

def read_names(z):
    names = {}
    if "xl/workbook.bin" not in z.namelist():
        return names
    for rid, body in records(z.read("xl/workbook.bin")):
        if rid == 39:  # BrtName: flags(4) chKey(1) itab(4) name(wide) formula
            pos = 9
            ln = struct.unpack_from("<I", body, pos)[0]
            pos += 4
            name = body[pos:pos + 2 * ln].decode("utf-16-le")
            pos += 2 * ln
            info = formula_info(body, pos)
            names[name] = info
    return names

def read_xlsx(z):
    out = {}
    for name in sorted(z.namelist()):
        m = re.match(r"xl/worksheets/sheet(\d+)\.xml$", name)
        if not m:
            continue
        xml = z.read(name).decode("utf8")
        cells = {}
        for cm in re.finditer(r'<c r="([A-Z]+\d+)"[^>]*?(?:/>|>(.*?)</c>)', xml, re.S):
            inner = cm.group(2) or ""
            fm = re.search(r"<f([^>]*?)(/>|>(.*?)</f>)", inner, re.S)
            if fm:
                attrs = fm.group(1)
                cells[cm.group(1)] = {"ca": 1 if re.search(r'\bca="(1|true)"', attrs) else 0,
                                      "t": (re.search(r'\bt="(\w+)"', attrs) or [None, None])[1]}
        out["sheet" + m.group(1)] = cells
    return out

def read(path):
    with zipfile.ZipFile(path) as z:
        names = z.namelist()
        if any(n.endswith(".bin") for n in names):
            out = read_xlsb(z)
            nm = read_names(z)
            if nm:
                out["names"] = nm
            return out
        return read_xlsx(z)

if __name__ == "__main__":
    result = {p: read(p) for p in sys.argv[1:]}
    json.dump(result, sys.stdout, indent=1, sort_keys=True)
