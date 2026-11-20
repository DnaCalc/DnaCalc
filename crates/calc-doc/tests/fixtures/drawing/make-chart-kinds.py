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
 12 "Column sd"    barChart clustered, series B with one standard deviation error bars; its anchor is editAs="oneCell"
 13 "Pie labels"   pieChart, series B, labels showing category and percentage (best fit)
 14 "Column labels" barChart clustered, series B, labels showing series name and value, separator "; ";
                   point 1 shows its category and value (separator ": "), point 2 has no label
 (the surface chart is placed before the 3-D line so the numbers below shift by one in the sheet's layout)
 "Surface"      surface3DChart over series B, C and D (filled, not a wireframe)
Excel 2016 charts (cx:chartSpace parts, each frame in an mc:AlternateContent around whole anchors with the text-box
fallback Excel writes for older readers), over F2:F6 (Change: 10, -3, 4, -2 and the subtotal 9):
 15 "Waterfall"    cx:waterfall, the last point a subtotal
 16 "Funnel"       cx:funnel over B2:B6
 17 "Treemap"      cx:treemap over B2:B6 with two category levels (the days, and the parents Early: Mon-Tue, Late: Wed-Fri
                   given at their first rows, the leaf formula covering A2:A6 only)
 18 "Box"          cx:boxWhisker over B2:B6 and C2:C6 as one series without categories (exclusive quartiles)
 19 "Histogram"    cx:clusteredColumn over C2:C6 binned by 3, closed on the right
 21 "Sunburst"     cx:sunburst over C2:C6 with the same levels
 20 "Pareto"       the same with a cx:paretoLine series (the cumulative share)
Not yet opened by Excel (codex-runs/dw2/RUN-ON-WINDOWS.txt).
Run: python3 -I make-chart-kinds.py shapes-paint4.xlsx chart-kinds.xlsx"""
import os, sys, zipfile

theme = zipfile.ZipFile(sys.argv[1]).read("xl/theme/theme1.xml")
header = ["Day", "Low", "High", "Close", "Last", "Change"]
data = [
    ["Mon", 10, 18, 14, 16, 10],
    ["Tue", 12, 20, 13, 12, -3],
    ["Wed", 9, 15, 11, 14, 4],
    ["Thu", 11, 22, 21, 19, -2],
    ["Fri", 14, 19, 15, 17, 9],
]
cols = "ABCDEF"
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


def labels(value=0, category=0, series=0, percent=0, position=None, separator=None, points=''):
    """A series' c:dLbls as Excel 2016 writes one (its schema order: position, the show flags, the separator)."""
    pos = f'<c:dLblPos val="{position}"/>' if position else ''
    sep = f'<c:separator>{separator}</c:separator>' if separator else ''
    return (f'<c:dLbls>{points}<c:spPr><a:noFill/><a:ln><a:noFill/></a:ln></c:spPr>{pos}<c:showLegendKey val="0"/><c:showVal val="{value}"/>'
            f'<c:showCatName val="{category}"/><c:showSerName val="{series}"/><c:showPercent val="{percent}"/><c:showBubbleSize val="0"/>'
            f'{sep}<c:showLeaderLines val="1"/></c:dLbls>')


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
    ("Pie labels", "", f'<c:pieChart><c:varyColors val="1"/>{ser(0, "B", extra=labels(category=1, percent=1, position="bestFit"))}'
     '<c:firstSliceAng val="0"/></c:pieChart>'),
    ("Column labels", "", f'<c:barChart><c:barDir val="col"/><c:grouping val="clustered"/><c:varyColors val="0"/>'
     f'{ser(0, "B", extra=labels(series=1, value=1, separator="; ", points=('<c:dLbl><c:idx val="1"/><c:showLegendKey val="0"/><c:showVal val="1"/><c:showCatName val="1"/><c:showSerName val="0"/><c:showPercent val="0"/><c:showBubbleSize val="0"/><c:separator>: </c:separator></c:dLbl><c:dLbl><c:idx val="2"/><c:delete val="1"/></c:dLbl>')))}<c:gapWidth val="150"/>{ids}</c:barChart>{cat_val_axes}'),
    ("Surface", view3d, f'<c:surface3DChart><c:wireframe val="0"/>{ser(0, "B")}{ser(1, "C")}{ser(2, "D")}<c:bandFmts/>{ids}<c:axId val="3"/></c:surface3DChart>'
     f'{cat_val_axes}<c:serAx><c:axId val="3"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="b"/>'
     '<c:majorTickMark val="out"/><c:minorTickMark val="none"/><c:tickLblPos val="nextTo"/><c:crossAx val="2"/><c:crosses val="autoZero"/></c:serAx>'),
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
        f'<xdr:twoCellAnchor{" editAs=\"oneCell\"" if title == "Column sd" else ""}><xdr:from><xdr:col>{col}</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>{row}</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from>'
        f'<xdr:to><xdr:col>{col + 7}</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>{row + 14}</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to>'
        f'<xdr:graphicFrame macro=""><xdr:nvGraphicFramePr><xdr:cNvPr id="{n + 2}" name="{title}"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr>'
        '<xdr:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/></xdr:xfrm>'
        f'<a:graphic><a:graphicData uri="{C}"><c:chart xmlns:c="{C}" r:id="rId{n + 1}"/></a:graphicData></a:graphic></xdr:graphicFrame><xdr:clientData/></xdr:twoCellAnchor>')
CX = "http://schemas.microsoft.com/office/drawing/2014/chartex"
CX_REL = "http://schemas.microsoft.com/office/2014/relationships/chartEx"
MC = "http://schemas.openxmlformats.org/markup-compatibility/2006"
# (title, layoutId, values column, extra layout properties, the cx namespace version Excel requires for it)
chartex = [
    ("Waterfall", "waterfall", "F", '<cx:layoutPr><cx:subtotals><cx:idx val="4"/></cx:subtotals></cx:layoutPr>', ("cx1", "http://schemas.microsoft.com/office/drawing/2015/9/8/chartex")),
    ("Funnel", "funnel", "B", "", ("cx2", "http://schemas.microsoft.com/office/drawing/2015/10/21/chartex")),
    ("Treemap", "treemap", "B^", '<cx:layoutPr><cx:parentLabelLayout val="overlapping"/></cx:layoutPr>', ("cx1", "http://schemas.microsoft.com/office/drawing/2015/9/8/chartex")),
    ("Histogram", "clusteredColumn", "C!", '<cx:layoutPr><cx:binning intervalClosed="r"><cx:binSize val="3"/></cx:binning></cx:layoutPr>', ("cx1", "http://schemas.microsoft.com/office/drawing/2015/9/8/chartex")),
    ("Sunburst", "sunburst", "C^", "", ("cx1", "http://schemas.microsoft.com/office/drawing/2015/9/8/chartex")),
    ("Pareto", "clusteredColumn", "C!", '<cx:layoutPr><cx:binning intervalClosed="r"><cx:binSize val="3"/></cx:binning></cx:layoutPr></cx:series><cx:series layoutId="paretoLine" ownerIdx="0" uniqueId="{00000000-0000-4000-8000-0000000000A1}"><cx:axisId val="1"/>', ("cx1", "http://schemas.microsoft.com/office/drawing/2015/9/8/chartex")),
    ("Box", "boxWhisker", "B:C", '<cx:layoutPr><cx:visibility meanLine="0" meanMarker="1" nonoutliers="0" outliers="1"/><cx:statistics quartileMethod="exclusive"/></cx:layoutPr>', ("cx1", "http://schemas.microsoft.com/office/drawing/2015/9/8/chartex")),
]


def chartex_xml(title, layout, col, layout_pr):
    pts = "".join(f'<cx:pt idx="{i}">{row[0]}</cx:pt>' for i, row in enumerate(data))
    if col.endswith("^"):
        # Two category levels: the days (leaves) and their parents, named at the first of their rows.
        col = col[:-1]
        vals = "".join(f'<cx:pt idx="{i}">{row[cols.index(col)]}</cx:pt>' for i, row in enumerate(data))
        dims = (f'<cx:strDim type="cat"><cx:f>Kinds!$A$2:$A$6</cx:f><cx:lvl ptCount="5">{pts}</cx:lvl>'
                '<cx:lvl ptCount="5"><cx:pt idx="0">Early</cx:pt><cx:pt idx="2">Late</cx:pt></cx:lvl></cx:strDim>'
                f'<cx:numDim type="val"><cx:f>Kinds!${col}$2:${col}$6</cx:f><cx:lvl ptCount="5" formatCode="General">{vals}</cx:lvl></cx:numDim>')
    elif col.endswith("!"):
        # One column's values without categories (a histogram counts them).
        col = col[:-1]
        values = [row[cols.index(col)] for row in data]
        vals = "".join(f'<cx:pt idx="{i}">{v}</cx:pt>' for i, v in enumerate(values))
        dims = f'<cx:numDim type="val"><cx:f>Kinds!${col}$2:${col}$6</cx:f><cx:lvl ptCount="{len(values)}" formatCode="General">{vals}</cx:lvl></cx:numDim>'
    elif ":" in col:
        # Two columns as one series without categories (a box and whisker over all their values).
        a, b = col.split(":")
        values = [row[cols.index(c)] for c in (a, b) for row in data]
        vals = "".join(f'<cx:pt idx="{i}">{v}</cx:pt>' for i, v in enumerate(values))
        dims = f'<cx:numDim type="val"><cx:f>Kinds!${a}$2:${b}$6</cx:f><cx:lvl ptCount="{len(values)}" formatCode="General">{vals}</cx:lvl></cx:numDim>'
        col = a
    else:
        vals = "".join(f'<cx:pt idx="{i}">{row[cols.index(col)]}</cx:pt>' for i, row in enumerate(data))
        dims = (f'<cx:strDim type="cat"><cx:f>Kinds!$A$2:$A$6</cx:f><cx:lvl ptCount="5">{pts}</cx:lvl></cx:strDim>'
                f'<cx:numDim type="val"><cx:f>Kinds!${col}$2:${col}$6</cx:f><cx:lvl ptCount="5" formatCode="General">{vals}</cx:lvl></cx:numDim>')
    return ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
            f'<cx:chartSpace xmlns:a="{A}" xmlns:r="{R}" xmlns:cx="{CX}"><cx:chartData><cx:data id="0">'
            f'{dims}'
            '</cx:data></cx:chartData><cx:chart>'
            f'<cx:title pos="t" align="ctr" overlay="0"><cx:tx><cx:txData><cx:v>{title}</cx:v></cx:txData></cx:tx></cx:title>'
            f'<cx:plotArea><cx:plotAreaRegion><cx:series layoutId="{layout}" uniqueId="{{00000000-0000-4000-8000-00000000000{len(title) % 10}}}">'
            f'<cx:tx><cx:txData><cx:f>Kinds!${col}$1</cx:f><cx:v>{header[cols.index(col)]}</cx:v></cx:txData></cx:tx>'
            f'<cx:dataId val="0"/>{layout_pr}</cx:series></cx:plotAreaRegion></cx:plotArea>'
            '<cx:legend pos="t" align="ctr" overlay="0"/></cx:chart></cx:chartSpace>')


base = len(charts)
for k, (title, layout, col, layout_pr, (prefix, ns)) in enumerate(chartex):
    n = base + k
    col_, row = 6 + (n % 2) * 8, (n // 2) * 15
    place = (f'<xdr:from><xdr:col>{col_}</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>{row}</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from>'
             f'<xdr:to><xdr:col>{col_ + 7}</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>{row + 14}</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to>')
    # Excel requires the block of alternatives INSIDE the anchor, around the graphic frame, after `xdr:to`
    # (dw2 Windows item 5: wrapping whole anchors makes Excel refuse the workbook).
    anchors.append(
        f'<xdr:twoCellAnchor>{place}<mc:AlternateContent xmlns:mc="{MC}"><mc:Choice xmlns:{prefix}="{ns}" Requires="{prefix}">'
        f'<xdr:graphicFrame macro=""><xdr:nvGraphicFramePr><xdr:cNvPr id="{n + 2}" name="{title}"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr>'
        '<xdr:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/></xdr:xfrm>'
        f'<a:graphic><a:graphicData uri="{CX}"><cx:chart xmlns:cx="{CX}" r:id="rId{n + 1}"/></a:graphicData></a:graphic></xdr:graphicFrame>'
        '</mc:Choice><mc:Fallback>'
        '<xdr:sp macro="" textlink=""><xdr:nvSpPr><xdr:cNvPr id="0" name=""/><xdr:cNvSpPr><a:spLocks noTextEdit="1"/></xdr:cNvSpPr></xdr:nvSpPr>'
        '<xdr:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="4572000" cy="2743200"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom>'
        '<a:solidFill><a:prstClr val="white"/></a:solidFill><a:ln w="1"><a:solidFill><a:prstClr val="green"/></a:solidFill></a:ln></xdr:spPr>'
        '<xdr:txBody><a:bodyPr vertOverflow="clip" horzOverflow="clip"/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US" sz="1100"/>'
        '<a:t>This chart isn\'t available in your version of Excel.</a:t></a:r></a:p></xdr:txBody></xdr:sp>'
        '</mc:Fallback></mc:AlternateContent><xdr:clientData/></xdr:twoCellAnchor>')
drawing = (f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<xdr:wsDr xmlns:xdr="{XDR}" xmlns:a="{A}" xmlns:r="{R}">' + "".join(anchors) + "</xdr:wsDr>")
drawing_rels = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
                + "".join(f'<Relationship Id="rId{n + 1}" Type="{R}/chart" Target="../charts/chart{n + 1}.xml"/>' for n in range(len(charts)))
                + "".join(f'<Relationship Id="rId{base + k + 1}" Type="{CX_REL}" Target="../charts/chartEx{k + 1}.xml"/>' for k in range(len(chartex)))
                + "</Relationships>")
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
        + "".join(f'<Override PartName="/xl/charts/chart{n + 1}.xml" ContentType="{CT}.drawingml.chart+xml"/>' for n in range(len(charts)))
        + "".join(f'<Override PartName="/xl/charts/chartEx{k + 1}.xml" ContentType="application/vnd.ms-office.chartex+xml"/>'
                  f'<Override PartName="/xl/charts/style{k + 1}.xml" ContentType="application/vnd.ms-office.chartstyle+xml"/>'
                  f'<Override PartName="/xl/charts/colors{k + 1}.xml" ContentType="application/vnd.ms-office.chartcolorstyle+xml"/>' for k in range(len(chartex))) + '</Types>',
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
# Each Excel 2016 chart part names a style part and a colour-style part (dw2 Windows item 5: Excel refuses the workbook
# without them); these are the ones Excel wrote for a chart of its own (chartex-style/).
here = os.path.dirname(os.path.abspath(__file__))
style_xml = open(os.path.join(here, "chartex-style", "style.xml"), "rb").read()
colors_xml = open(os.path.join(here, "chartex-style", "colors.xml"), "rb").read()
for k, (title, layout, col, layout_pr, _) in enumerate(chartex):
    files[f"xl/charts/chartEx{k + 1}.xml"] = chartex_xml(title, layout, col, layout_pr)
    files[f"xl/charts/style{k + 1}.xml"] = style_xml
    files[f"xl/charts/colors{k + 1}.xml"] = colors_xml
    files[f"xl/charts/_rels/chartEx{k + 1}.xml.rels"] = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        f'<Relationship Id="rId2" Type="http://schemas.microsoft.com/office/2011/relationships/chartColorStyle" Target="colors{k + 1}.xml"/>'
        f'<Relationship Id="rId1" Type="http://schemas.microsoft.com/office/2011/relationships/chartStyle" Target="style{k + 1}.xml"/></Relationships>')
with zipfile.ZipFile(sys.argv[2], "w", zipfile.ZIP_DEFLATED) as z:
    for name_, content in files.items():
        info = zipfile.ZipInfo(name_, date_time=(2026, 11, 20, 0, 0, 0))
        info.compress_type = zipfile.ZIP_DEFLATED
        z.writestr(info, content)
