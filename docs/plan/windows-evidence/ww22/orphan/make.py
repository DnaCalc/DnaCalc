from pathlib import Path
import hashlib
import json
import posixpath
import re
import xml.etree.ElementTree as ET
import zipfile

root=Path(__file__).parent
src=root.parent/"projects/delete-before.xlsm"
dest=root/"orphan.xlsm"
rns="http://schemas.openxmlformats.org/officeDocument/2006/relationships"

def remove_tag(xml, tag, attr, value):
    pattern=rb"<"+tag+rb"\b[^>]*\b"+attr+rb'="'+re.escape(value)+rb'"[^>]*/>'
    updated, count=re.subn(pattern,b"",xml)
    if count!=1:
        raise ValueError((tag,attr,value,count))
    return updated

with zipfile.ZipFile(src) as source:
    workbook=source.read("xl/workbook.xml")
    sheet=next(s for s in ET.fromstring(workbook).find("{*}sheets") if s.attrib["name"]=="Middle")
    relationship=sheet.attrib[f"{{{rns}}}id"]
    rels=source.read("xl/_rels/workbook.xml.rels")
    rel=next(el for el in ET.fromstring(rels) if el.attrib["Id"]==relationship)
    target=rel.attrib["Target"]
    part=target.lstrip("/") if target.startswith("/") else posixpath.normpath(posixpath.join("xl",target))
    types=source.read("[Content_Types].xml")
    workbook=remove_tag(workbook,b"sheet",b"name",b"Middle")
    workbook=re.sub(rb'(<workbookView\b[^>]*\bactiveTab=")\d+(")',rb'\g<1>0\2',workbook)
    patches={
        "xl/workbook.xml":workbook,
        "xl/_rels/workbook.xml.rels":remove_tag(rels,b"Relationship",b"Id",relationship.encode()),
        "[Content_Types].xml":remove_tag(types,b"Override",b"PartName",("/"+part).encode()),
    }
    with zipfile.ZipFile(dest,"w",compression=zipfile.ZIP_DEFLATED) as modified:
        for name in source.namelist():
            if name!=part:
                modified.writestr(name,patches.get(name,source.read(name)))
    with zipfile.ZipFile(dest) as modified:
        assert source.read("xl/vbaProject.bin")==modified.read("xl/vbaProject.bin")
(root/"input.json").write_text(json.dumps(dict(source=src.name,removed=part,excel_sha256=hashlib.sha256(src.read_bytes()).hexdigest(),input_sha256=hashlib.sha256(dest.read_bytes()).hexdigest()),indent=2)+"\n")
