"""Check that a VBA case's workbook is exactly what CalcDoc builds from the case's sheets and modules.

The Touchstone VBA policy approves module source. This checks the .xlsm that Excel will open against it
(rw14 finding 3): the package parts are the blank macro-enabled workbook's, vbaProject.bin is a compound
file of exactly PROJECT, PROJECTwm and VBA/{_VBA_PROJECT, dir, one stream per module}, each module stream
decompresses to the authored source and nothing else (no compiled cache: the text offset is 0 and
_VBA_PROJECT carries no p-code), and no part is an XLM macro sheet, an external link or a relationship that
leaves the package. engines/excel/VbaPackage.cs is the same check for the worker; keep them in step with
crates/calc-doc/src/vba_write.rs.
"""
import io
import re
import struct
import zipfile
from xml.etree import ElementTree as ET

MAX_PROJECT = 8 * 1024 * 1024
MAX_PART = 64 * 1024
END, NONE = 0xFFFFFFFE, 0xFFFFFFFF
PROJECT_NAME = "VBAProject"
PROJECT_ID = "{CB4A6FD3-4DB2-4698-915C-D69261AB7550}"

CONTENT_TYPES = {
    "/xl/workbook.xml": "application/vnd.ms-excel.sheet.macroEnabled.main+xml",
    "/xl/styles.xml": "application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml",
    "/xl/sharedStrings.xml": "application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml",
    "/xl/vbaProject.bin": "application/vnd.ms-office.vbaProject",
}
SHEET_TYPE = "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"
DEFAULTS = {"rels": "application/vnd.openxmlformats-package.relationships+xml", "xml": "application/xml"}
REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/"
RELATIONSHIP_TYPES = {
    REL + "worksheet": None, REL + "styles": "styles.xml", REL + "sharedStrings": "sharedStrings.xml",
    "http://schemas.microsoft.com/office/2006/relationships/vbaProject": "vbaProject.bin",
}
XML_HEAD = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
STYLES = XML_HEAD + (
    '<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="1"><font>'
    '<sz val="11"/><color auto="1"/><name val="Calibri"/><family val="2"/><scheme val="minor"/></font></fonts>'
    '<fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/>'
    '</fill></fills><borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders>'
    '<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs>'
    '<cellXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/></cellXfs>'
    '<cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles></styleSheet>')
SHARED_STRINGS = XML_HEAD + (
    '<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="0" uniqueCount="0"/>')


def _sheet_xml(index):
    return XML_HEAD + ('<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">'
                       f'<sheetPr codeName="Sheet{index}"/><sheetData/></worksheet>')


def _refused(detail):
    return ValueError("invalid VBA workbook: " + detail)


def verify_vba_workbook(data, case):
    """Raise ValueError unless `data` is the package CalcDoc builds from case['sheets'] and case['modules']."""
    sheets = [sheet["name"] for sheet in case["sheets"]]
    modules = _project_modules(case, len(sheets))
    parts = _read_package(data, len(sheets))
    _check_package_parts(parts, sheets)
    _check_project(parts["xl/vbaProject.bin"], modules)


def _read_package(data, sheet_count):
    try:
        archive = zipfile.ZipFile(io.BytesIO(data))
    except zipfile.BadZipFile as error:
        raise _refused("not a package") from error
    expected = {"[Content_Types].xml", "_rels/.rels", "xl/workbook.xml", "xl/_rels/workbook.xml.rels",
                "xl/styles.xml", "xl/sharedStrings.xml", "xl/vbaProject.bin",
                *(f"xl/worksheets/sheet{i}.xml" for i in range(1, sheet_count + 1))}
    infos = archive.infolist()
    if sorted(info.filename for info in infos) != sorted(expected):
        raise _refused("unexpected package parts")
    parts = {}
    for info in infos:
        limit = MAX_PROJECT if info.filename == "xl/vbaProject.bin" else MAX_PART
        if info.file_size > limit or info.flag_bits & 1:
            raise _refused("oversized or encrypted package part")
        with archive.open(info) as stream:
            body = stream.read(limit + 1)
        if len(body) != info.file_size:
            raise _refused("package part size")
        parts[info.filename] = body
    return parts


def _xml(body, what):
    text = body.decode("utf-8")
    if "<!DOCTYPE" in text or "<!ENTITY" in text:
        raise _refused(what + " declares entities")
    return ET.fromstring(text)


def _local(tag):
    return tag.rpartition("}")[2]


def _check_package_parts(parts, sheets):
    count = len(sheets)
    if parts["xl/styles.xml"] != STYLES.encode() or parts["xl/sharedStrings.xml"] != SHARED_STRINGS.encode():
        raise _refused("styles or shared strings differ from the blank workbook")
    for index in range(1, count + 1):
        if parts[f"xl/worksheets/sheet{index}.xml"] != _sheet_xml(index).encode():
            raise _refused("a worksheet is not an empty sheet")
    present = {"/" + name for name in parts}
    types = _xml(parts["[Content_Types].xml"], "content types")
    seen = set()
    for element in types:
        name = _local(element.tag)
        if name == "Default" and set(element.attrib) == {"Extension", "ContentType"}:
            if DEFAULTS.get(element.get("Extension")) != element.get("ContentType"):
                raise _refused("unexpected default content type")
        elif name == "Override" and set(element.attrib) == {"PartName", "ContentType"}:
            part, kind = element.get("PartName"), element.get("ContentType")
            wanted = CONTENT_TYPES.get(part) or (
                SHEET_TYPE if re.fullmatch(r"/xl/worksheets/sheet[1-9][0-9]*\.xml", part) else None)
            if kind != wanted or part not in present or part in seen:
                raise _refused("unexpected content type override")
            seen.add(part)
        else:
            raise _refused("unexpected content types entry")
    if seen != present - {"/[Content_Types].xml", "/_rels/.rels", "/xl/_rels/workbook.xml.rels"}:
        raise _refused("content types do not cover the package")
    root_rels = list(_xml(parts["_rels/.rels"], "relationships"))
    if (len(root_rels) != 1 or set(root_rels[0].attrib) != {"Id", "Type", "Target"}
            or root_rels[0].get("Type") != REL + "officeDocument" or root_rels[0].get("Target") != "xl/workbook.xml"):
        raise _refused("unexpected package relationships")
    targets = {}
    for element in _xml(parts["xl/_rels/workbook.xml.rels"], "relationships"):
        kind, target = element.get("Type"), element.get("Target")
        fixed = RELATIONSHIP_TYPES.get(kind, False)
        if (_local(element.tag) != "Relationship" or set(element.attrib) != {"Id", "Type", "Target"}
                or fixed is False or fixed is not None and target != fixed
                or fixed is None and not re.fullmatch(r"worksheets/sheet[1-9][0-9]*\.xml", target)
                or "xl/" + target not in parts or element.get("Id") in targets):
            raise _refused("unexpected workbook relationship")
        targets[element.get("Id")] = (kind, target)
    if sorted(t for k, t in targets.values() if k.endswith("/worksheet")) != sorted(
            f"worksheets/sheet{i}.xml" for i in range(1, count + 1)):
        raise _refused("worksheet relationships do not match the sheets")
    if sum(1 for kind, _ in targets.values() if kind.endswith("/vbaProject")) != 1:
        raise _refused("one vbaProject relationship is required")
    workbook = _xml(parts["xl/workbook.xml"], "workbook")
    if (_local(workbook.tag) != "workbook" or [_local(e.tag) for e in workbook] != ["workbookPr", "sheets", "calcPr"]
            or workbook[0].attrib != {"codeName": "ThisWorkbook"} or set(workbook[2].attrib) - {"calcId"}
            or len(workbook[1]) != count):
        raise _refused("unexpected workbook.xml")
    rid = "{" + REL[:-1] + "}id"
    for index, (element, name) in enumerate(zip(workbook[1], sheets), 1):
        kind, target = targets.get(element.get(rid), (None, None))
        if (_local(element.tag) != "sheet" or set(element.attrib) != {"name", "sheetId", rid}
                or element.get("name") != name or element.get("sheetId") != str(index)
                or target != f"worksheets/sheet{index}.xml"):
            raise _refused("workbook sheets do not match the case")


# --- the project, rebuilt from the modules -------------------------------------------------------

def _project_modules(case, sheet_count):
    """(name, kind, source) in CalcDoc's project order: the case's modules, then missing documents."""
    modules = []
    for module in case["modules"]:
        kind = ("Standard" if module["kind"] == "standard"
                else "Workbook" if module["name"] == "ThisWorkbook" else "Worksheet")
        modules.append((module["name"], kind, module["source"]))
    documents = [("Workbook", "ThisWorkbook")] + [("Worksheet", f"Sheet{i}") for i in range(1, sheet_count + 1)]
    for kind, name in documents:
        if not any(m[1] == kind and m[0] == name for m in modules):
            modules.append((name, kind, ""))
    for name, kind, _ in modules:
        if kind != "Standard" and (kind, name) not in documents:
            raise _refused("a document module does not name ThisWorkbook or a sheet code name")
    return modules


def _encoded_source(name, kind, source):
    normalized = source.replace("\r\n", "\n").replace("\r", "\n")
    text = f'Attribute VB_Name = "{name}"\r\n'
    base = {"Standard": None, "Class": "FCFB3D2A-A0FA-1068-A738-08002B3371B5",
            "Workbook": "00020819-0000-0000-C000-000000000046",
            "Worksheet": "00020820-0000-0000-C000-000000000046"}[kind]
    if base:
        flag = "False" if kind == "Class" else "True"
        text += (f'Attribute VB_Base = "0{{{base}}}"\r\nAttribute VB_GlobalNameSpace = False\r\n'
                 f"Attribute VB_Creatable = False\r\nAttribute VB_PredeclaredId = {flag}\r\n"
                 f"Attribute VB_Exposed = {flag}\r\nAttribute VB_TemplateDerived = False\r\n"
                 f"Attribute VB_Customizable = {flag}\r\n")
    try:
        return (text + normalized.replace("\n", "\r\n")).encode("cp1252")
    except UnicodeEncodeError as error:
        raise _refused("module source is not Windows-1252") from error


def _record(dir_, record_id, body=b""):
    dir_ += struct.pack("<HI", record_id, len(body)) + body


def _utf16(text):
    return text.encode("utf-16-le")


def _directory(modules):
    d = bytearray()
    _record(d, 0x01, struct.pack("<I", 3))
    _record(d, 0x4A, struct.pack("<I", 6))
    _record(d, 0x02, struct.pack("<I", 0x0409))
    _record(d, 0x14, struct.pack("<I", 0x0409))
    _record(d, 0x03, struct.pack("<H", 1252))
    _record(d, 0x04, PROJECT_NAME.encode())
    for record_id in (0x05, 0x40, 0x06, 0x3D):
        _record(d, record_id)
    _record(d, 0x07, bytes(4))
    _record(d, 0x08, bytes(4))
    _record(d, 0x09, struct.pack("<I", 1))
    d += struct.pack("<H", 0)
    _record(d, 0x0C)
    _record(d, 0x3C)
    for name, libid in (
        ("stdole", r"*\G{00020430-0000-0000-C000-000000000046}#2.0#0#C:\Windows\System32\stdole2.tlb#OLE Automation"),
        ("Office", r"*\G{2DF8D04C-5BFA-101B-BDE5-00AA0044DE52}#2.0#0#C:\Program Files\Common Files\Microsoft Shared"
                   r"\OFFICE16\MSO.DLL#Microsoft Office 16.0 Object Library"),
    ):
        _record(d, 0x16, name.encode())
        _record(d, 0x3E, _utf16(name))
        _record(d, 0x0D, struct.pack("<I", len(libid)) + libid.encode() + bytes(6))
    _record(d, 0x0F, struct.pack("<H", len(modules)))
    _record(d, 0x13, bytes(2))
    for name, kind, _ in modules:
        _record(d, 0x19, name.encode())
        _record(d, 0x47, _utf16(name))
        _record(d, 0x1A, name.encode())
        _record(d, 0x32, _utf16(name))
        _record(d, 0x1C)
        _record(d, 0x48)
        _record(d, 0x31, bytes(4))
        _record(d, 0x1E, bytes(4))
        _record(d, 0x2C, bytes(2))
        _record(d, 0x21 if kind == "Standard" else 0x22)
        _record(d, 0x2B)
    _record(d, 0x10)
    return bytes(d)


def _project_property(data):
    key = sum(PROJECT_ID.encode()) & 0xFF
    previous, earlier, unencoded = key, 2, key
    out = "00%02X%02X" % (earlier, previous)
    for byte in struct.pack("<I", len(data)) + data:
        encoded = byte ^ ((earlier + unencoded) & 0xFF)
        out += "%02X" % encoded
        earlier, previous, unencoded = previous, encoded, byte
    return out


def _project_stream(modules):
    text = f'ID="{PROJECT_ID}"\r\n'
    for name, kind, _ in modules:
        prop, suffix = {"Standard": ("Module", ""), "Class": ("Class", ""),
                        "Workbook": ("Document", "/&H00000000"), "Worksheet": ("Document", "/&H00000000")}[kind]
        text += f"{prop}={name}{suffix}\r\n"
    text += f'Name="{PROJECT_NAME}"\r\nHelpContextID="0"\r\nVersionCompatible32="393222000"\r\n'
    for prop, data in (("CMG", bytes(4)), ("DPB", b"\x00"), ("GC", b"\xff")):
        text += f'{prop}="{_project_property(data)}"\r\n'
    text += "\r\n[Host Extender Info]\r\n&H00000001={3832D640-CF90-11CF-8E43-00A0C911005A};VBE;&H00000000\r\n"
    return text.encode("ascii")


def _name_map(modules):
    out = b""
    for name, _, _ in modules:
        out += name.encode() + b"\x00" + _utf16(name) + bytes(2)
    return out + bytes(2)


def _check_project(blob, modules):
    streams = _read_compound_file(blob)
    expected = {"PROJECT": _project_stream(modules), "PROJECTwm": _name_map(modules),
                "VBA/_VBA_PROJECT": bytes([0xCC, 0x61, 0xFF, 0xFF, 0, 1, 0])}
    decompressed = {"VBA/dir": _directory(modules)}
    for name, kind, source in modules:
        decompressed["VBA/" + name] = _encoded_source(name, kind, source)
    if set(streams) != set(expected) | set(decompressed):
        raise _refused("vbaProject.bin has unexpected or missing streams")
    for path, body in expected.items():
        if streams[path] != body:
            raise _refused(f"vbaProject.bin stream {path} differs from the authored project")
    for path, body in decompressed.items():
        if decompress_vba_stream(streams[path]) != body:
            raise _refused(f"vbaProject.bin stream {path} differs from the authored source")


def decompress_vba_stream(data):
    """MS-OVBA 2.4.1.3.1, refusing anything malformed."""
    if not data or data[0] != 1:
        raise _refused("bad compressed container")
    out, at = bytearray(), 1
    while at < len(data):
        if at + 2 > len(data):
            raise _refused("truncated compressed chunk")
        header = data[at] | data[at + 1] << 8
        end = at + (header & 0xFFF) + 3
        if header >> 12 & 7 != 3 or end > len(data):
            raise _refused("bad compressed chunk")
        if not header & 0x8000:
            if end - at != 4098:
                raise _refused("bad raw chunk")
            out += data[at + 2:end]
        else:
            start, pos = len(out), at + 2
            while pos < end:
                flags = data[pos]
                pos += 1
                for bit in range(8):
                    if pos >= end:
                        break
                    if not flags >> bit & 1:
                        out.append(data[pos])
                        pos += 1
                        continue
                    if pos + 2 > end:
                        raise _refused("truncated copy token")
                    token = data[pos] | data[pos + 1] << 8
                    pos += 2
                    distance = len(out) - start
                    bits = max((distance - 1).bit_length(), 4) if distance else 4
                    length = (token & (0xFFFF >> bits)) + 3
                    offset = (token >> (16 - bits)) + 1
                    if offset > distance:
                        raise _refused("bad copy token")
                    for _ in range(length):
                        out.append(out[-offset])
        at = end
        if len(out) > MAX_PROJECT:
            raise _refused("oversized module stream")
    return bytes(out)


def _read_compound_file(blob):
    if len(blob) < 1536 or len(blob) > MAX_PROJECT or blob[:8] != bytes.fromhex("d0cf11e0a1b11ae1"):
        raise _refused("vbaProject.bin is not a compound file")
    major, byte_order, shift, mini_shift = struct.unpack_from("<HHHH", blob, 26)
    if (major, byte_order, shift, mini_shift) != (3, 0xFFFE, 9, 6):
        raise _refused("unsupported compound file layout")
    fat_count, dir_start, _, cutoff, mini_start, mini_count, difat_start, difat_count = struct.unpack_from(
        "<IIIIIIII", blob, 44)
    sector = 512
    sectors = (len(blob) - 512) // sector

    def sector_bytes(index):
        if index >= sectors:
            raise _refused("sector out of range")
        return blob[512 + index * sector:512 + (index + 1) * sector]

    difat = list(struct.unpack_from("<109I", blob, 76))
    chain, seen = difat_start, set()
    for _ in range(difat_count):
        if chain in seen or chain >= sectors:
            raise _refused("bad DIFAT")
        seen.add(chain)
        entries = struct.unpack("<128I", sector_bytes(chain))
        difat += entries[:127]
        chain = entries[127]
    fat = []
    for index in difat[:fat_count]:
        fat += struct.unpack("<128I", sector_bytes(index))

    def chain_of(start, table):
        out, seen = [], set()
        while start != END:
            if start >= len(table) or start in seen:
                raise _refused("bad sector chain")
            seen.add(start)
            out.append(start)
            start = table[start]
        return out

    def read_chain(start, size):
        body = b"".join(sector_bytes(i) for i in chain_of(start, fat))
        if len(body) < size:
            raise _refused("short stream")
        return body[:size]

    directory = read_chain(dir_start, len(chain_of(dir_start, fat)) * sector)
    entries = []
    for at in range(0, len(directory), 128):
        raw = directory[at:at + 128]
        length = struct.unpack_from("<H", raw, 64)[0]
        kind = raw[66]
        left, right, child = struct.unpack_from("<III", raw, 68)
        start = struct.unpack_from("<I", raw, 116)[0]
        size = struct.unpack_from("<Q", raw, 120)[0]
        if kind and (length < 2 or length > 64 or length % 2):
            raise _refused("bad directory entry name")
        name = raw[:max(length - 2, 0)].decode("utf-16-le", "strict") if kind else ""
        entries.append((name, kind, left, right, child, start, size))
    if not entries or entries[0][1] != 5:
        raise _refused("no root entry")
    mini_fat = []
    if mini_count:
        body = read_chain(mini_start, len(chain_of(mini_start, fat)) * sector)
        mini_fat = list(struct.unpack(f"<{len(body) // 4}I", body))
    mini_stream = read_chain(entries[0][5], entries[0][6]) if entries[0][6] else b""

    def stream(entry):
        size, start = entry[6], entry[5]
        if size >= cutoff:
            return read_chain(start, size)
        body = b"".join(mini_stream[i * 64:(i + 1) * 64] for i in chain_of(start, mini_fat))
        if len(body) < size:
            raise _refused("short mini stream")
        return body[:size]

    found, visited = {}, set()

    def walk(index, prefix):
        stack = [index]
        while stack:
            at = stack.pop()
            if at == NONE:
                continue
            if at >= len(entries) or at in visited:
                raise _refused("bad directory tree")
            visited.add(at)
            name, kind, left, right, child, _, _ = entries[at]
            path = prefix + name
            if kind == 2:
                if path in found:
                    raise _refused("duplicate stream")
                found[path] = stream(entries[at])
            elif kind == 1:
                walk(child, path + "/")
            else:
                raise _refused("unexpected directory entry")
            stack += [left, right]

    visited.add(0)
    walk(entries[0][4], "")
    if sum(1 for entry in entries if entry[1]) != len(visited):
        raise _refused("unreachable directory entries")
    return found
