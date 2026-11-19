#!/usr/bin/env python3
"""Writes shapes-paint4.xlsx (own work, dc-zjsg.2, fourth slice): one sheet with a labelled grid and a drawing
part written the way Excel writes one (every shape with its xdr:style), holding what the fourth slice draws:
custom geometries (closed, open with an arrowhead, arcs from guides, quadratic and cubic curves), pattern
fills, style references in colours other than accent 1, and rectangles turned 30, 60, 120 and 300 degrees
over the same anchor size (Excel's anchor convention for a turned shape). The workbook carries the Office
2013-2022 theme part and a styles part, so Excel and DNA Calc draw it with the same theme.
Run: python3 -I make-shapes-paint4.py shapes-paint4.xlsx"""
import sys, zipfile

EMU = 9525  # per pixel at 96 dpi
COL, ROW = 64 * EMU, 20 * EMU

rows = []
for r in range(1, 41):
    cells = "".join(f'<c r="{c}{r}" t="inlineStr"><is><t>{c}{r}</t></is></c>' for c in "AB") if r <= 2 else ""
    rows.append(f'<row r="{r}">{cells}</row>')
sheet = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
 '<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" '
 'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">'
 '<sheetFormatPr defaultRowHeight="15"/><sheetData>' + "".join(rows) + '</sheetData>'
 '<drawing r:id="rId1"/></worksheet>')

STYLE_SHAPE = ('<xdr:style><a:lnRef idx="2"><a:schemeClr val="{c}"><a:shade val="50000"/></a:schemeClr></a:lnRef>'
 '<a:fillRef idx="1"><a:schemeClr val="{c}"/></a:fillRef><a:effectRef idx="0"><a:schemeClr val="{c}"/></a:effectRef>'
 '<a:fontRef idx="minor"><a:schemeClr val="lt1"/></a:fontRef></xdr:style>')
STYLE_LINE = ('<xdr:style><a:lnRef idx="1"><a:schemeClr val="accent1"/></a:lnRef><a:fillRef idx="0"><a:schemeClr val="accent1"/></a:fillRef>'
 '<a:effectRef idx="0"><a:schemeClr val="accent1"/></a:effectRef><a:fontRef idx="minor"><a:schemeClr val="tx1"/></a:fontRef></xdr:style>')

anchors = []
next_id = [2]

def anchor(col, row, cols, rows_, body):
    """A two-cell anchor from (col, row) spanning cols columns and rows_ rows (0-based, whole cells)."""
    anchors.append(f'<xdr:twoCellAnchor editAs="oneCell"><xdr:from><xdr:col>{col}</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>{row}</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from>'
                   f'<xdr:to><xdr:col>{col + cols}</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>{row + rows_}</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to>{body}<xdr:clientData/></xdr:twoCellAnchor>')

def sp(name, cols, rows_, geom, props="", style=None, text="", rot=0):
    i = next_id[0]; next_id[0] += 1
    rot_attr = f' rot="{rot}"' if rot else ""
    style = STYLE_SHAPE.format(c="accent1") if style is None else style
    body = ""
    if text:
        body = ('<xdr:txBody><a:bodyPr vertOverflow="clip" horzOverflow="clip" rtlCol="0" anchor="ctr"/><a:lstStyle/>'
                f'<a:p><a:pPr algn="ctr"/><a:r><a:rPr lang="en-US" sz="1100"/><a:t>{text}</a:t></a:r></a:p></xdr:txBody>')
    return (f'<xdr:sp macro="" textlink=""><xdr:nvSpPr><xdr:cNvPr id="{i}" name="{name}"/><xdr:cNvSpPr/></xdr:nvSpPr>'
            f'<xdr:spPr><a:xfrm{rot_attr}><a:off x="0" y="0"/><a:ext cx="{cols * COL}" cy="{rows_ * ROW}"/></a:xfrm>{geom}{props}</xdr:spPr>'
            f'{style}{body}</xdr:sp>')

def cust(paths, gd="", rect='<a:rect l="l" t="t" r="r" b="b"/>'):
    return f'<a:custGeom><a:avLst/><a:gdLst>{gd}</a:gdLst><a:ahLst/><a:cxnLst/>{rect}<a:pathLst>{paths}</a:pathLst></a:custGeom>'

def pts(*p):
    return "".join(f'<a:pt x="{x}" y="{y}"/>' for x, y in p)

RECT = '<a:prstGeom prst="rect"><a:avLst/></a:prstGeom>'

# Row 1 (rows 4-9): custom geometries.
anchor(1, 3, 2, 5, sp("Freeform triangle", 2, 5, cust(
    f'<a:path w="1000" h="1000"><a:moveTo>{pts((500, 0))}</a:moveTo><a:lnTo>{pts((1000, 1000))}</a:lnTo><a:lnTo>{pts((0, 1000))}</a:lnTo><a:close/></a:path>'),
    '<a:solidFill><a:srgbClr val="C00000"/></a:solidFill>', text="Tri"))
anchor(4, 3, 2, 5, sp("Freeform open arrow", 2, 5, cust(
    f'<a:path w="100" h="100" fill="none"><a:moveTo>{pts((0, 100))}</a:moveTo><a:lnTo>{pts((30, 20))}</a:lnTo><a:lnTo>{pts((70, 80))}</a:lnTo><a:lnTo>{pts((100, 0))}</a:lnTo></a:path>'),
    '<a:noFill/><a:ln w="28575"><a:solidFill><a:srgbClr val="7030A0"/></a:solidFill><a:tailEnd type="triangle"/></a:ln>', style=STYLE_LINE))
anchor(7, 3, 2, 5, sp("Freeform half disc from guides", 2, 5, cust(
    '<a:path><a:moveTo><a:pt x="r" y="vc"/></a:moveTo><a:arcTo wR="hc" hR="vc" stAng="0" swAng="cd2"/><a:close/></a:path>',
    gd='<a:gd name="unused" fmla="*/ w 1 2"/>'),
    '<a:solidFill><a:srgbClr val="00B050"/></a:solidFill>'))
anchor(10, 3, 2, 5, sp("Freeform curves", 2, 5, cust(
    f'<a:path w="200" h="200"><a:moveTo>{pts((0, 200))}</a:moveTo><a:quadBezTo>{pts((100, -100), (200, 200))}</a:quadBezTo>'
    f'<a:cubicBezTo>{pts((150, 120), (50, 120), (0, 200))}</a:cubicBezTo><a:close/></a:path>'),
    '<a:solidFill><a:srgbClr val="FFC000"/></a:solidFill>'))
anchor(13, 3, 2, 5, sp("Freeform two paths, one unfilled", 2, 5, cust(
    f'<a:path w="100" h="100"><a:moveTo>{pts((0, 0))}</a:moveTo><a:lnTo>{pts((100, 0))}</a:lnTo><a:lnTo>{pts((100, 100))}</a:lnTo><a:lnTo>{pts((0, 100))}</a:lnTo><a:close/></a:path>'
    f'<a:path w="100" h="100" fill="none"><a:moveTo>{pts((0, 0))}</a:moveTo><a:lnTo>{pts((100, 100))}</a:lnTo></a:path>'),
    '<a:solidFill><a:srgbClr val="9DC3E6"/></a:solidFill><a:ln w="19050"><a:solidFill><a:srgbClr val="000000"/></a:solidFill></a:ln>'))

# Row 2 (rows 11-15): pattern fills.
patterns = [("pct50", "000000", "FFFFFF"), ("ltHorz", "4472C4", "FFFFFF"), ("dkUpDiag", "C00000", "FFFF00"),
            ("smCheck", "000000", "D9D9D9"), ("lgGrid", "00B050", "FFFFFF"), ("diagCross", "7030A0", "FFFFFF"),
            ("pct20", "000000", "FFFFFF"), ("horzBrick", "843C0C", "F8CBAD")]
for i, (prst, fg, bg) in enumerate(patterns):
    anchor(1 + i * 2, 10, 2, 4, sp(f"Pattern {prst}", 2, 4, RECT,
        f'<a:pattFill prst="{prst}"><a:fgClr><a:srgbClr val="{fg}"/></a:fgClr><a:bgClr><a:srgbClr val="{bg}"/></a:bgClr></a:pattFill>'))

# Row 3 (rows 17-21): style references in other colours.
anchor(1, 16, 3, 4, sp("Style accent2", 3, 4, RECT, style=STYLE_SHAPE.format(c="accent2"), text="Accent 2"))
anchor(5, 16, 3, 4, sp("Style accent6 line only", 3, 4, RECT, style=(
    '<xdr:style><a:lnRef idx="3"><a:schemeClr val="accent6"/></a:lnRef><a:fillRef idx="0"><a:schemeClr val="accent6"/></a:fillRef>'
    '<a:effectRef idx="0"><a:schemeClr val="accent6"/></a:effectRef><a:fontRef idx="minor"><a:schemeClr val="dk1"/></a:fontRef></xdr:style>'),
    text="Line 3"))
anchor(9, 16, 3, 4, sp("Style accent4 dark text", 3, 4, RECT, style=(
    '<xdr:style><a:lnRef idx="1"><a:schemeClr val="accent4"/></a:lnRef><a:fillRef idx="1"><a:schemeClr val="accent4"><a:lumMod val="40000"/><a:lumOff val="60000"/></a:schemeClr></a:fillRef>'
    '<a:effectRef idx="0"><a:schemeClr val="accent4"/></a:effectRef><a:fontRef idx="minor"><a:schemeClr val="tx1"/></a:fontRef></xdr:style>'),
    text="Accent 4 light"))

# Row 4 (rows 24-33): rectangles 3 columns by 3 rows (192 by 60 px), turned.
for i, deg in enumerate([30, 60, 120, 300]):
    anchor(1 + i * 4, 25, 3, 3, sp(f"Rect rot {deg}", 3, 3, RECT, rot=deg * 60000, text=f"{deg} deg"))

drawing = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
 '<xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" '
 'xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">' + "".join(anchors) + '</xdr:wsDr>')

def scheme(name, val):
    return f'<a:{name}><a:srgbClr val="{val}"/></a:{name}>'

theme = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
 '<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="Office Theme"><a:themeElements>'
 '<a:clrScheme name="Office"><a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1><a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1>'
 + scheme("dk2", "44546A") + scheme("lt2", "E7E6E6") + scheme("accent1", "4472C4") + scheme("accent2", "ED7D31")
 + scheme("accent3", "A5A5A5") + scheme("accent4", "FFC000") + scheme("accent5", "5B9BD5") + scheme("accent6", "70AD47")
 + scheme("hlink", "0563C1") + scheme("folHlink", "954F72") + '</a:clrScheme>'
 '<a:fontScheme name="Office"><a:majorFont><a:latin typeface="Calibri Light"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont>'
 '<a:minorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont></a:fontScheme>'
 '<a:fmtScheme name="Office"><a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill>'
 '<a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:fillStyleLst>'
 '<a:lnStyleLst>'
 + "".join(f'<a:ln w="{w}" cap="flat" cmpd="sng" algn="ctr"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:prstDash val="solid"/><a:miter lim="800000"/></a:ln>' for w in (6350, 12700, 19050))
 + '</a:lnStyleLst><a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle>'
 '<a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst><a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill>'
 '<a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:bgFillStyleLst></a:fmtScheme>'
 '</a:themeElements></a:theme>')

styles = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
 '<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="1"><font><sz val="11"/><name val="Calibri"/><family val="2"/><scheme val="minor"/></font></fonts>'
 '<fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills>'
 '<borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders>'
 '<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs>'
 '<cellXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/></cellXfs>'
 '<cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles></styleSheet>')

REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
files = {
 "[Content_Types].xml": '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>'
   '<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>'
   '<Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>'
   '<Override PartName="/xl/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/>'
   '<Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>'
   '<Override PartName="/xl/drawings/drawing1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawing+xml"/></Types>',
 "_rels/.rels": f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/officeDocument" Target="xl/workbook.xml"/></Relationships>',
 "xl/workbook.xml": f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="{REL}"><sheets><sheet name="Shapes" sheetId="1" r:id="rId1"/></sheets></workbook>',
 "xl/_rels/workbook.xml.rels": f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="{REL}/theme" Target="theme/theme1.xml"/><Relationship Id="rId3" Type="{REL}/styles" Target="styles.xml"/></Relationships>',
 "xl/worksheets/sheet1.xml": sheet,
 "xl/worksheets/_rels/sheet1.xml.rels": f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/drawing" Target="../drawings/drawing1.xml"/></Relationships>',
 "xl/drawings/drawing1.xml": drawing,
 "xl/theme/theme1.xml": theme,
 "xl/styles.xml": styles,
}
with zipfile.ZipFile(sys.argv[1], "w", zipfile.ZIP_DEFLATED) as z:
    for name, data in files.items():
        info = zipfile.ZipInfo(name, date_time=(2026, 11, 19, 0, 0, 0))
        info.compress_type = zipfile.ZIP_DEFLATED
        z.writestr(info, data)
