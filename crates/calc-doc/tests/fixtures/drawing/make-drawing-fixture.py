#!/usr/bin/env python3
"""Writes drawing-layer.xlsx (own work, dc-zjsg.1): one sheet whose drawing part holds every object
kind the reader models (auto shape with text and fill, text box, picture, connector glued to two
shapes, group of two) and objects it only keeps (an OLE-style graphic frame, a content part).
Run: python3 -I make-drawing-fixture.py drawing-layer.xlsx"""
import struct, sys, zlib, zipfile

def png():
    def chunk(t, d):
        c = struct.pack(">I", len(d)) + t + d
        return c + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
    raw = b"\x00\xff\x00\x00"
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 1, 1, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw + b"\x00\x00")) + chunk(b"IEND", b"")

XDR = 'xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"'

def pt(c, co, r, ro):
    return f"<xdr:col>{c}</xdr:col><xdr:colOff>{co}</xdr:colOff><xdr:row>{r}</xdr:row><xdr:rowOff>{ro}</xdr:rowOff>"

drawing = f'''<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<xdr:wsDr {XDR}>
<xdr:twoCellAnchor><xdr:from>{pt(1,0,1,0)}</xdr:from><xdr:to>{pt(3,0,4,0)}</xdr:to>
<xdr:sp macro="" textlink=""><xdr:nvSpPr><xdr:cNvPr id="2" name="Rectangle 1"/><xdr:cNvSpPr/></xdr:nvSpPr>
<xdr:spPr><a:xfrm><a:off x="609600" y="190500"/><a:ext cx="1219200" cy="571500"/></a:xfrm><a:prstGeom prst="roundRect"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val="FF8000"/></a:solidFill><a:ln w="19050"><a:solidFill><a:schemeClr val="accent2"/></a:solidFill><a:prstDash val="dash"/></a:ln></xdr:spPr>
<xdr:style><a:lnRef idx="2"><a:schemeClr val="accent1"/></a:lnRef><a:fillRef idx="1"><a:schemeClr val="accent1"/></a:fillRef><a:effectRef idx="0"><a:schemeClr val="accent1"/></a:effectRef><a:fontRef idx="minor"><a:schemeClr val="lt1"/></a:fontRef></xdr:style>
<xdr:txBody><a:bodyPr anchor="ctr"/><a:lstStyle/><a:p><a:pPr algn="ctr"/><a:r><a:rPr lang="en-US" sz="1100" b="1"/><a:t>Hello </a:t></a:r><a:r><a:rPr lang="en-US"/><a:t>shape</a:t></a:r></a:p><a:p><a:endParaRPr lang="en-US"/></a:p></xdr:txBody></xdr:sp><xdr:clientData/></xdr:twoCellAnchor>
<xdr:twoCellAnchor editAs="oneCell"><xdr:from>{pt(5,0,1,0)}</xdr:from><xdr:to>{pt(7,0,3,0)}</xdr:to>
<xdr:sp macro="" textlink=""><xdr:nvSpPr><xdr:cNvPr id="3" name="TextBox 2"/><xdr:cNvSpPr txBox="1"/></xdr:nvSpPr>
<xdr:spPr><a:xfrm><a:off x="3048000" y="190500"/><a:ext cx="1219200" cy="381000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/></xdr:spPr>
<xdr:txBody><a:bodyPr wrap="square"/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US"/><a:t>a &amp; b &lt; c</a:t></a:r></a:p></xdr:txBody></xdr:sp><xdr:clientData/></xdr:twoCellAnchor>
<xdr:oneCellAnchor><xdr:from>{pt(1,0,6,0)}</xdr:from><xdr:ext cx="952500" cy="952500"/>
<xdr:pic><xdr:nvPicPr><xdr:cNvPr id="4" name="Picture 3" descr="a red dot"/><xdr:cNvPicPr><a:picLocks noChangeAspect="1"/></xdr:cNvPicPr></xdr:nvPicPr>
<xdr:blipFill><a:blip r:embed="rId1"/><a:stretch><a:fillRect/></a:stretch></xdr:blipFill>
<xdr:spPr><a:xfrm><a:off x="609600" y="1143000"/><a:ext cx="952500" cy="952500"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></xdr:spPr></xdr:pic><xdr:clientData/></xdr:oneCellAnchor>
<xdr:twoCellAnchor><xdr:from>{pt(3,0,2,0)}</xdr:from><xdr:to>{pt(5,0,2,0)}</xdr:to>
<xdr:cxnSp macro=""><xdr:nvCxnSpPr><xdr:cNvPr id="5" name="Straight Arrow Connector 4"/><xdr:cNvCxnSpPr><a:stCxn id="2" idx="3"/><a:endCxn id="3" idx="1"/></xdr:cNvCxnSpPr></xdr:nvCxnSpPr>
<xdr:spPr><a:xfrm flipV="1"><a:off x="1828800" y="380999"/><a:ext cx="1219200" cy="1"/></a:xfrm><a:prstGeom prst="straightConnector1"><a:avLst/></a:prstGeom><a:ln w="9525"><a:solidFill><a:srgbClr val="000000"/></a:solidFill><a:tailEnd type="triangle"/></a:ln></xdr:spPr></xdr:cxnSp><xdr:clientData/></xdr:twoCellAnchor>
<xdr:absoluteAnchor><xdr:pos x="5715000" y="190500"/><xdr:ext cx="1905000" cy="952500"/>
<xdr:grpSp><xdr:nvGrpSpPr><xdr:cNvPr id="6" name="Group 5"/><xdr:cNvGrpSpPr/></xdr:nvGrpSpPr>
<xdr:grpSpPr><a:xfrm><a:off x="5715000" y="190500"/><a:ext cx="1905000" cy="952500"/><a:chOff x="0" y="0"/><a:chExt cx="1905000" cy="952500"/></a:xfrm></xdr:grpSpPr>
<xdr:sp macro="" textlink=""><xdr:nvSpPr><xdr:cNvPr id="7" name="Oval 6"/><xdr:cNvSpPr/></xdr:nvSpPr><xdr:spPr><a:xfrm rot="5400000"><a:off x="0" y="0"/><a:ext cx="952500" cy="952500"/></a:xfrm><a:prstGeom prst="ellipse"><a:avLst/></a:prstGeom></xdr:spPr></xdr:sp>
<xdr:sp macro="" textlink=""><xdr:nvSpPr><xdr:cNvPr id="8" name="Rectangle 7" hidden="1"/><xdr:cNvSpPr/></xdr:nvSpPr><xdr:spPr><a:xfrm><a:off x="952500" y="0"/><a:ext cx="952500" cy="952500"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></xdr:spPr></xdr:sp>
</xdr:grpSp><xdr:clientData/></xdr:absoluteAnchor>
<xdr:twoCellAnchor><xdr:from>{pt(1,0,12,0)}</xdr:from><xdr:to>{pt(4,0,16,0)}</xdr:to>
<xdr:graphicFrame macro=""><xdr:nvGraphicFramePr><xdr:cNvPr id="9" name="Diagram 8"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/></xdr:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/diagram"/></a:graphic></xdr:graphicFrame><xdr:clientData/></xdr:twoCellAnchor>
</xdr:wsDr>
'''

files = {
 "[Content_Types].xml": '''<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/drawings/drawing1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawing+xml"/></Types>''',
 "_rels/.rels": '''<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>''',
 "xl/workbook.xml": '''<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets></workbook>''',
 "xl/_rels/workbook.xml.rels": '''<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>''',
 "xl/worksheets/sheet1.xml": '''<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData><drawing r:id="rId1"/></worksheet>''',
 "xl/worksheets/_rels/sheet1.xml.rels": '''<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing" Target="../drawings/drawing1.xml"/></Relationships>''',
 "xl/drawings/drawing1.xml": drawing,
 "xl/drawings/_rels/drawing1.xml.rels": '''<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/image1.png"/></Relationships>''',
}
with zipfile.ZipFile(sys.argv[1], "w", zipfile.ZIP_DEFLATED) as z:
    for name, text in files.items():
        z.writestr(zipfile.ZipInfo(name, (2026, 11, 14, 0, 0, 0)), text.encode(), zipfile.ZIP_DEFLATED)
    z.writestr(zipfile.ZipInfo("xl/media/image1.png", (2026, 11, 14, 0, 0, 0)), png(), zipfile.ZIP_DEFLATED)
