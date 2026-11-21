#!/usr/bin/env python3
"""Write the deterministic W1 OOXML workload. Standard library only."""
import argparse
import hashlib
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile, ZipInfo

MAIN = "http://schemas.openxmlformats.org/spreadsheetml/2006/main"
REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
PACKAGE = "http://schemas.openxmlformats.org/package/2006/relationships"
CONTENT = "http://schemas.openxmlformats.org/package/2006/content-types"
XLSX = "application/vnd.openxmlformats-officedocument.spreadsheetml"

STYLES = f"""<styleSheet xmlns="{MAIN}">
  <numFmts count="1"><numFmt numFmtId="164" formatCode="0.000 &quot;kg&quot;"/></numFmts>
  <fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts>
  <fills count="3"><fill><patternFill patternType="none"/></fill>
  <fill><patternFill patternType="gray125"/></fill>
  <fill><patternFill patternType="solid"><fgColor rgb="FFFFECC2"/><bgColor indexed="64"/></patternFill></fill></fills>
  <borders count="2"><border><left/><right/><top/><bottom/><diagonal/></border>
  <border><left/><right/><top/><bottom style="thin"><color rgb="FF286A73"/></bottom><diagonal/></border></borders>
  <cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs>
  <cellXfs count="5">
    <xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/>
    <xf numFmtId="14" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/>
    <xf numFmtId="164" fontId="0" fillId="2" borderId="1" xfId="0"
        applyNumberFormat="1" applyFill="1" applyBorder="1"/>
    <xf numFmtId="10" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/>
    <xf numFmtId="0" fontId="0" fillId="2" borderId="1" xfId="0" applyFill="1" applyBorder="1"/>
  </cellXfs>
  <cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles>
</styleSheet>"""


def column(number):
    result = ""
    while number:
        number, remainder = divmod(number - 1, 26)
        result = chr(65 + remainder) + result
    return result


def cell(row, col, name):
    ref = f"{name}{row}"
    if ref == "A1":
        return '<c r="A1"><v>1</v></c>'
    if ref == "AH1":
        return '<c r="AH1" s="4" t="inlineStr"><is><t>Merged label</t></is></c>'
    if 2 <= row <= 501 and 2 <= col <= 21:
        offset = (row - 2) * 20 + col - 1
        return f'<c r="{ref}"><f>$A$1+{offset}</f><v>{1 + offset}</v></c>'
    kind = (row + col) % 7
    if kind == 0:
        return f'<c r="{ref}" t="inlineStr"><is><t>Text {ref} 東京 café</t></is></c>'
    if kind == 1:
        return f'<c r="{ref}" s="1"><v>{45000 + row % 366}</v></c>'
    if kind == 2:
        return f'<c r="{ref}" t="b"><v>{(row + col) // 7 % 2}</v></c>'
    if kind == 3:
        return f'<c r="{ref}" t="e"><v>#N/A</v></c>'
    if kind == 4:
        return f'<c r="{ref}" s="2"><v>{row}.{col:02d}</v></c>'
    if kind == 5:
        return f'<c r="{ref}" s="3"><v>0.{col:02d}</v></c>'
    return f'<c r="{ref}"><v>{row * col}</v></c>'


def sheet():
    yield f'<worksheet xmlns="{MAIN}"><dimension ref="A1:AN2501"/>'
    yield ('<sheetViews><sheetView workbookViewId="0"><pane xSplit="1" ySplit="1" '
           'topLeftCell="B2" activePane="bottomRight" state="frozen"/>'
           '<selection pane="bottomRight" activeCell="A1" sqref="A1"/></sheetView></sheetViews>'
           '<sheetFormatPr defaultRowHeight="15"/><sheetData>')
    columns = [(number, column(number)) for number in range(1, 41)]
    for row in range(1, 2502):
        active = columns if row <= 2500 else columns[:2]
        cells = "".join(cell(row, index, name) for index, name in active
                        if row != 1 or index not in (35, 36))
        yield f'<row r="{row}">{cells}</row>'
    yield '</sheetData><mergeCells count="1"><mergeCell ref="AH1:AJ1"/></mergeCells></worksheet>'


def part(archive, name, chunks):
    info = ZipInfo(name)
    info.create_system = 3
    info.compress_type = ZIP_DEFLATED
    with archive.open(info, "w") as target:
        for chunk in chunks:
            target.write(chunk.encode("utf-8"))


def generate(path):
    with ZipFile(path, "w") as archive:
        part(archive, "[Content_Types].xml", [f"""<Types xmlns="{CONTENT}">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/xl/workbook.xml" ContentType="{XLSX}.sheet.main+xml"/>
<Override PartName="/xl/worksheets/sheet1.xml" ContentType="{XLSX}.worksheet+xml"/>
<Override PartName="/xl/styles.xml" ContentType="{XLSX}.styles+xml"/>
</Types>"""])
        part(archive, "_rels/.rels", [f"""<Relationships xmlns="{PACKAGE}">
<Relationship Id="rId1" Type="{REL}/officeDocument" Target="xl/workbook.xml"/>
</Relationships>"""])
        part(archive, "xl/workbook.xml", [f"""<workbook xmlns="{MAIN}" xmlns:r="{REL}">
<bookViews><workbookView/></bookViews>
<sheets><sheet name="W1" sheetId="1" r:id="rId1"/></sheets>
<calcPr calcMode="auto" fullCalcOnLoad="1"/></workbook>"""])
        part(archive, "xl/_rels/workbook.xml.rels", [f"""<Relationships xmlns="{PACKAGE}">
<Relationship Id="rId1" Type="{REL}/worksheet" Target="worksheets/sheet1.xml"/>
<Relationship Id="rId2" Type="{REL}/styles" Target="styles.xml"/>
</Relationships>"""])
        part(archive, "xl/styles.xml", [STYLES])
        part(archive, "xl/worksheets/sheet1.xml", sheet())
    print(f"{path} {path.stat().st_size} bytes SHA-256 {hashlib.sha256(path.read_bytes()).hexdigest()}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="Path to W1.xlsx")
    generate(parser.parse_args().output)
