#!/usr/bin/env python3
"""Writes chart-and-dialog-sheets.xlsx: our own small authored workbook (no third-party content).

Sheets in order: Data (worksheet), Chart1 (chart sheet over Data!A1:B2), Dialog1 (dialog sheet),
After (worksheet; B1 reads Data and SHEETS()). A sheet-scoped defined name on After pins that
localSheetId counts the chart and dialog sheets. Cached values are the ones Excel gives them.
Run: python3 make-fixture.py chart-and-dialog-sheets.xlsx
"""
import sys, zipfile

M = "http://schemas.openxmlformats.org/spreadsheetml/2006/main"
R = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
PR = "http://schemas.openxmlformats.org/package/2006/relationships"
D = R + "/"

parts = {}
parts["[Content_Types].xml"] = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/worksheets/sheet2.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/chartsheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.chartsheet+xml"/><Override PartName="/xl/dialogsheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.dialogsheet+xml"/><Override PartName="/xl/drawings/drawing1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawing+xml"/><Override PartName="/xl/charts/chart1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawingml.chart+xml"/></Types>"""
parts["_rels/.rels"] = f"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="{PR}"><Relationship Id="rId1" Type="{D}officeDocument" Target="xl/workbook.xml"/></Relationships>"""
parts["xl/workbook.xml"] = f"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="{M}" xmlns:r="{R}"><bookViews><workbookView activeTab="0"/></bookViews><sheets><sheet name="Data" sheetId="1" r:id="rId1"/><sheet name="Chart1" sheetId="2" r:id="rId2"/><sheet name="Dialog1" sheetId="3" r:id="rId3"/><sheet name="After" sheetId="4" r:id="rId4"/></sheets><definedNames><definedName name="Rate" localSheetId="3">After!$A$1</definedName></definedNames><calcPr calcId="191029"/></workbook>"""
parts["xl/_rels/workbook.xml.rels"] = f"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="{PR}"><Relationship Id="rId1" Type="{D}worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="{D}chartsheet" Target="chartsheets/sheet1.xml"/><Relationship Id="rId3" Type="{D}dialogsheet" Target="dialogsheets/sheet1.xml"/><Relationship Id="rId4" Type="{D}worksheet" Target="worksheets/sheet2.xml"/></Relationships>"""
parts["xl/worksheets/sheet1.xml"] = f"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="{M}"><sheetData><row r="1"><c r="A1"><v>2</v></c><c r="B1"><v>3</v></c></row><row r="2"><c r="A2"><v>5</v></c><c r="B2"><f>A2*B1</f><v>15</v></c></row></sheetData></worksheet>"""
parts["xl/worksheets/sheet2.xml"] = f"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="{M}"><sheetData><row r="1"><c r="A1"><v>7</v></c><c r="B1"><f>Data!B2+Rate</f><v>22</v></c><c r="C1"><f>SHEETS()</f><v>4</v></c></row></sheetData></worksheet>"""
parts["xl/chartsheets/sheet1.xml"] = f"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<chartsheet xmlns="{M}" xmlns:r="{R}"><sheetViews><sheetView workbookViewId="0" zoomToFit="1"/></sheetViews><pageMargins left="0.7" right="0.7" top="0.75" bottom="0.75" header="0.3" footer="0.3"/><drawing r:id="rId1"/></chartsheet>"""
parts["xl/chartsheets/_rels/sheet1.xml.rels"] = f"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="{PR}"><Relationship Id="rId1" Type="{D}drawing" Target="../drawings/drawing1.xml"/></Relationships>"""
parts["xl/drawings/drawing1.xml"] = f"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><xdr:absoluteAnchor><xdr:pos x="0" y="0"/><xdr:ext cx="9293678" cy="6000000"/><xdr:graphicFrame macro=""><xdr:nvGraphicFramePr><xdr:cNvPr id="2" name="Chart 1"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/></xdr:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:r="{R}" r:id="rId1"/></a:graphicData></a:graphic></xdr:graphicFrame><xdr:clientData/></xdr:absoluteAnchor></xdr:wsDr>"""
parts["xl/drawings/_rels/drawing1.xml.rels"] = f"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="{PR}"><Relationship Id="rId1" Type="{D}chart" Target="../charts/chart1.xml"/></Relationships>"""
parts["xl/charts/chart1.xml"] = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><c:chart><c:plotArea><c:layout/><c:barChart><c:barDir val="col"/><c:grouping val="clustered"/><c:ser><c:idx val="0"/><c:order val="0"/><c:val><c:numRef><c:f>Data!$A$1:$B$1</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val="2"/><c:pt idx="0"><c:v>2</c:v></c:pt><c:pt idx="1"><c:v>3</c:v></c:pt></c:numCache></c:numRef></c:val></c:ser><c:axId val="1"/><c:axId val="2"/></c:barChart><c:catAx><c:axId val="1"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="b"/><c:crossAx val="2"/></c:catAx><c:valAx><c:axId val="2"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="l"/><c:crossAx val="1"/></c:valAx></c:plotArea><c:plotVisOnly val="1"/></c:chart></c:chartSpace>"""
parts["xl/dialogsheets/sheet1.xml"] = f"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<dialogsheet xmlns="{M}"><sheetViews><sheetView workbookViewId="0"/></sheetViews><sheetFormatPr defaultRowHeight="15"/><pageMargins left="0.75" right="0.75" top="1" bottom="1" header="0.5" footer="0.5"/></dialogsheet>"""

out = sys.argv[1]
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
    for name, text in parts.items():
        info = zipfile.ZipInfo(name, date_time=(2026, 11, 11, 0, 0, 0))
        info.compress_type = zipfile.ZIP_DEFLATED
        z.writestr(info, text.encode("utf-8"))
