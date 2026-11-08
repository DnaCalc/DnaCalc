"""Fixture writer for pivot-job.ps1 results.

  py -3 tools/excel-session/pivot-fixtures.py cells RESULT.json STAGE OUT.tsv [--sheets P1,P2,...]
  py -3 tools/excel-session/pivot-fixtures.py formats RESULT.json STAGE OUT.tsv
  py -3 tools/excel-session/pivot-fixtures.py stages RESULT.json OUT.tsv [--prefix P] [--formats]
  py -3 tools/excel-session/pivot-fixtures.py gpd RESULT.json OUT.tsv      (m3: id, formula, kind, value, text; entry refusals)
  py -3 tools/excel-session/pivot-fixtures.py cube RESULT.json OUT.tsv     (m4: every stage of CUBE rows, converted and model pivot cells)
  py -3 tools/excel-session/pivot-fixtures.py compare A.tsv B.tsv

`formats` and `--formats` add: number_format, text, font_name, font_size, bold, italic, font_color,
fill, pattern, borders (left|top|bottom|right as style:weight:color), halign, indent, column_width, row_height.
`stages` writes the same lines for every stage that holds one pivot, keyed by the stage name instead
of the sheet (the m1/m2 job names each case); `--prefix` keeps only stages starting with P.

`cells` writes one line per occupied pivot output cell: sheet, address, kind (s string, n number,
b boolean, e error), value. Sheets default to P1..P<n> in pivot order and then PName, PTable, the
names pivot-job.ps1 gives them. `formats` adds number format, text, bold, fill and column width
(the formatting set). Both read only what Excel returned; nothing is calculated here.
"""
import json
import sys


def load(path):
    with open(path, encoding="utf-8-sig") as handle:
        return json.load(handle)


def stage(result, name):
    for item in result["observations"]:
        if item.get("stage") == name:
            return item
    raise SystemExit(f"stage {name!r} not found in result")


def sheet_names(count, names=None):
    if names:
        return names
    plain = [f"P{n}" for n in range(1, count + 1)]
    return plain


def cell_value(value):
    kind = value.get("type")
    if kind == "empty":
        return None
    if kind == "double":
        number = value["value"]
        return ("n", repr(float(number)).removesuffix(".0") if float(number).is_integer() else repr(float(number)))
    if kind == "String":
        return ("s", value["value"])
    if kind == "bool":
        return ("b", "TRUE" if value["value"] else "FALSE")
    if kind == "error-or-int":
        return ("e", str(value["value"]))
    if kind == "date":
        return ("n", repr(float(value["value"])))
    return (kind, str(value.get("value")))


def format_columns(cell):
    borders = cell.get("borders") or {}
    edges = "|".join(str(borders.get(side, "")) for side in ("left", "top", "bottom", "right"))
    fill = cell.get("fill", cell.get("interior_color", ""))
    return [str(cell.get("number_format", "")), str(cell.get("text", "")), str(cell.get("font_name", "")),
            str(cell.get("font_size", "")), str(cell.get("bold", "")), str(cell.get("italic", "")),
            str(cell.get("font_color", "")), str(fill), str(cell.get("pattern", "")), edges,
            str(cell.get("halign", "")), str(cell.get("indent", "")), str(cell.get("column_width", "")),
            str(cell.get("row_height", ""))]


def pivot_sheets(pivots, override):
    if override:
        return override
    count = len(pivots)
    # pivot-job.ps1 creates sheets P1..Pn for the specification list, then PName and PTable.
    named = [p["name"] for p in pivots]
    tail = [s for s, n in (("PName", "pvt_named"), ("PTable", "pvt_table")) if n in named]
    return [f"P{n}" for n in range(1, count - len(tail) + 1)] + tail


def write_cells(result_path, stage_name, out, sheets, formats):
    pivots = stage(load(result_path), stage_name)["pivots"]
    names = pivot_sheets(pivots, sheets)
    if not sheets and all(p.get("sheet") for p in pivots):
        names = [p["sheet"] for p in pivots]
    lines = []
    for sheet, pivot in zip(names, pivots):
        cells = pivot.get("cells")
        if not isinstance(cells, list):
            continue
        for cell in cells:
            decoded = cell_value(cell["value"])
            if decoded is None:
                continue
            kind, text = decoded
            row = [sheet, cell["address"], kind, text]
            if formats:
                row += format_columns(cell)
            lines.append("\t".join(row))
    with open(out, "w", encoding="utf-8", newline="\n") as handle:
        handle.write("\n".join(lines) + "\n")
    print(f"{out}: {len(lines)} cells")


def write_stages(result_path, out, prefix, formats):
    lines = []
    count = 0
    for item in load(result_path)["observations"]:
        if "stage" not in item or not item["stage"].startswith(prefix):
            continue
        pivots = item.get("pivots") or []
        if len(pivots) != 1 or not isinstance(pivots[0].get("cells"), list):
            continue
        count += 1
        for cell in pivots[0]["cells"]:
            decoded = cell_value(cell["value"])
            if decoded is None:
                continue
            row = [item["stage"], cell["address"], *decoded]
            if formats:
                row += format_columns(cell)
            lines.append("\t".join(row))
    with open(out, "w", encoding="utf-8", newline="\n") as handle:
        handle.write("\n".join(lines) + "\n")
    print(f"{out}: {count} stages, {len(lines)} cells")


def write_gpd(result_path, out):
    result = load(result_path)
    refused = {e["step"].removeprefix("entry-"): e["message"] for e in result["errors"] if e["step"].startswith("entry-")}
    lines = []
    for item in result["observations"]:
        if item.get("stage") == "gpd-values":
            for v in item["values"]:
                kind, text = cell_value(v["value"]) or ("empty", "")
                formula = v["formula"] if v["has_formula"] else ""
                lines.append("\t".join([v["id"], formula, kind, text, v["text"], refused.get(v["id"], "")]))
    with open(out, "w", encoding="utf-8", newline="\n") as handle:
        handle.write("\n".join(lines) + "\n")
    print(f"{out}: {len(lines)} formulas, {sum(1 for l in lines if l.split(chr(9))[5])} refused at entry")


def write_cube(result_path, out):
    result = load(result_path)
    lines = []
    for item in result["observations"]:
        stage_name = item.get("stage", "")
        for v in item.get("cube", []):
            kind, text = cell_value(v["value"]) or ("empty", "")
            lines.append("\t".join([stage_name, f"A{v['row']}", kind, text, v["text"], v["formula"]]))
        for v in item.get("values", []):
            if stage_name == "gpd-model":
                kind, text = cell_value(v["value"]) or ("empty", "")
                lines.append("\t".join([stage_name, f"B{v['row']}", kind, text, v["text"], v["formula"]]))
        for key in ("cells",):
            if stage_name == "model-pivot":
                for cell in item.get(key, []):
                    decoded = cell_value(cell["value"])
                    if decoded:
                        lines.append("\t".join([stage_name, cell["address"], *decoded, cell["text"], ""]))
        for v in item.get("formulas", []):
            kind, text = cell_value(v["value"]) or ("empty", "")
            lines.append("\t".join([stage_name, v["address"], kind, text, v["text"], v["formula"]]))
    with open(out, "w", encoding="utf-8", newline="\n") as handle:
        handle.write("\n".join(lines) + "\n")
    print(f"{out}: {len(lines)} lines")


def main(argv):
    if argv and argv[0] in ("cells", "formats") and len(argv) >= 4:
        sheets = None
        if "--sheets" in argv:
            sheets = argv[argv.index("--sheets") + 1].split(",")
        write_cells(argv[1], argv[2], argv[3], sheets, argv[0] == "formats")
    elif argv and argv[0] == "gpd" and len(argv) == 3:
        write_gpd(argv[1], argv[2])
    elif argv and argv[0] == "cube" and len(argv) == 3:
        write_cube(argv[1], argv[2])
    elif argv and argv[0] == "stages" and len(argv) >= 3:
        prefix = argv[argv.index("--prefix") + 1] if "--prefix" in argv else ""
        write_stages(argv[1], argv[2], prefix, "--formats" in argv)
    elif argv and argv[0] == "compare" and len(argv) == 3:
        a = open(argv[1], encoding="utf-8").read().splitlines()
        b = open(argv[2], encoding="utf-8").read().splitlines()
        if a == b:
            print(f"identical: {len(a)} lines")
            return 0
        sa, sb = set(a), set(b)
        print(f"DIFFERENT: {len(a)} vs {len(b)} lines; only in first {len(sa - sb)}, only in second {len(sb - sa)}")
        for line in sorted(sa - sb)[:10]:
            print("<", line)
        for line in sorted(sb - sa)[:10]:
            print(">", line)
        return 1
    else:
        print(__doc__)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
