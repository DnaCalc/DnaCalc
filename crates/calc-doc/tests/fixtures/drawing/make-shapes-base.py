#!/usr/bin/env python3
"""Writes shapes-base.xlsx (own work, dc-zjsg.2): one sheet with a labelled grid and no drawing, whose
sizes the shapes of shapes-paint.xlsx are anchored over: column A 103 px wide, column D hidden, column E
145 px, row 3 40 px, row 6 hidden, row 10 60 px, all else the default (64 px by 20 px).
shapes-paint.xlsx is this file with its shapes added by calc-doc's own save route (drawing_layer.rs,
dc_zjsg_2__shapes_paint_workbook).
Run: python3 -I make-shapes-base.py shapes-base.xlsx"""
import sys, zipfile

rows = []
for r in range(1, 13):
    cells = "".join(f'<c r="{c}{r}" t="inlineStr"><is><t>{c}{r}</t></is></c>' for c in "ABCDEFGHIJ")
    attrs = ""
    if r == 3: attrs = ' ht="30" customHeight="1"'
    if r == 6: attrs = ' hidden="1"'
    if r == 10: attrs = ' ht="45" customHeight="1"'
    rows.append(f'<row r="{r}"{attrs}>{cells}</row>')
sheet = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
 '<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">'
 '<sheetFormatPr defaultRowHeight="15"/>'
 '<cols><col min="1" max="1" width="14" customWidth="1"/><col min="4" max="4" width="9" hidden="1" customWidth="1"/>'
 '<col min="5" max="5" width="20" customWidth="1"/></cols>'
 '<sheetData>' + "".join(rows) + '</sheetData></worksheet>')
files = {
 "[Content_Types].xml": '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>',
 "_rels/.rels": '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>',
 "xl/workbook.xml": '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Shapes" sheetId="1" r:id="rId1"/></sheets></workbook>',
 "xl/_rels/workbook.xml.rels": '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>',
 "xl/worksheets/sheet1.xml": sheet,
}
with zipfile.ZipFile(sys.argv[1], "w", zipfile.ZIP_DEFLATED) as z:
    for name, text in files.items():
        z.writestr(zipfile.ZipInfo(name, (2026, 11, 15, 0, 0, 0)), text.encode(), zipfile.ZIP_DEFLATED)
