#!/usr/bin/env python3
"""Writes kept-objects.xlsx (own work, dc-zjsg.9): one sheet holding the four kinds of object DNA Calc keeps but does not
edit, each stored the way Excel stores it, so their fallbacks can be drawn and an untouched save checked byte for byte:
- an equation: an mc:AlternateContent whose a14 Choice is a text box with OMML (a14:m / m:oMathPara: a fraction, a
  superscript, a radical, an n-ary sum, a delimiter and a 2x2 matrix) and whose Fallback is the same box filled with a
  picture of the equation (blipFill) and its text unfilled;
- ink: an mc:AlternateContent whose xdr14 Choice is a content part (xl/ink/ink1.xml) and whose Fallback is a picture;
- SmartArt: a graphic frame naming its data, layout, style and colour parts, with the drawing Excel caches beside them
  (xl/diagrams/drawing1.xml, dsp shapes: three boxes and two arrows);
- an OLE object: the sheet's oleObjects with an objectPr anchor and its picture (EMF in Excel's files, a PNG here), the
  embedding part, and the hidden drawing shape Excel writes for it.
Pictures are small PNGs drawn here (Pillow). Run: python3 -I make-kept-objects.py kept-objects.xlsx"""
import io, struct, sys, zipfile
from PIL import Image, ImageDraw


def minimal_ole_cfb():
    """Minimal valid MS-CFB (OLE2 compound file, 3 x 512 bytes): header, 1 FAT sector, 1 Root Entry directory sector."""
    hdr = bytearray(512)
    hdr[0:8] = b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1"  # CFBF signature
    struct.pack_into("<HHHHHH", hdr, 24, 0x003E, 0x0003, 0xFFFE, 9, 6, 0)  # v3, little-endian, 512B/64B sectors
    struct.pack_into("<IIIIIIII", hdr, 44, 1, 1, 0, 4096, 0xFFFFFFFE, 0, 0xFFFFFFFE, 0)  # 1 FAT, dir at sec 1
    struct.pack_into("<I", hdr, 76, 0)  # DIFAT[0] = sector 0 (FAT)
    for i in range(1, 109):
        struct.pack_into("<I", hdr, 76 + i * 4, 0xFFFFFFFF)
    fat = bytearray(b"\xff" * 512)
    struct.pack_into("<II", fat, 0, 0xFFFFFFFD, 0xFFFFFFFE)  # sec 0 = FATSECT, sec 1 = ENDOFCHAIN (dir)
    root = bytearray(512)
    name = "Root Entry\0".encode("utf-16le")
    root[0:len(name)] = name
    struct.pack_into("<HBBIII", root, 64, len(name), 5, 1, 0xFFFFFFFF, 0xFFFFFFFF, 0xFFFFFFFF)  # Root Storage, black
    struct.pack_into("<IQ", root, 116, 0xFFFFFFFE, 0)  # ENDOFCHAIN, size 0
    return bytes(hdr + fat + root)

EMU = 9525

def png(w, h, colour, label_box=None):
    im = Image.new("RGB", (w, h), "white")
    d = ImageDraw.Draw(im)
    d.rectangle([2, 2, w - 3, h - 3], outline=colour, width=3)
    d.line([8, h // 2, w - 8, h // 2], fill=colour, width=3)
    b = io.BytesIO(); im.save(b, "PNG", optimize=True); return b.getvalue()

EQUATION_PNG = png(240, 80, (0, 0, 0))
INK_PNG = png(160, 100, (200, 0, 0))
OLE_PNG = png(128, 96, (0, 0, 160))

R = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
XDR = "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing"
A = "http://schemas.openxmlformats.org/drawingml/2006/main"
MC = "http://schemas.openxmlformats.org/markup-compatibility/2006"
M = "http://schemas.openxmlformats.org/officeDocument/2006/math"
A14 = "http://schemas.microsoft.com/office/drawing/2010/main"
XDR14 = "http://schemas.microsoft.com/office/excel/2010/spreadsheetDrawing"

def frm(col, row):
    return f"<xdr:col>{col}</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>{row}</xdr:row><xdr:rowOff>0</xdr:rowOff>"

def two(c0, r0, c1, r1, body, edit=""):
    return f'<xdr:twoCellAnchor{edit}><xdr:from>{frm(c0, r0)}</xdr:from><xdr:to>{frm(c1, r1)}</xdr:to>{body}<xdr:clientData/></xdr:twoCellAnchor>'

def mr(t):
    return f'<m:r><a:rPr lang="en-US" i="1"><a:latin typeface="Cambria Math" panose="02040503050406030204" pitchFamily="18" charset="0"/></a:rPr><m:t>{t}</m:t></m:r>'

OMML = (f'<m:oMathPara xmlns:m="{M}"><m:oMathParaPr><m:jc m:val="centerGroup"/></m:oMathParaPr><m:oMath>'
        + f'<m:f><m:fPr><m:ctrlPr><a:rPr lang="en-US" i="1"/></m:ctrlPr></m:fPr><m:num>{mr("a")}</m:num><m:den>{mr("b")}</m:den></m:f>'
        + mr("+")
        + f'<m:sSup><m:e>{mr("x")}</m:e><m:sup>{mr("2")}</m:sup></m:sSup>'
        + mr("+")
        + f'<m:rad><m:radPr><m:degHide m:val="1"/></m:radPr><m:deg/><m:e>{mr("y")}</m:e></m:rad>'
        + mr("+")
        + f'<m:nary><m:naryPr><m:chr m:val="∑"/></m:naryPr><m:sub>{mr("i=1")}</m:sub><m:sup>{mr("n")}</m:sup><m:e>{mr("i")}</m:e></m:nary>'
        + f'<m:d><m:dPr><m:begChr m:val="("/><m:endChr m:val=")"/></m:dPr><m:e><m:m><m:mr><m:e>{mr("1")}</m:e><m:e>{mr("0")}</m:e></m:mr>'
        + f'<m:mr><m:e>{mr("0")}</m:e><m:e>{mr("1")}</m:e></m:mr></m:m></m:e></m:d>'
        + '</m:oMath></m:oMathPara>')

TEXTBOX_STYLE = ('<xdr:style><a:lnRef idx="0"><a:scrgbClr r="0" g="0" b="0"/></a:lnRef><a:fillRef idx="0"><a:scrgbClr r="0" g="0" b="0"/></a:fillRef>'
                 '<a:effectRef idx="0"><a:scrgbClr r="0" g="0" b="0"/></a:effectRef><a:fontRef idx="minor"><a:schemeClr val="dk1"/></a:fontRef></xdr:style>')

def equation_sp(fallback):
    fill = ('<a:blipFill><a:blip r:embed="rId1"/><a:stretch><a:fillRect/></a:stretch></a:blipFill>' if fallback
            else '<a:solidFill><a:schemeClr val="lt1"/></a:solidFill>')
    body = (f'<a:p><a:pPr algn="ctr"/><a:r><a:rPr lang="en-US" sz="1100"><a:noFill/></a:rPr><a:t> </a:t></a:r></a:p>' if fallback
            else f'<a:p><a:pPr algn="ctr"/><a14:m>{OMML}</a14:m><a:endParaRPr lang="en-US" sz="1100"/></a:p>')
    return (f'<xdr:sp macro="" textlink=""><xdr:nvSpPr><xdr:cNvPr id="2" name="TextBox 1"/><xdr:cNvSpPr txBox="1"/></xdr:nvSpPr>'
            f'<xdr:spPr><a:xfrm><a:off x="{64 * EMU}" y="{20 * EMU}"/><a:ext cx="{256 * EMU}" cy="{80 * EMU}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom>{fill}'
            f'<a:ln w="9525" cmpd="sng"><a:solidFill><a:schemeClr val="lt1"><a:shade val="50000"/></a:schemeClr></a:solidFill></a:ln></xdr:spPr>{TEXTBOX_STYLE}'
            f'<xdr:txBody><a:bodyPr vertOverflow="clip" horzOverflow="clip" wrap="none" rtlCol="0" anchor="t"><a:spAutoFit/></a:bodyPr><a:lstStyle/>{body}</xdr:txBody></xdr:sp>')

equation = (f'<mc:AlternateContent xmlns:mc="{MC}"><mc:Choice xmlns:a14="{A14}" Requires="a14">{two(1, 1, 5, 5, equation_sp(False))}</mc:Choice>'
            f'<mc:Fallback>{two(1, 1, 5, 5, equation_sp(True))}</mc:Fallback></mc:AlternateContent>')

ink = (f'<mc:AlternateContent xmlns:mc="{MC}"><mc:Choice xmlns:xdr14="{XDR14}" Requires="xdr14">'
       + two(7, 1, 10, 6, f'<xdr:contentPart xmlns:r="{R}" r:id="rId2"><xdr14:nvContentPartPr><xdr14:cNvPr id="3" name="Ink 2"/><xdr14:cNvContentPartPr><a14:cpLocks xmlns:a14="{A14}" noRot="1" noChangeAspect="1"/></xdr14:cNvContentPartPr></xdr14:nvContentPartPr>'
             f'<xdr14:nvPr macro=""/><xdr14:xfrm><a:off x="{448 * EMU}" y="{20 * EMU}"/><a:ext cx="{192 * EMU}" cy="{100 * EMU}"/></xdr14:xfrm></xdr:contentPart>', ' editAs="oneCell"')
       + '</mc:Choice><mc:Fallback>'
       + two(7, 1, 10, 6, '<xdr:pic macro=""><xdr:nvPicPr><xdr:cNvPr id="3" name="Ink 2"/><xdr:cNvPicPr/></xdr:nvPicPr><xdr:blipFill><a:blip r:embed="rId3"/><a:stretch><a:fillRect/></a:stretch></xdr:blipFill>'
             f'<xdr:spPr><a:xfrm><a:off x="{448 * EMU}" y="{20 * EMU}"/><a:ext cx="{192 * EMU}" cy="{100 * EMU}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></xdr:spPr></xdr:pic>', ' editAs="oneCell"')
       + '</mc:Fallback></mc:AlternateContent>')

DGM = "http://schemas.openxmlformats.org/drawingml/2006/diagram"
smartart = two(1, 8, 9, 16, f'<xdr:graphicFrame macro=""><xdr:nvGraphicFramePr><xdr:cNvPr id="4" name="Diagram 3"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr>'
               f'<xdr:xfrm><a:off x="{64 * EMU}" y="{160 * EMU}"/><a:ext cx="{512 * EMU}" cy="{160 * EMU}"/></xdr:xfrm><a:graphic><a:graphicData uri="{DGM}">'
               f'<dgm:relIds xmlns:dgm="{DGM}" xmlns:r="{R}" r:dm="rId4" r:lo="rId5" r:qs="rId6" r:cs="rId7"/></a:graphicData></a:graphic></xdr:graphicFrame>')

ole_shape = two(11, 8, 13, 13, '<xdr:sp macro="" textlink=""><xdr:nvSpPr><xdr:cNvPr id="1025" name="Object 1" hidden="1"><a:extLst><a:ext uri="{63B3BB69-23CF-44E3-9099-C40C66FF867C}">'
                f'<a14:compatExt xmlns:a14="{A14}" spid="_x0000_s1025"/></a:ext></a:extLst></xdr:cNvPr><xdr:cNvSpPr/></xdr:nvSpPr>'
                f'<xdr:spPr bwMode="auto"><a:xfrm><a:off x="{704 * EMU}" y="{160 * EMU}"/><a:ext cx="{128 * EMU}" cy="{100 * EMU}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></xdr:spPr></xdr:sp>')
ole = f'<mc:AlternateContent xmlns:mc="{MC}"><mc:Choice xmlns:a14="{A14}" Requires="a14">{ole_shape}</mc:Choice></mc:AlternateContent>'

drawing = (f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<xdr:wsDr xmlns:xdr="{XDR}" xmlns:a="{A}" xmlns:r="{R}">'
           + equation + ink + smartart + ole + '</xdr:wsDr>')
drawing_rels = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
    f'<Relationship Id="rId1" Type="{R}/image" Target="../media/image1.png"/>'
    '<Relationship Id="rId2" Type="http://schemas.microsoft.com/office/2007/relationships/ink" Target="../ink/ink1.xml"/>'
    f'<Relationship Id="rId3" Type="{R}/image" Target="../media/image2.png"/>'
    f'<Relationship Id="rId4" Type="{R}/diagramData" Target="../diagrams/data1.xml"/>'
    f'<Relationship Id="rId5" Type="{R}/diagramLayout" Target="../diagrams/layout1.xml"/>'
    f'<Relationship Id="rId6" Type="{R}/diagramQuickStyle" Target="../diagrams/quickStyle1.xml"/>'
    f'<Relationship Id="rId7" Type="{R}/diagramColors" Target="../diagrams/colors1.xml"/>'
    '<Relationship Id="rId8" Type="http://schemas.microsoft.com/office/2007/relationships/diagramDrawing" Target="../diagrams/drawing1.xml"/>'
    '</Relationships>')

ink_xml = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<inkml:ink xmlns:inkml="http://www.w3.org/2003/InkML">'
           '<inkml:definitions><inkml:context xml:id="ctx0"><inkml:inkSource xml:id="inkSrc0"><inkml:traceFormat>'
           '<inkml:channel name="X" type="integer" max="19200" units="cm"/><inkml:channel name="Y" type="integer" max="10800" units="cm"/>'
           '<inkml:channel name="F" type="integer" max="32767" units="dev"/></inkml:traceFormat>'
           '<inkml:channelProperties><inkml:channelProperty channel="X" name="resolution" value="1000" units="1/cm"/>'
           '<inkml:channelProperty channel="Y" name="resolution" value="1000" units="1/cm"/>'
           '<inkml:channelProperty channel="F" name="resolution" value="0" units="1/dev"/></inkml:channelProperties></inkml:inkSource>'
           '<inkml:timestamp xml:id="ts0" timeString="2026-11-19T00:00:00"/></inkml:context>'
           '<inkml:brush xml:id="br0"><inkml:brushProperty name="width" value="0.05" units="cm"/><inkml:brushProperty name="height" value="0.05" units="cm"/>'
           '<inkml:brushProperty name="color" value="#E71224"/><inkml:brushProperty name="ignorePressure" value="1"/></inkml:brush></inkml:definitions>'
           '<inkml:trace contextRef="#ctx0" brushRef="#br0">0 0 0, 100 50 0, 200 0 0, 300 50 0</inkml:trace></inkml:ink>')

DSP = "http://schemas.microsoft.com/office/drawing/2008/diagram"
points = ["{11111111-1111-1111-1111-111111111111}", "{22222222-2222-2222-2222-222222222222}", "{33333333-3333-3333-3333-333333333333}"]
labels = ["Plan", "Build", "Ship"]
data = (f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<dgm:dataModel xmlns:dgm="{DGM}" xmlns:a="{A}"><dgm:ptLst>'
        '<dgm:pt modelId="{00000000-0000-0000-0000-000000000001}" type="doc"><dgm:prSet loTypeId="urn:microsoft.com/office/officeart/2005/8/layout/process1"/><dgm:spPr/><dgm:t><a:bodyPr/><a:lstStyle/><a:p><a:endParaRPr lang="en-US"/></a:p></dgm:t></dgm:pt>'
        + "".join(f'<dgm:pt modelId="{p}"><dgm:prSet phldrT="[Text]"/><dgm:spPr/><dgm:t><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US"/><a:t>{t}</a:t></a:r></a:p></dgm:t></dgm:pt>' for p, t in zip(points, labels))
        + '</dgm:ptLst><dgm:cxnLst>'
        + "".join(f'<dgm:cxn modelId="{{4444444{i}-0000-0000-0000-000000000000}}" srcId="{{00000000-0000-0000-0000-000000000001}}" destId="{p}" srcOrd="{i}" destOrd="0"/>' for i, p in enumerate(points))
        + '</dgm:cxnLst><dgm:bg/><dgm:whole/><dgm:extLst><a:ext uri="http://schemas.microsoft.com/office/drawing/2008/diagram">'
        f'<dsp:dataModelExt xmlns:dsp="{DSP}" relId="rId8" minVer="http://schemas.openxmlformats.org/drawingml/2006/diagram"/></a:ext></dgm:extLst></dgm:dataModel>')
layout = f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<dgm:layoutDef xmlns:dgm="{DGM}" xmlns:a="{A}" uniqueId="urn:microsoft.com/office/officeart/2005/8/layout/process1"><dgm:layoutNode name="Name0"/></dgm:layoutDef>'
quick = f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<dgm:styleDef xmlns:dgm="{DGM}" xmlns:a="{A}" uniqueId="urn:microsoft.com/office/officeart/2005/8/quickstyle/simple1"><dgm:scene3d><a:camera prst="orthographicFront"/><a:lightRig rig="threePt" dir="t"/></dgm:scene3d></dgm:styleDef>'
colors = f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<dgm:colorsDef xmlns:dgm="{DGM}" xmlns:a="{A}" uniqueId="urn:microsoft.com/office/officeart/2005/8/colors/accent1_2"><dgm:styleLbl name="node0"/></dgm:colorsDef>'

def dsp_sp(model, x, y, w, h, geom, text):
    body = (f'<dsp:txBody><a:bodyPr spcFirstLastPara="0" vert="horz" wrap="square" lIns="64770" tIns="64770" rIns="64770" bIns="64770" numCol="1" spcCol="1270" anchor="ctr" anchorCtr="0"><a:noAutofit/></a:bodyPr>'
            f'<a:lstStyle/><a:p><a:pPr algn="ctr"/><a:r><a:rPr lang="en-US" sz="1700" kern="1200"/><a:t>{text}</a:t></a:r></a:p></dsp:txBody>' if text else "")
    return (f'<dsp:sp modelId="{model}"><dsp:nvSpPr><dsp:cNvPr id="0" name=""/><dsp:cNvSpPr/></dsp:nvSpPr><dsp:spPr><a:xfrm><a:off x="{x * EMU}" y="{y * EMU}"/><a:ext cx="{w * EMU}" cy="{h * EMU}"/></a:xfrm>'
            f'<a:prstGeom prst="{geom}"><a:avLst/></a:prstGeom><a:solidFill><a:schemeClr val="accent1"><a:hueOff val="0"/></a:schemeClr></a:solidFill>'
            f'<a:ln w="12700"><a:solidFill><a:schemeClr val="lt1"/></a:solidFill></a:ln></dsp:spPr>'
            f'<dsp:style><a:lnRef idx="2"><a:scrgbClr r="0" g="0" b="0"/></a:lnRef><a:fillRef idx="1"><a:scrgbClr r="0" g="0" b="0"/></a:fillRef><a:effectRef idx="0"><a:scrgbClr r="0" g="0" b="0"/></a:effectRef>'
            f'<a:fontRef idx="minor"><a:schemeClr val="lt1"/></a:fontRef></dsp:style>{body}</dsp:sp>')

diagram_drawing = (f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<dsp:drawing xmlns:dgm="{DGM}" xmlns:dsp="{DSP}" xmlns:a="{A}"><dsp:spTree>'
                   '<dsp:nvGrpSpPr><dsp:cNvPr id="0" name=""/><dsp:cNvGrpSpPr/></dsp:nvGrpSpPr><dsp:grpSpPr/>'
                   + dsp_sp(points[0], 0, 30, 128, 100, "roundRect", "Plan")
                   + dsp_sp("{55555555-0000-0000-0000-000000000001}", 141, 64, 52, 32, "rightArrow", "")
                   + dsp_sp(points[1], 192, 30, 128, 100, "roundRect", "Build")
                   + dsp_sp("{55555555-0000-0000-0000-000000000002}", 333, 64, 52, 32, "rightArrow", "")
                   + dsp_sp(points[2], 384, 30, 128, 100, "roundRect", "Ship")
                   + '</dsp:spTree></dsp:drawing>')

rows = "".join(f'<row r="{r}"/>' for r in range(1, 2))
sheet = (f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="{R}" '
         f'xmlns:mc="{MC}" xmlns:x14ac="http://schemas.microsoft.com/office/spreadsheetml/2009/9/ac" xmlns:xdr="{XDR}" mc:Ignorable="x14ac">'
         '<sheetFormatPr defaultRowHeight="15" x14ac:dyDescent="0.25"/><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Kept objects</t></is></c></row></sheetData>'
         '<pageMargins left="0.7" right="0.7" top="0.75" bottom="0.75" header="0.3" footer="0.3"/>'
         '<drawing r:id="rId1"/><legacyDrawing r:id="rId2"/>'
         f'<oleObjects><mc:AlternateContent><mc:Choice xmlns:x14="http://schemas.microsoft.com/office/spreadsheetml/2009/9/main" Requires="x14"><oleObject progId="Package" shapeId="1025" r:id="rId3">'
         '<objectPr defaultSize="0" autoPict="0" r:id="rId4"><anchor moveWithCells="1"><from><xdr:col>11</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>8</xdr:row><xdr:rowOff>0</xdr:rowOff></from>'
         '<to><xdr:col>13</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>13</xdr:row><xdr:rowOff>0</xdr:rowOff></to></anchor></objectPr></oleObject></mc:Choice>'
         '<mc:Fallback><oleObject progId="Package" shapeId="1025" r:id="rId3"/></mc:Fallback></mc:AlternateContent></oleObjects></worksheet>')
sheet_rels = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
    f'<Relationship Id="rId1" Type="{R}/drawing" Target="../drawings/drawing1.xml"/>'
    f'<Relationship Id="rId2" Type="{R}/vmlDrawing" Target="../drawings/vmlDrawing1.vml"/>'
    f'<Relationship Id="rId3" Type="{R}/oleObject" Target="../embeddings/Package1.bin"/>'
    f'<Relationship Id="rId4" Type="{R}/image" Target="../media/image3.png"/></Relationships>')
vml = ('<xml xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office" xmlns:x="urn:schemas-microsoft-com:office:excel">\r\n'
       ' <o:shapelayout v:ext="edit"><o:idmap v:ext="edit" data="1"/></o:shapelayout><v:shapetype id="_x0000_t75" coordsize="21600,21600" o:spt="75" o:preferrelative="t" path="m@4@5l@4@11@9@11@9@5xe" filled="f" stroked="f">'
       '<v:stroke joinstyle="miter"/><v:path o:extrusionok="f" gradientshapeok="t" o:connecttype="rect"/><o:lock v:ext="edit" aspectratio="t"/></v:shapetype>'
       '<v:shape id="_x0000_s1025" type="#_x0000_t75" style="position:absolute;margin-left:528pt;margin-top:120pt;width:96pt;height:75pt;z-index:1">'
       '<v:imagedata o:relid="rId1" o:title=""/><x:ClientData ObjectType="Pict"><x:SizeWithCells/><x:Anchor>11, 0, 8, 0, 13, 0, 13, 0</x:Anchor><x:CF>Pict</x:CF><x:AutoPict/></x:ClientData></v:shape></xml>')
vml_rels = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
            f'<Relationship Id="rId1" Type="{R}/image" Target="../media/image3.png"/></Relationships>')

CT = "application/vnd.openxmlformats-officedocument"
files = {
 "[Content_Types].xml": '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
   '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>'
   f'<Default Extension="png" ContentType="image/png"/><Default Extension="vml" ContentType="{CT}.vmlDrawing"/><Default Extension="bin" ContentType="{CT}.oleObject"/>'
   f'<Override PartName="/xl/workbook.xml" ContentType="{CT}.spreadsheetml.sheet.main+xml"/>'
   f'<Override PartName="/xl/worksheets/sheet1.xml" ContentType="{CT}.spreadsheetml.worksheet+xml"/>'
   f'<Override PartName="/xl/styles.xml" ContentType="{CT}.spreadsheetml.styles+xml"/>'
   f'<Override PartName="/xl/drawings/drawing1.xml" ContentType="{CT}.drawing+xml"/>'
   '<Override PartName="/xl/ink/ink1.xml" ContentType="application/inkml+xml"/>'
   f'<Override PartName="/xl/diagrams/data1.xml" ContentType="{CT}.drawingml.diagramData+xml"/>'
   f'<Override PartName="/xl/diagrams/layout1.xml" ContentType="{CT}.drawingml.diagramLayout+xml"/>'
   f'<Override PartName="/xl/diagrams/quickStyle1.xml" ContentType="{CT}.drawingml.diagramStyle+xml"/>'
   f'<Override PartName="/xl/diagrams/colors1.xml" ContentType="{CT}.drawingml.diagramColors+xml"/>'
   '<Override PartName="/xl/diagrams/drawing1.xml" ContentType="application/vnd.ms-office.drawingml.diagramDrawing+xml"/></Types>',
 "_rels/.rels": f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{R}/officeDocument" Target="xl/workbook.xml"/></Relationships>',
 "xl/workbook.xml": f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="{R}"><sheets><sheet name="Kept" sheetId="1" r:id="rId1"/></sheets></workbook>',
 "xl/_rels/workbook.xml.rels": f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{R}/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="{R}/styles" Target="styles.xml"/></Relationships>',
 "xl/styles.xml": '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="1"><font><sz val="11"/><name val="Calibri"/><family val="2"/><scheme val="minor"/></font></fonts>'
   '<fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills><borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders>'
   '<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/></cellXfs>'
   '<cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles></styleSheet>',
 "xl/worksheets/sheet1.xml": sheet,
 "xl/worksheets/_rels/sheet1.xml.rels": sheet_rels,
 "xl/drawings/drawing1.xml": drawing,
 "xl/drawings/_rels/drawing1.xml.rels": drawing_rels,
 "xl/drawings/vmlDrawing1.vml": vml,
 "xl/drawings/_rels/vmlDrawing1.vml.rels": vml_rels,
 "xl/ink/ink1.xml": ink_xml,
 "xl/diagrams/data1.xml": data,
 "xl/diagrams/layout1.xml": layout,
 "xl/diagrams/quickStyle1.xml": quick,
 "xl/diagrams/colors1.xml": colors,
 "xl/diagrams/drawing1.xml": diagram_drawing,
 "xl/embeddings/Package1.bin": minimal_ole_cfb(),
 "xl/media/image1.png": EQUATION_PNG,
 "xl/media/image2.png": INK_PNG,
 "xl/media/image3.png": OLE_PNG,
}
with zipfile.ZipFile(sys.argv[1], "w", zipfile.ZIP_DEFLATED) as z:
    for name, data in files.items():
        info = zipfile.ZipInfo(name, date_time=(2026, 11, 19, 0, 0, 0))
        info.compress_type = zipfile.ZIP_DEFLATED
        z.writestr(info, data)
