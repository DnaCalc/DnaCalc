#!/usr/bin/env python3
"""Writes sparklines.xlsx (own work, dc-zjsg.7): one sheet "Spark" with data in A2:E7 and four sparkline groups in the
worksheet's x14 extension, written the way Excel writes them: a line group (markers, high and low points, an empty cell
shown as a gap) over F2:F3, a column group with negative points and an axis over F4:F5 sharing a group axis, a
win/loss group over F6, a line group spanning empty cells with a custom axis 0..10 over F7, and (dw2) a line group on a
date axis over F8, its values in A8:E8 and its dates in A9:E9 (the group's xm:f). The theme part is the
Office 2013-2022 theme shapes-paint4.xlsx carries (read from it).
Run: python3 -I make-sparklines.py shapes-paint4.xlsx sparklines.xlsx"""
import sys, zipfile

theme = zipfile.ZipFile(sys.argv[1]).read("xl/theme/theme1.xml")
data = {
    2: [3, 5, 2, 8, 6],
    3: [1, None, 4, 2, 5],
    4: [4, -2, 3, -1, 5],
    5: [10, 12, 8, 15, 9],
    6: [1, -1, 1, 0, -1],
    7: [2, None, None, 7, 4],
    8: [1, 3, 2, 5, 4],
    9: [45000, 45001, 45004, 45005, 45008],
}
rows = ['<row r="1"><c r="A1" t="inlineStr"><is><t>Data</t></is></c><c r="F1" t="inlineStr"><is><t>Spark</t></is></c></row>']
for r, values in data.items():
    cells = "".join(f'<c r="{"ABCDE"[i]}{r}"><v>{v}</v></c>' for i, v in enumerate(values) if v is not None)
    rows.append(f'<row r="{r}">{cells}</row>')

def colours(series, extra=""):
    return (f'<x14:colorSeries {series}/><x14:colorNegative theme="5"/><x14:colorAxis rgb="FF000000"/>'
            '<x14:colorMarkers theme="4" tint="-0.499984740745262"/><x14:colorFirst theme="4" tint="0.39997558519241921"/>'
            '<x14:colorLast theme="4" tint="0.39997558519241921"/><x14:colorHigh rgb="FF00B050"/><x14:colorLow rgb="FFFF0000"/>' + extra)

def lines(*pairs):
    return "<x14:sparklines>" + "".join(f"<x14:sparkline><xm:f>Spark!A{r}:E{r}</xm:f><xm:sqref>F{r}</xm:sqref></x14:sparkline>" for r in pairs) + "</x14:sparklines>"

groups = (
    '<x14:sparklineGroup displayEmptyCellsAs="gap" markers="1" high="1" low="1">' + colours('theme="4" tint="-0.499984740745262"') + lines(2, 3) + '</x14:sparklineGroup>'
    '<x14:sparklineGroup type="column" displayEmptyCellsAs="gap" negative="1" displayXAxis="1" minAxisType="group" maxAxisType="group">' + colours('theme="5"') + lines(4, 5) + '</x14:sparklineGroup>'
    '<x14:sparklineGroup type="stacked" displayEmptyCellsAs="gap" negative="1">' + colours('rgb="FF376092"') + lines(6) + '</x14:sparklineGroup>'
    '<x14:sparklineGroup manualMin="0" manualMax="10" lineWeight="2.25" displayEmptyCellsAs="span" minAxisType="custom" maxAxisType="custom">' + colours('theme="9"') + lines(7) + '</x14:sparklineGroup>'
    '<x14:sparklineGroup dateAxis="1" displayEmptyCellsAs="gap">' + colours('theme="4"') + '<xm:f>Spark!A9:E9</xm:f>' + lines(8) + '</x14:sparklineGroup>'
)
R = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
sheet = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" '
         f'xmlns:r="{R}"><sheetFormatPr defaultRowHeight="15"/><sheetData>' + "".join(rows) + '</sheetData>'
         '<pageMargins left="0.7" right="0.7" top="0.75" bottom="0.75" header="0.3" footer="0.3"/>'
         '<extLst><ext uri="{05C60535-1F16-4fd2-B633-F4F36F0B64E0}" xmlns:x14="http://schemas.microsoft.com/office/spreadsheetml/2009/9/main">'
         '<x14:sparklineGroups xmlns:xm="http://schemas.microsoft.com/office/excel/2006/main">' + groups + '</x14:sparklineGroups></ext></extLst></worksheet>')
styles = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="1"><font><sz val="11"/><name val="Calibri"/><family val="2"/><scheme val="minor"/></font></fonts>'
          '<fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills><borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders>'
          '<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/></cellXfs>'
          '<cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles></styleSheet>')
CT = "application/vnd.openxmlformats-officedocument"
files = {
    "[Content_Types].xml": '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>'
        f'<Override PartName="/xl/workbook.xml" ContentType="{CT}.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="{CT}.spreadsheetml.worksheet+xml"/>'
        f'<Override PartName="/xl/theme/theme1.xml" ContentType="{CT}.theme+xml"/><Override PartName="/xl/styles.xml" ContentType="{CT}.spreadsheetml.styles+xml"/></Types>',
    "_rels/.rels": f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{R}/officeDocument" Target="xl/workbook.xml"/></Relationships>',
    "xl/workbook.xml": f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="{R}"><sheets><sheet name="Spark" sheetId="1" r:id="rId1"/></sheets></workbook>',
    "xl/_rels/workbook.xml.rels": f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{R}/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="{R}/theme" Target="theme/theme1.xml"/><Relationship Id="rId3" Type="{R}/styles" Target="styles.xml"/></Relationships>',
    "xl/worksheets/sheet1.xml": sheet,
    "xl/theme/theme1.xml": theme,
    "xl/styles.xml": styles,
}
with zipfile.ZipFile(sys.argv[2], "w", zipfile.ZIP_DEFLATED) as z:
    for name, content in files.items():
        info = zipfile.ZipInfo(name, date_time=(2026, 11, 20, 0, 0, 0))
        info.compress_type = zipfile.ZIP_DEFLATED
        z.writestr(info, content)
