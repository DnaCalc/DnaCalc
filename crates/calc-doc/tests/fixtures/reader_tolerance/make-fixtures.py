#!/usr/bin/env python3
"""Writes the reader-tolerance workbooks: our own small authored packages (no third-party content).

Each defect has a control that differs from it in exactly the defect (dc-hc85 follow-up):
  dangling-rid.xlsx          the workbook part's second <sheet> has r:id="rId9", which the workbook
                             part's relationships do not define (the control has rId2)
  empty-rid-veryhidden.xlsx  the shape of 687 Enron corpus sheets (VBA modules saved by Excel): a veryHidden sheet
                             with r:id="" and no part or relationship; empty-rid-visible.xlsx is the same sheet
                             visible; empty-rid-control.xlsx has the veryHidden sheet with a real r:id
  duplicate-sheetid-gap.xlsx the Enron shape of it: three sheets with the ids 1, 109 and 109
  duplicate-sheetid-lower.xlsx  ids 1, 1 and 5: shows whether the repaired id is the next free one or the maximum plus one
  duplicate-sheetid-triple.xlsx, duplicate-sheetid-two-pairs.xlsx  ids 1, 1, 1 and 2, 2, 3, 3: several repairs in one workbook
  duplicate-sheetid.xlsx     two sheets both have sheetId="1" (the control has 1 and 2)
  array-inner-formula.xlsx   A2 holds a plain formula inside the area of the array formula A1:A3
                             (the control gives A2 only a cached value);
                             array-inner-empty-f.xlsx gives A2 the empty <f ca="1"/> that Excel itself writes
                             inside a multi-cell array's area in 7 Enron files;
                             array-inner-formula-stale.xlsx is array-inner-formula.xlsx with a wrong cached value in A3,
                             to show whether Excel recalculates the repaired array
  watcher-control-malformed-sheet.xlsx   a worksheet cut off inside a row: a file Excel is known to
                             answer with a repair prompt, to show the dialog watcher works
Run: python3 make-fixtures.py <output directory>
"""
import os, sys, zipfile

M = "http://schemas.openxmlformats.org/spreadsheetml/2006/main"
R = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
PR = "http://schemas.openxmlformats.org/package/2006/relationships"
D = R + "/"
HEAD = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'


def package(sheets, workbook_rids, sheet_ids, sheet_xml, states=None, no_part=()):
    """sheets: names; workbook_rids/sheet_ids: per sheet; sheet_xml: per sheet worksheet XML."""
    parts = {}
    states = states or [None] * len(sheets)
    overrides = "".join(
        f'<Override PartName="/xl/worksheets/sheet{i + 1}.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>'
        for i in range(len(sheets)) if i not in no_part)
    parts["[Content_Types].xml"] = (
        HEAD + '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
        '<Default Extension="xml" ContentType="application/xml"/>'
        '<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>'
        + overrides + "</Types>")
    parts["_rels/.rels"] = (HEAD + f'<Relationships xmlns="{PR}"><Relationship Id="rId1" Type="{D}officeDocument" Target="xl/workbook.xml"/></Relationships>')
    decl = "".join(
        f'<sheet name="{n}" sheetId="{sid}"' + (f' state="{st}"' if st else "") + f' r:id="{rid}"/>'
        for n, sid, rid, st in zip(sheets, sheet_ids, workbook_rids, states))
    parts["xl/workbook.xml"] = (HEAD + f'<workbook xmlns="{M}" xmlns:r="{R}"><bookViews><workbookView activeTab="0"/></bookViews><sheets>{decl}</sheets><calcPr calcId="191029"/></workbook>')
    rels = "".join(f'<Relationship Id="rId{i + 1}" Type="{D}worksheet" Target="worksheets/sheet{i + 1}.xml"/>' for i in range(len(sheets)) if i not in no_part)
    parts["xl/_rels/workbook.xml.rels"] = HEAD + f'<Relationships xmlns="{PR}">{rels}</Relationships>'
    for i, xml in enumerate(sheet_xml):
        if i in no_part:
            continue
        parts[f"xl/worksheets/sheet{i + 1}.xml"] = HEAD + f'<worksheet xmlns="{M}"><sheetData>{xml}</sheetData></worksheet>'
    return parts


def write(path, parts):
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
        for name, text in parts.items():
            info = zipfile.ZipInfo(name, date_time=(2026, 11, 11, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            z.writestr(info, text.encode("utf-8"))


ONE = '<row r="1"><c r="A1"><v>2</v></c><c r="B1"><v>3</v></c></row>'
TWO = '<row r="1"><c r="A1"><v>5</v></c><c r="B1"><f>A1*2</f><v>10</v></c></row>'


def two_sheets(rids, ids):
    return package(["One", "Two"], rids, ids, [ONE, TWO])


def hidden_second(rid, state, with_part):
    """Excel's own shape of an empty r:id: the second sheet (a VBA module in the corpus files) has r:id="" and no part."""
    return package(["One", "Two"], ["rId1", rid], [1, 2], [ONE, TWO], states=[None, state], no_part=() if with_part else (1,))


def array_sheet(inner, a3="6"):
    rows = ('<row r="1"><c r="A1"><f t="array" ref="A1:A3">B1:B3*2</f><v>2</v></c><c r="B1"><v>1</v></c></row>'
            f'<row r="2">{inner}<c r="B2"><v>2</v></c></row>'
            f'<row r="3"><c r="A3"><v>{a3}</v></c><c r="B3"><v>3</v></c></row>')
    return package(["Arr"], ["rId1"], [1], [rows])


def malformed():
    """A positive control for the dialog watcher of open-in-excel-job.ps1: a worksheet cut off inside a row."""
    parts = two_sheets(["rId1", "rId2"], [1, 2])
    parts["xl/worksheets/sheet2.xml"] = parts["xl/worksheets/sheet2.xml"].replace("<v>10</v></c></row>", "<v>10</v></c>")
    return parts


out = sys.argv[1]
os.makedirs(out, exist_ok=True)
write(f"{out}/dangling-rid-control.xlsx", two_sheets(["rId1", "rId2"], [1, 2]))
write(f"{out}/dangling-rid.xlsx", two_sheets(["rId1", "rId9"], [1, 2]))
write(f"{out}/empty-rid-control.xlsx", hidden_second("rId2", "veryHidden", True))
write(f"{out}/empty-rid-veryhidden.xlsx", hidden_second("", "veryHidden", False))
write(f"{out}/empty-rid-visible.xlsx", hidden_second("", None, False))
write(f"{out}/duplicate-sheetid-gap.xlsx", package(["One", "Two", "Three"], ["rId1", "rId2", "rId3"], [1, 109, 109], [ONE, TWO, ONE]))
write(f"{out}/duplicate-sheetid-lower.xlsx", package(["One", "Two", "Three"], ["rId1", "rId2", "rId3"], [1, 1, 5], [ONE, TWO, ONE]))
write(f"{out}/duplicate-sheetid-triple.xlsx", package(["One", "Two", "Three"], ["rId1", "rId2", "rId3"], [1, 1, 1], [ONE, TWO, ONE]))
write(f"{out}/duplicate-sheetid-two-pairs.xlsx", package(["One", "Two", "Three", "Four"], ["rId1", "rId2", "rId3", "rId4"], [2, 2, 3, 3], [ONE, TWO, ONE, TWO]))
write(f"{out}/duplicate-sheetid-control.xlsx", two_sheets(["rId1", "rId2"], [1, 2]))
write(f"{out}/duplicate-sheetid.xlsx", two_sheets(["rId1", "rId2"], [1, 1]))
write(f"{out}/watcher-control-malformed-sheet.xlsx", malformed())
write(f"{out}/array-inner-formula-control.xlsx", array_sheet('<c r="A2"><v>4</v></c>'))
write(f"{out}/array-inner-empty-f.xlsx", array_sheet('<c r="A2"><f ca="1"/><v>4</v></c>'))
write(f"{out}/array-inner-formula-stale.xlsx", array_sheet('<c r="A2"><f>B2+100</f><v>102</v></c>', a3="999"))
write(f"{out}/array-inner-formula.xlsx", array_sheet('<c r="A2"><f>B2+100</f><v>102</v></c>'))
