#!/usr/bin/env python3
"""Writes chart-kinds.xlsx (own work, dc-zjsg.6): one sheet "Kinds" with data in A1:E6 and seven charts written the way
Excel 2016 writes them (c:chartSpace with numRef/strRef caches), each in its own two-cell anchor:
  1 "3-D Column"   bar3DChart, barDir col, grouping clustered, two series (B, C) over A2:A6, with c:view3D
  2 "3-D Pie"      pie3DChart, one series (B) over A2:A6, varyColors
  3 "Radar marker" radarChart radarStyle marker, two series (B, C)
  4 "Radar filled" radarChart radarStyle filled, one series (B)
  5 "Bubble"       bubbleChart, one series: x B2:B6, y C2:C6, sizes D2:D6, bubbleScale 100, sizeRepresents area
  6 "Stock HLC"    stockChart, three series high (C), low (B), close (D) over A2:A6, hiLowLines
  7 "Stock OHLC"   stockChart, four series open (B), high (C), low (D), close (E), hiLowLines and upDownBars
  8 "3-D Line"     line3DChart, grouping standard, two series (B, C), with its series (depth) axis c:serAx id 3
  9 "Line trends"  lineChart, series B with a linear trendline (forward 1) and series C with a polynomial (order 2)
                   and a moving average (period 2) trendline
 10 "Scatter exp"  scatterChart, x B2:B6, y C2:C6, an exponential trendline in red, x error bars of 0.5
 11 "Line drops"   lineChart with drop lines and high-low lines, series B with fixed error bars of 1 (both),
                   series C with 10% error bars (plus, no end caps)
 12 "Column sd"    barChart clustered, series B with one standard deviation error bars
Not yet opened by Excel (codex-runs/dw2/RUN-ON-WINDOWS.txt).
Run: python3 -I make-chart-kinds.py shapes-paint4.xlsx chart-kinds.xlsx"""
import sys, zipfile

theme = zipfile.ZipFile(sys.argv[1]).read("xl/theme/theme1.xml")
header = ["Day", "Low", "High", "Close", "Last"]
data = [
    ["Mon", 10, 18, 14, 16],
    ["Tue", 12, 20, 13, 12],
    ["Wed", 9, 15, 11, 14],
    ["Thu", 11, 22, 21, 19],
    ["Fri", 14, 19, 15, 17],
]
cols = "ABCDE"
rows = ['<row r="1">' + "".join(f'<c r="{cols[i]}1" t="inlineStr"><is><t>{h}</t></is></c>' for i, h in enumerate(header)) + "</row>"]
for r, values in enumerate(data, start=2):
    cells = f'<c r="A{r}" t="inlineStr"><is><t>{values[0]}</t></is></c>' + "".join(f'<c r="{cols[i]}{r}"><v>{v}</v></c>' for i, v in enumerate(values) if i)
    rows.append(f'<row r="{r}">{cells}</row>')

C = "http://schemas.openxmlformats.org/drawingml/2006/chart"
A = "http://schemas.openxmlformats.org/drawingml/2006/main"
R = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"


def num_ref(col):
    pts = "".join(f'<c:pt idx="{i}"><c:v>{row[cols.index(col)]}</c:v></c:pt>' for i, row in enumerate(data))
    return f'<c:numRef><c:f>Kinds!${col}$2:${col}$6</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val="5"/>{pts}</c:numCache></c:numRef>'


def str_ref():
    pts = "".join(f'<c:pt idx="{i}"><c:v>{row[0]}</c:v></c:pt>' for i, row in enumerate(data))
    return f'<c:strRef><c:f>Kinds!$A$2:$A$6</c:f><c:strCache><c:ptCount val="5"/>{pts}</c:strCache></c:strRef>'


def name(col):
    return f'<c:tx><c:strRef><c:f>Kinds!${col}$1</c:f><c:strCache><c:ptCount val="1"/><c:pt idx="0"><c:v>{header[cols.index(col)]}</c:v></c:pt></c:strCache></c:strRef></c:tx>'


def ser(i, col, extra="", line=False):
    sp = '<c:spPr><a:ln w="28575" cap="rnd"><a:noFill/><a:round/></a:ln></c:spPr><c:marker><c:symbol val="none"/></c:marker>' if line else ""
    # `extra` (trendlines) goes where the schema puts it, before the categories.
    return f'<c:ser><c:idx val="{i}"/><c:order val="{i}"/>{name(col)}{sp}{extra}<c:cat>{str_ref()}</c:cat><c:val>{num_ref(col)}</c:val></c:ser>'


cat_val_axes = ('<c:catAx><c:axId val="1"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="b"/>'
                '<c:numFmt formatCode="General" sourceLinked="1"/><c:majorTickMark val="none"/><c:minorTickMark val="none"/><c:tickLblPos val="nextTo"/>'
                '<c:crossAx val="2"/><c:crosses val="autoZero"/><c:auto val="1"/><c:lblAlgn val="ctr"/><c:lblOffset val="100"/><c:noMultiLvlLbl val="0"/></c:catAx>'
                '<c:valAx><c:axId val="2"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="l"/><c:majorGridlines/>'
                '<c:numFmt formatCode="General" sourceLinked="1"/><c:majorTickMark val="none"/><c:minorTickMark val="none"/><c:tickLblPos val="nextTo"/>'
                '<c:crossAx val="1"/><c:crosses val="autoZero"/><c:crossBetween val="between"/></c:valAx>')
value_axes = ('<c:valAx><c:axId val="1"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="b"/>'
              '<c:numFmt formatCode="General" sourceLinked="1"/><c:majorTickMark val="none"/><c:minorTickMark val="none"/><c:tickLblPos val="nextTo"/>'
              '<c:crossAx val="2"/><c:crosses val="autoZero"/><c:crossBetween val="midCat"/></c:valAx>'
              '<c:valAx><c:axId val="2"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="l"/><c:majorGridlines/>'
              '<c:numFmt formatCode="General" sourceLinked="1"/><c:majorTickMark val="none"/><c:minorTickMark val="none"/><c:tickLblPos val="nextTo"/>'
              '<c:crossAx val="1"/><c:crosses val="autoZero"/><c:crossBetween val="midCat"/></c:valAx>')
ids = '<c:axId val="1"/><c:axId val="2"/>'


def errbars(kind, value, bar="both", direction=None, cap=True):
    """A c:errBars as Excel 2016 writes one."""
    d = f'<c:errDir val="{direction}"/>' if direction else ''
    return (f'<c:errBars>{d}<c:errBarType val="{bar}"/><c:errValType val="{kind}"/><c:noEndCap val="{0 if cap else 1}"/>'
            f'<c:val val="{value}"/></c:errBars>')


def trend(kind, order=None, period=None, forward=None, colour=None):
    """A c:trendline as Excel 2016 writes one (its dotted line in the series' or the stated colour)."""
    fill = f'<a:solidFill><a:srgbClr val="{colour}"/></a:solidFill>' if colour else '<a:solidFill><a:schemeClr val="accent1"/></a:solidFill>'
    out = f'<c:trendline><c:spPr><a:ln w="19050" cap="rnd">{fill}<a:prstDash val="sysDot"/></a:ln></c:spPr><c:trendlineType val="{kind}"/>'
    if order:
        out += f'<c:order val="{order}"/>'
    if period:
        out += f'<c:period val="{period}"/>'
    if forward:
        out += f'<c:forward val="{forward}"/>'
    return out + '<c:dispRSqr val="0"/><c:dispEq val="0"/></c:trendline>'
view3d = '<c:view3D><c:rotX val="15"/><c:rotY val="20"/><c:depthPercent val="100"/><c:rAngAx val="1"/></c:view3D>'
bubble_ser = (f'<c:ser><c:idx val="0"/><c:order val="0"/>{name("D")}<c:invertIfNegative val="0"/><c:xVal>{num_ref("B")}</c:xVal>'
              f'<c:yVal>{num_ref("C")}</c:yVal><c:bubbleSize>{num_ref("D")}</c:bubbleSize><c:bubble3D val="0"/></c:ser>')
charts = [
    ("3-D Column", view3d, f'<c:bar3DChart><c:barDir val="col"/><c:grouping val="clustered"/><c:varyColors val="0"/>{ser(0, "B")}{ser(1, "C")}'
     f'<c:gapWidth val="150"/><c:shape val="box"/>{ids}<c:axId val="0"/></c:bar3DChart>{cat_val_axes}'),
    ("3-D Pie", view3d.replace('<c:rAngAx val="1"/>', '<c:rAngAx val="0"/>'), f'<c:pie3DChart><c:varyColors val="1"/>{ser(0, "B")}</c:pie3DChart>'),
    ("Radar marker", "", f'<c:radarChart><c:radarStyle val="marker"/><c:varyColors val="0"/>{ser(0, "B")}{ser(1, "C")}{ids}</c:radarChart>{cat_val_axes}'),
    ("Radar filled", "", f'<c:radarChart><c:radarStyle val="filled"/><c:varyColors val="0"/>{ser(0, "B")}{ids}</c:radarChart>{cat_val_axes}'),
    ("Bubble", "", f'<c:bubbleChart><c:varyColors val="0"/>{bubble_ser}<c:bubbleScale val="100"/><c:showNegBubbles val="0"/>{ids}</c:bubbleChart>{value_axes}'),
    ("Stock HLC", "", f'<c:stockChart>{ser(0, "C", line=True)}{ser(1, "B", line=True)}{ser(2, "D", line=True)}'
     f'<c:hiLowLines/>{ids}</c:stockChart>{cat_val_axes}'),
    ("Stock OHLC", "", f'<c:stockChart>{ser(0, "B", line=True)}{ser(1, "C", line=True)}{ser(2, "D", line=True)}{ser(3, "E", line=True)}'
     f'<c:hiLowLines/><c:upDownBars><c:gapWidth val="150"/><c:upBars/><c:downBars/></c:upDownBars>{ids}</c:stockChart>{cat_val_axes}'),
    ("Line trends", "", f'<c:lineChart><c:grouping val="standard"/><c:varyColors val="0"/>'
     f'{ser(0, "B", extra=trend("linear", forward=1))}{ser(1, "C", extra=trend("poly", order=2) + trend("movingAvg", period=2))}'
     f'<c:marker val="1"/>{ids}</c:lineChart>{cat_val_axes}'),
    ("Scatter exp", "", f'<c:scatterChart><c:scatterStyle val="lineMarker"/><c:varyColors val="0"/>'
     f'<c:ser><c:idx val="0"/><c:order val="0"/>{name("C")}<c:spPr><a:ln w="19050"><a:noFill/></a:ln></c:spPr>'
     f'{trend("exp", colour="FF0000")}{errbars("fixedVal", 0.5, direction="x")}<c:xVal>{num_ref("B")}</c:xVal><c:yVal>{num_ref("C")}</c:yVal><c:smooth val="0"/></c:ser>'
     f'{ids}</c:scatterChart>{value_axes}'),
    ("Line drops", "", f'<c:lineChart><c:grouping val="standard"/><c:varyColors val="0"/>'
     f'{ser(0, "B", extra=errbars("fixedVal", 1))}{ser(1, "C", extra=errbars("percentage", 10, bar="plus", cap=False))}'
     f'<c:dropLines/><c:hiLowLines/><c:marker val="1"/>{ids}</c:lineChart>{cat_val_axes}'),
    ("Column sd", "", f'<c:barChart><c:barDir val="col"/><c:grouping val="clustered"/><c:varyColors val="0"/>'
     f'{ser(0, "B", extra=errbars("stdDev", 1))}{ser(1, "C")}<c:gapWidth val="150"/>{ids}</c:barChart>{cat_val_axes}'),
    ("3-D Line", view3d, f'<c:line3DChart><c:grouping val="standard"/><c:varyColors val="0"/>{ser(0, "B")}{ser(1, "C")}{ids}<c:axId val="3"/></c:line3DChart>'
     f'{cat_val_axes}<c:serAx><c:axId val="3"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="b"/>'
     '<c:majorTickMark val="out"/><c:minorTickMark val="none"/><c:tickLblPos val="nextTo"/><c:crossAx val="2"/><c:crosses val="autoZero"/></c:serAx>'),
]


def chart_xml(title, view, plot):
    return ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
            f'<c:chartSpace xmlns:c="{C}" xmlns:a="{A}" xmlns:r="{R}"><c:date1904 val="0"/><c:lang val="en-US"/><c:roundedCorners val="0"/>'
            f'<c:chart><c:title><c:tx><c:rich><a:bodyPr/><a:lstStyle/><a:p><a:r><a:t>{title}</a:t></a:r></a:p></c:rich></c:tx><c:overlay val="0"/></c:title>'
            f'<c:autoTitleDeleted val="0"/>{view}<c:plotArea><c:layout/>{plot}</c:plotArea><c:legend><c:legendPos val="r"/><c:overlay val="0"/></c:legend>'
            '<c:plotVisOnly val="1"/><c:dispBlanksAs val="gap"/></c:chart></c:chartSpace>')


XDR = "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing"
anchors = []
for n, (title, _, _) in enumerate(charts):
    col, row = 6 + (n % 2) * 8, (n // 2) * 15
    anchors.append(
        f'<xdr:twoCellAnchor><xdr:from><xdr:col>{col}</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>{row}</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from>'
        f'<xdr:to><xdr:col>{col + 7}</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>{row + 14}</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to>'
        f'<xdr:graphicFrame macro=""><xdr:nvGraphicFramePr><xdr:cNvPr id="{n + 2}" name="{title}"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr>'
        '<xdr:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/></xdr:xfrm>'
        f'<a:graphic><a:graphicData uri="{C}"><c:chart xmlns:c="{C}" r:id="rId{n + 1}"/></a:graphicData></a:graphic></xdr:graphicFrame><xdr:clientData/></xdr:twoCellAnchor>')
drawing = (f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<xdr:wsDr xmlns:xdr="{XDR}" xmlns:a="{A}" xmlns:r="{R}">' + "".join(anchors) + "</xdr:wsDr>")
drawing_rels = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
                + "".join(f'<Relationship Id="rId{n + 1}" Type="{R}/chart" Target="../charts/chart{n + 1}.xml"/>' for n in range(len(charts))) + "</Relationships>")
sheet = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" '
         f'xmlns:r="{R}"><sheetFormatPr defaultRowHeight="15"/><sheetData>' + "".join(rows) + '</sheetData>'
         '<pageMargins left="0.7" right="0.7" top="0.75" bottom="0.75" header="0.3" footer="0.3"/><drawing r:id="rId1"/></worksheet>')
styles = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="1"><font><sz val="11"/><name val="Calibri"/><family val="2"/><scheme val="minor"/></font></fonts>'
          '<fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills><borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders>'
          '<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/></cellXfs>'
          '<cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles></styleSheet>')
CT = "application/vnd.openxmlformats-officedocument"
files = {
    "[Content_Types].xml": '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>'
        f'<Override PartName="/xl/workbook.xml" ContentType="{CT}.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="{CT}.spreadsheetml.worksheet+xml"/>'
        f'<Override PartName="/xl/theme/theme1.xml" ContentType="{CT}.theme+xml"/><Override PartName="/xl/styles.xml" ContentType="{CT}.spreadsheetml.styles+xml"/>'
        f'<Override PartName="/xl/drawings/drawing1.xml" ContentType="{CT}.drawing+xml"/>'
        + "".join(f'<Override PartName="/xl/charts/chart{n + 1}.xml" ContentType="{CT}.drawingml.chart+xml"/>' for n in range(len(charts))) + '</Types>',
    "_rels/.rels": f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{R}/officeDocument" Target="xl/workbook.xml"/></Relationships>',
    "xl/workbook.xml": f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="{R}"><sheets><sheet name="Kinds" sheetId="1" r:id="rId1"/></sheets></workbook>',
    "xl/_rels/workbook.xml.rels": f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{R}/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="{R}/theme" Target="theme/theme1.xml"/><Relationship Id="rId3" Type="{R}/styles" Target="styles.xml"/></Relationships>',
    "xl/worksheets/sheet1.xml": sheet,
    "xl/worksheets/_rels/sheet1.xml.rels": f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{R}/drawing" Target="../drawings/drawing1.xml"/></Relationships>',
    "xl/drawings/drawing1.xml": drawing,
    "xl/drawings/_rels/drawing1.xml.rels": drawing_rels,
    "xl/theme/theme1.xml": theme,
    "xl/styles.xml": styles,
}
for n, (title, view, plot) in enumerate(charts):
    files[f"xl/charts/chart{n + 1}.xml"] = chart_xml(title, view, plot)
with zipfile.ZipFile(sys.argv[2], "w", zipfile.ZIP_DEFLATED) as z:
    for name_, content in files.items():
        info = zipfile.ZipInfo(name_, date_time=(2026, 11, 20, 0, 0, 0))
        info.compress_type = zipfile.ZIP_DEFLATED
        z.writestr(info, content)
