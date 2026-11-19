#!/usr/bin/env python3
"""Writes form-controls.xlsx (own work, dc-zjsg.8): one sheet "Controls" holding the nine legacy form controls the way
Excel 2010 and later stores them: the worksheet's `controls` (each `control` with its `controlPr` anchor, in an
mc:AlternateContent whose Fallback has no anchor), one ctrlProp part each (`formControlPr` with the control's type,
linked cell, input range, value, limits), a hidden DrawingML shape each in the drawing part (`a14:compatExt`, the
caption as its text), and the legacy VML shapes. Values: B2 TRUE (the check box), B3 2 (the second option button), B4 3
(the list box), B5 30 (the scroll bar), B6 5 (the spinner), B7 2 (the combo box); D2:D5 the list items.
Run: python3 -I make-form-controls.py form-controls.xlsx"""
import sys, zipfile

R = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
EMU = 9525
# name, objectType, (col, row) from, (col, row) to, ctrlProp attributes, caption
controls = [
    ("Button 1", "Button", (6, 1), (8, 3), '', "Run"),
    ("Check Box 2", "CheckBox", (6, 4), (8, 5), 'checked="Checked" fmlaLink="$B$2" lockText="1" noThreeD="1"', "Include tax"),
    ("Option Button 3", "Radio", (6, 6), (8, 7), 'fmlaLink="$B$3" lockText="1" noThreeD="1"', "Monthly"),
    ("Option Button 4", "Radio", (6, 7), (8, 8), 'checked="Checked" lockText="1" noThreeD="1"', "Yearly"),
    ("List Box 5", "List", (9, 1), (11, 6), 'fmlaLink="$B$4" fmlaRange="$D$2:$D$5" noThreeD="1" sel="3" val="0"', ""),
    ("Scroll Bar 6", "Scroll", (9, 7), (12, 8), 'fmlaLink="$B$5" horiz="1" max="100" page="10" val="30"', ""),
    ("Spinner 7", "Spin", (12, 1), (13, 3), 'fmlaLink="$B$6" max="10" min="1" val="5"', ""),
    ("Drop Down 8", "Drop", (12, 4), (14, 5), 'dropLines="4" dropStyle="combo" fmlaLink="$B$7" fmlaRange="$D$2:$D$5" noThreeD="1" sel="2" val="0"', ""),
    ("Group Box 9", "GBox", (5, 10), (9, 15), 'noThreeD="1"', "Options"),
    ("Label 10", "Label", (10, 10), (12, 11), '', "A label"),
]
cells = {2: ("B", "1", "b"), 3: ("B", "2", None), 4: ("B", "3", None), 5: ("B", "30", None), 6: ("B", "5", None), 7: ("B", "2", None)}
items = {2: "North", 3: "South", 4: "East", 5: "West"}
rows = []
for r in range(1, 8):
    cs = ""
    if r in cells:
        col, v, t = cells[r]
        cs += f'<c r="{col}{r}" t="b"><v>{v}</v></c>' if t else f'<c r="{col}{r}"><v>{v}</v></c>'
    if r in items:
        cs += f'<c r="D{r}" t="inlineStr"><is><t>{items[r]}</t></is></c>'
    rows.append(f'<row r="{r}">{cs}</row>')

def point(col, row):
    return f"<xdr:col>{col}</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>{row}</xdr:row><xdr:rowOff>0</xdr:rowOff>"

sheet_controls = []
drawing = []
vml = []
props = {}
rels = [f'<Relationship Id="rId1" Type="{R}/drawing" Target="../drawings/drawing1.xml"/>',
        f'<Relationship Id="rId2" Type="{R}/vmlDrawing" Target="../drawings/vmlDrawing1.vml"/>']
for i, (name, kind, f, t, attrs, caption) in enumerate(controls):
    shape_id = 1025 + i
    rid = f"rId{3 + i}"
    rels.append(f'<Relationship Id="{rid}" Type="{R}/ctrlProp" Target="../ctrlProps/ctrlProp{i + 1}.xml"/>')
    props[f"xl/ctrlProps/ctrlProp{i + 1}.xml"] = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
        f'<formControlPr xmlns="http://schemas.microsoft.com/office/spreadsheetml/2009/9/main" objectType="{kind}" {attrs}/>')
    macro = ' macro="[0]!Button1_Click"' if kind == "Button" else ""
    sheet_controls.append(
        f'<mc:AlternateContent xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"><mc:Choice Requires="x14">'
        f'<control shapeId="{shape_id}" r:id="{rid}" name="{name}"><controlPr defaultSize="0"{macro} autoFill="0" autoLine="0" autoPict="0">'
        f'<anchor moveWithCells="1"><from>{point(*f)}</from><to>{point(*t)}</to></anchor></controlPr></control></mc:Choice>'
        f'<mc:Fallback><control shapeId="{shape_id}" r:id="{rid}" name="{name}"/></mc:Fallback></mc:AlternateContent>')
    body = (f'<xdr:txBody><a:bodyPr vertOverflow="clip" wrap="square" lIns="18288" tIns="18288" rIns="0" bIns="18288" anchor="ctr" upright="1"/><a:lstStyle/>'
            f'<a:p><a:pPr algn="l" rtl="0"><a:defRPr sz="1000"/></a:pPr><a:r><a:rPr lang="en-US" sz="800" b="0" i="0" u="none" strike="noStrike" baseline="0">'
            f'<a:solidFill><a:srgbClr val="000000"/></a:solidFill><a:latin typeface="Segoe UI"/></a:rPr><a:t>{caption}</a:t></a:r></a:p></xdr:txBody>') if caption else ""
    drawing.append(
        '<mc:AlternateContent xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"><mc:Choice xmlns:a14="http://schemas.microsoft.com/office/drawing/2010/main" Requires="a14">'
        f'<xdr:twoCellAnchor editAs="oneCell"><xdr:from>{point(*f)}</xdr:from><xdr:to>{point(*t)}</xdr:to><xdr:sp macro="" textlink="">'
        f'<xdr:nvSpPr><xdr:cNvPr id="{shape_id}" name="{name}" hidden="1"><a:extLst><a:ext uri="{{63B3BB69-23CF-44E3-9099-C40C66FF867C}}"><a14:compatExt spid="_x0000_s{shape_id}"/></a:ext>'
        '</a:extLst></xdr:cNvPr><xdr:cNvSpPr/></xdr:nvSpPr><xdr:spPr bwMode="auto"><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom>'
        f'<a:noFill/><a:ln><a:noFill/></a:ln></xdr:spPr>{body}</xdr:sp><xdr:clientData/></xdr:twoCellAnchor></mc:Choice></mc:AlternateContent>')
    vml_kind = {"Button": "Button", "CheckBox": "Checkbox", "Radio": "Radio", "List": "List", "Scroll": "Scroll", "Spin": "Spin",
                "Drop": "Drop", "GBox": "GBox", "Label": "Label"}[kind]
    vml.append(
        f'<v:shape id="_x0000_s{shape_id}" type="#_x0000_t201" style="position:absolute;z-index:{i + 1}" filled="f" stroked="f" o:insetmode="auto">'
        f'<x:ClientData ObjectType="{vml_kind}"><x:Anchor>{f[0]}, 0, {f[1]}, 0, {t[0]}, 0, {t[1]}, 0</x:Anchor><x:AutoFill>False</x:AutoFill></x:ClientData></v:shape>')

sheet = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" '
         f'xmlns:r="{R}" xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:x14="http://schemas.microsoft.com/office/spreadsheetml/2009/9/main" '
         'xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"><sheetFormatPr defaultRowHeight="15"/><sheetData>' + "".join(rows) + '</sheetData>'
         '<pageMargins left="0.7" right="0.7" top="0.75" bottom="0.75" header="0.3" footer="0.3"/><drawing r:id="rId1"/><legacyDrawing r:id="rId2"/>'
         '<controls>' + "".join(sheet_controls) + '</controls></worksheet>')
drawing_xml = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" '
               'xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">' + "".join(drawing) + '</xdr:wsDr>')
vml_xml = ('<xml xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office" xmlns:x="urn:schemas-microsoft-com:office:excel">\r\n'
           ' <o:shapelayout v:ext="edit"><o:idmap v:ext="edit" data="1"/></o:shapelayout><v:shapetype id="_x0000_t201" coordsize="21600,21600" o:spt="201" path="m,l,21600r21600,l21600,xe">'
           '<v:stroke joinstyle="miter"/><v:path shadowok="f" o:extrusionok="f" strokeok="f" fillok="f" o:connecttype="rect"/><o:lock v:ext="edit" shapetype="t"/></v:shapetype>'
           + "".join(vml) + '</xml>')
styles = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="1"><font><sz val="11"/><name val="Calibri"/><family val="2"/><scheme val="minor"/></font></fonts>'
          '<fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills><borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders>'
          '<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/></cellXfs>'
          '<cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles></styleSheet>')
CT = "application/vnd.openxmlformats-officedocument"
overrides = "".join(f'<Override PartName="/{p}" ContentType="application/vnd.ms-excel.controlproperties+xml"/>' for p in props)
files = {
    "[Content_Types].xml": '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
        f'<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="vml" ContentType="{CT}.vmlDrawing"/>'
        f'<Override PartName="/xl/workbook.xml" ContentType="{CT}.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="{CT}.spreadsheetml.worksheet+xml"/>'
        f'<Override PartName="/xl/styles.xml" ContentType="{CT}.spreadsheetml.styles+xml"/><Override PartName="/xl/drawings/drawing1.xml" ContentType="{CT}.drawing+xml"/>' + overrides + '</Types>',
    "_rels/.rels": f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{R}/officeDocument" Target="xl/workbook.xml"/></Relationships>',
    "xl/workbook.xml": f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="{R}"><sheets><sheet name="Controls" sheetId="1" r:id="rId1"/></sheets></workbook>',
    "xl/_rels/workbook.xml.rels": f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{R}/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="{R}/styles" Target="styles.xml"/></Relationships>',
    "xl/styles.xml": styles,
    "xl/worksheets/sheet1.xml": sheet,
    "xl/worksheets/_rels/sheet1.xml.rels": '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">' + "".join(rels) + '</Relationships>',
    "xl/drawings/drawing1.xml": drawing_xml,
    "xl/drawings/vmlDrawing1.vml": vml_xml,
}
files.update(props)
with zipfile.ZipFile(sys.argv[1], "w", zipfile.ZIP_DEFLATED) as z:
    for name, content in files.items():
        info = zipfile.ZipInfo(name, date_time=(2026, 11, 20, 0, 0, 0))
        info.compress_type = zipfile.ZIP_DEFLATED
        z.writestr(info, content)
