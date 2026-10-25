"""Compares what lies on the cells of two packages of one workbook after the same row and column
insertions and deletions (dc-jhn): the one Excel saved after making the edits itself
(compare-structural.ps1 -ExcelOut) and the one DNA Calc saved after making them through calc-host.

For each sheet (by name) it compares, as sets: merged cells; hyperlinks' cells; data validations
(sqref, type, formulas); conditional formats (sqref, formulas); each row's height, custom-height
flag and hidden flag (rows that state any); each column's width and hidden flag (expanded from the
col spans); each cell's number format code (resolved through the styles part); comments (cell and
text); notes' shapes (VML x:Row and x:Column); threaded comments' cells; drawing anchors (from and
to); tables' and filters' areas. And for the workbook, its defined names. Excel rewrites the
package when it saves (styles re-indexed, shared formulas regrouped, cells' order), so values in
cells are not compared here: compare-structural.ps1 compares those in Excel.

    python compare-structural-parts.py <excel.xlsx> <saved.xlsx> <out.json>
"""

import json
import re
import sys
import zipfile
import xml.etree.ElementTree as ET

MAIN = "{http://schemas.openxmlformats.org/spreadsheetml/2006/main}"
REL = "{http://schemas.openxmlformats.org/officeDocument/2006/relationships}"
PKG = "{http://schemas.openxmlformats.org/package/2006/relationships}"
XDR = "{http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing}"
TC = "{http://schemas.microsoft.com/office/spreadsheetml/2018/threadedcomments}"
X14 = "{http://schemas.microsoft.com/office/spreadsheetml/2009/9/main}"
XM = "{http://schemas.microsoft.com/office/excel/2006/main}"

BUILTIN_FORMATS = {
    0: "General", 1: "0", 2: "0.00", 3: "#,##0", 4: "#,##0.00", 9: "0%", 10: "0.00%",
    11: "0.00E+00", 12: "# ?/?", 13: "# ??/??", 14: "m/d/yyyy", 15: "d-mmm-yy", 16: "d-mmm",
    17: "mmm-yy", 18: "h:mm AM/PM", 19: "h:mm:ss AM/PM", 20: "h:mm", 21: "h:mm:ss",
    22: "m/d/yyyy h:mm", 37: "#,##0 ;(#,##0)", 38: "#,##0 ;[Red](#,##0)",
    39: "#,##0.00;(#,##0.00)", 40: "#,##0.00;[Red](#,##0.00)", 45: "mm:ss", 46: "[h]:mm:ss",
    47: "mmss.0", 48: "##0.0E+0", 49: "@",
}


def resolve(base, target):
    if target.startswith("/"):
        return target[1:]
    parts = base.split("/")[:-1]
    for piece in target.split("/"):
        if piece == "..":
            parts.pop()
        elif piece != ".":
            parts.append(piece)
    return "/".join(parts)


def rels_of(z, part):
    folder, _, name = part.rpartition("/")
    path = f"{folder}/_rels/{name}.rels" if folder else f"_rels/{name}.rels"
    if path not in z.namelist():
        return []
    root = ET.fromstring(z.read(path))
    out = []
    for rel in root.iter(PKG + "Relationship"):
        if rel.get("TargetMode") == "External":
            continue
        out.append((rel.get("Type").rsplit("/", 1)[-1], resolve(part, rel.get("Target")), rel.get("Id")))
    return out


def text_of(element):
    return "".join(t.text or "" for t in element.iter(MAIN + "t"))


def column_number(letters):
    n = 0
    for ch in letters:
        n = n * 26 + ord(ch) - 64
    return n


def cell_position(ref):
    m = re.match(r"([A-Z]+)(\d+)$", ref)
    return int(m.group(2)), column_number(m.group(1))


def number_formats(z):
    root = ET.fromstring(z.read("xl/styles.xml"))
    custom = {}
    fmts = root.find(MAIN + "numFmts")
    if fmts is not None:
        for f in fmts:
            custom[int(f.get("numFmtId"))] = f.get("formatCode")
    xfs = root.find(MAIN + "cellXfs")
    codes = []
    for xf in xfs:
        fid = int(xf.get("numFmtId", "0"))
        codes.append(custom.get(fid, BUILTIN_FORMATS.get(fid, f"builtin {fid}")))
    return codes


def sorted_sqref(text):
    return " ".join(sorted((text or "").split()))


def sheet_record(z, part, formats):
    root = ET.fromstring(z.read(part))
    record = {}
    record["merged"] = sorted(m.get("ref") for m in root.iter(MAIN + "mergeCell"))
    record["hyperlinks"] = sorted(h.get("ref") for h in root.iter(MAIN + "hyperlink"))
    record["validations"] = sorted(
        json.dumps([sorted_sqref(v.get("sqref")), v.get("type"),
                    (v.findtext(MAIN + "formula1") or ""), (v.findtext(MAIN + "formula2") or "")])
        for v in root.iter(MAIN + "dataValidation"))
    # Excel 2010's validations (in the extensions), which name another sheet.
    for v in root.iter(X14 + "dataValidation"):
        formulas = [f.text or "" for f in v.iter(XM + "f")]
        record["validations"].append(json.dumps(
            [sorted_sqref(v.findtext(XM + "sqref")), v.get("type")] + formulas))
    record["validations"].sort()
    record["conditional"] = sorted(
        json.dumps([sorted_sqref(c.get("sqref")),
                    [f.text or "" for f in c.iter(MAIN + "formula")]])
        for c in root.iter(MAIN + "conditionalFormatting"))
    rows = {}
    formats_by_cell = {}
    for row in root.iter(MAIN + "row"):
        number = int(row.get("r"))
        attributes = (row.get("ht"), row.get("customHeight") in ("1", "true"),
                      row.get("hidden") in ("1", "true"))
        if attributes != (None, False, False):
            rows[number] = attributes
        for cell in row.iter(MAIN + "c"):
            code = formats[int(cell.get("s", "0"))]
            if code != "General":
                formats_by_cell[cell.get("r")] = code
    record["rows"] = {str(k): list(v) for k, v in sorted(rows.items())}
    columns = {}
    for col in root.iter(MAIN + "col"):
        for n in range(int(col.get("min")), min(int(col.get("max")), 16384) + 1):
            width = col.get("width")
            hidden = col.get("hidden") in ("1", "true")
            columns[n] = [float(width) if width else None, hidden]
    # A run of columns past the last stated one is not compared beyond 1000.
    record["columns"] = {str(k): v for k, v in sorted(columns.items()) if k <= 1000}
    record["number_formats"] = dict(sorted(formats_by_cell.items()))
    auto = root.find(MAIN + "autoFilter")
    record["filter"] = auto.get("ref") if auto is not None else None
    related = rels_of(z, part)
    comments, notes, threaded, anchors, tables = [], [], [], [], []
    for kind, target, _ in related:
        if target not in z.namelist():
            continue
        if kind == "comments":
            for c in ET.fromstring(z.read(target)).iter(MAIN + "comment"):
                comments.append(f"{c.get('ref')} {text_of(c)}")
        elif kind == "vmlDrawing":
            vml = z.read(target).decode("utf-8", "replace")
            for shape in re.findall(r"<v:shape .*?</v:shape>", vml, re.S):
                if 'ObjectType="Note"' not in shape:
                    continue
                row = re.search(r"<x:Row>(\d+)</x:Row>", shape)
                column = re.search(r"<x:Column>(\d+)</x:Column>", shape)
                notes.append(f"{row.group(1) if row else '?'},{column.group(1) if column else '?'}")
        elif kind == "threadedComment":
            for c in ET.fromstring(z.read(target)).iter(TC + "threadedComment"):
                threaded.append(c.get("ref"))
        elif kind == "drawing":
            for anchor in ET.fromstring(z.read(target)):
                corners = []
                for corner in ("from", "to"):
                    found = anchor.find(XDR + corner)
                    if found is not None:
                        corners.append(f"{found.findtext(XDR + 'col')},{found.findtext(XDR + 'row')}")
                anchors.append(" ".join(corners))
        elif kind == "table":
            table = ET.fromstring(z.read(target))
            filt = table.find(MAIN + "autoFilter")
            tables.append(f"{table.get('displayName')} {table.get('ref')} "
                          f"{filt.get('ref') if filt is not None else ''}")
    record["comments"] = sorted(comments)
    record["notes"] = sorted(notes)
    record["threaded_comments"] = sorted(threaded)
    record["drawing_anchors"] = sorted(anchors)
    record["tables"] = sorted(tables)
    return record


def workbook_record(path):
    z = zipfile.ZipFile(path)
    workbook = ET.fromstring(z.read("xl/workbook.xml"))
    formats = number_formats(z)
    targets = {rid: target for _, target, rid in rels_of(z, "xl/workbook.xml")}
    sheets = {}
    names = []
    for sheet in workbook.iter(MAIN + "sheet"):
        part = targets[sheet.get(REL + "id")]
        sheets[sheet.get("name")] = sheet_record(z, part, formats)
    for name in workbook.iter(MAIN + "definedName"):
        names.append(f"{name.get('name')} {name.get('localSheetId', '')} {name.text or ''}")
    return {"sheets": sheets, "names": sorted(names)}


def main():
    excel_path, saved_path, out_path = sys.argv[1:4]
    excel, saved = workbook_record(excel_path), workbook_record(saved_path)
    summary = {"excel": excel_path.replace("\\", "/").rsplit("/", 1)[-1],
               "saved": saved_path.replace("\\", "/").rsplit("/", 1)[-1],
               "compared": {}, "differences": []}
    def compare(where, kind, a, b):
        entry = summary["compared"].setdefault(kind, {"items": 0, "equal": 0})
        if isinstance(a, dict):
            keys = sorted(set(a) | set(b))
            entry["items"] += len(keys)
            for key in keys:
                if a.get(key) == b.get(key):
                    entry["equal"] += 1
                elif len(summary["differences"]) < 60:
                    summary["differences"].append(
                        {"where": where, "kind": kind, "key": key,
                         "excel": a.get(key), "saved": b.get(key)})
        else:
            items = sorted(set(a) | set(b))
            entry["items"] += len(items)
            entry["equal"] += len(set(a) & set(b))
            for item in items:
                if (item in a) != (item in b) and len(summary["differences"]) < 60:
                    summary["differences"].append(
                        {"where": where, "kind": kind,
                         "only_in": "excel" if item in a else "saved", "item": item})
    compare("workbook", "defined_names", excel["names"], saved["names"])
    for name, record in excel["sheets"].items():
        ours = saved["sheets"].get(name)
        if ours is None:
            summary["differences"].append({"where": name, "kind": "sheet", "only_in": "excel"})
            continue
        for kind, value in record.items():
            if kind == "filter":
                compare(name, kind, [value] if value else [], [ours[kind]] if ours[kind] else [])
            else:
                compare(name, kind, value, ours[kind])
    summary["all_equal"] = all(v["items"] == v["equal"] for v in summary["compared"].values())
    with open(out_path, "w", encoding="utf-8") as out:
        json.dump(summary, out, indent=2)
    for kind, v in summary["compared"].items():
        print(f"{kind}: {v['equal']} of {v['items']} equal")
    print("all equal" if summary["all_equal"] else f"{len(summary['differences'])} differences listed")


if __name__ == "__main__":
    main()
