#!/usr/bin/env python3
"""Static host-reach footprint of corpus VBA projects (dc-54aki).

Reads each workbook's VBA project as bytes (ZIP member or OLE2 storage), decompresses
the module streams (MS-OVBA 2.4.1) and scans the source text. Nothing is executed and
no Office program is started. No source text is written: outputs carry construct
names, ProgIDs, Declare library and symbol names, Environ names and path shapes only.

Per procedure it records the direct features, then closes them over the project's
call graph (an identifier equal to a procedure name is a call: an over-approximation),
so every procedure gets its may-reach set. docs/design/PORTABLE_EXECUTION.md section 8
describes the analysis; this script is its first measurement, not the product check.

Conditional compilation is evaluated with the Windows dialect constants (Mac False,
Win32, Win64 and VBA7 True) before scanning, so a `#If Mac` branch adds nothing; a
condition on a constant the scan does not know keeps all its branches.

Usage: footprint.py --list <file of workbook paths> --root <dir> --out <dir>
                    [--watch-members <names file>] [--watch-vba <names file>]
footprint_report.py joins the output with `dnacalc run --all --json` results.
"""
import argparse
import collections
import hashlib
import io
import json
import re
import sys
import zipfile
from pathlib import Path

import olefile


def decompress(data: bytes) -> bytes:
    """MS-OVBA 2.4.1 decompression."""
    if not data or data[0] != 1:
        raise ValueError('not a compressed container')
    out = bytearray()
    pos = 1
    n = len(data)
    while pos + 2 <= n:
        header = data[pos] | (data[pos + 1] << 8)
        pos += 2
        size = (header & 0x0FFF) + 3
        end = min(n, pos + size - 2)
        if not header & 0x8000:
            out += data[pos:pos + 4096]
            pos += 4096
            continue
        start = len(out)
        while pos < end:
            flags = data[pos]
            pos += 1
            for bit in range(8):
                if pos >= end:
                    break
                if not (flags >> bit) & 1:
                    out.append(data[pos])
                    pos += 1
                    continue
                if pos + 2 > end:
                    pos = end
                    break
                token = data[pos] | (data[pos + 1] << 8)
                pos += 2
                diff = len(out) - start
                bits = max((diff - 1).bit_length(), 4)
                length_mask = 0xFFFF >> bits
                length = (token & length_mask) + 3
                offset = ((token & ~length_mask & 0xFFFF) >> (16 - bits)) + 1
                if offset > len(out):
                    raise ValueError('copy token before start')
                for _ in range(length):
                    out.append(out[-offset])
    return bytes(out)


def parse_dir(raw: bytes):
    """Module list, code page and references from the decompressed dir stream."""
    pos = 0
    codepage = 1252
    modules = []
    refs = []
    cur = None
    n = len(raw)
    while pos + 6 <= n:
        rid = int.from_bytes(raw[pos:pos + 2], 'little')
        size = int.from_bytes(raw[pos + 2:pos + 6], 'little')
        pos += 6
        if rid == 0x0009:  # PROJECTVERSION: size field is 4, six bytes follow
            pos += 6
            continue
        body = raw[pos:pos + size]
        pos += size
        if rid == 0x0003 and size >= 2:
            codepage = int.from_bytes(body[:2], 'little')
        elif rid == 0x0016:
            refs.append(body.decode('latin-1'))
        elif rid == 0x0019:
            cur = {'name': body, 'stream': body, 'offset': 0, 'kind': 'std'}
            modules.append(cur)
        elif rid == 0x001A and cur is not None:
            cur['stream'] = body
        elif rid == 0x0031 and cur is not None and size >= 4:
            cur['offset'] = int.from_bytes(body[:4], 'little')
        elif rid == 0x0022 and cur is not None:
            cur['kind'] = 'doc-or-class'
    return codepage, modules, refs


def codec(codepage: int) -> str:
    name = {65001: 'utf-8', 10000: 'mac_roman'}.get(codepage, f'cp{codepage}')
    try:
        ''.encode(name)
        return name
    except LookupError:
        return 'cp1252'


def read_project(path: Path):
    """Returns (modules: [(name, kind, text)], references) or raises."""
    data = path.read_bytes()
    blob = None
    if data[:2] == b'PK':
        with zipfile.ZipFile(io.BytesIO(data)) as z:
            for name in z.namelist():
                if name.lower().endswith('vbaproject.bin'):
                    blob = z.read(name)
                    break
        if blob is None:
            raise ValueError('no vbaProject.bin')
    else:
        blob = data
    ole = olefile.OleFileIO(io.BytesIO(blob))
    try:
        dirs = [e for e in ole.listdir(streams=True) if len(e) >= 2 and e[-1].lower() == 'dir' and e[-2].upper() == 'VBA']
        if not dirs:
            raise ValueError('no VBA/dir stream')
        base = dirs[0][:-1]
        codepage, modules, refs = parse_dir(decompress(ole.openstream(dirs[0]).read()))
        enc = codec(codepage)
        out = []
        for m in modules:
            stream = m['stream'].decode(enc, 'replace')
            try:
                raw = ole.openstream(base + [stream]).read()
                text = decompress(raw[m['offset']:]).decode(enc, 'replace')
            except Exception:
                continue
            out.append((m['name'].decode(enc, 'replace'), m['kind'], text))
        return out, refs
    finally:
        ole.close()


STRING = re.compile(r'"(?:[^"]|"")*"')
PROC = re.compile(r'^\s*(?:(public|private|friend)\s+)?(?:static\s+)?(sub|function|property\s+(?:get|let|set))\s+([^\W\d]\w*)', re.I)
END_PROC = re.compile(r'^\s*end\s+(sub|function|property)\b', re.I)
DECLARE = re.compile(r'^\s*(?:(?:public|private)\s+)?declare\s+(ptrsafe\s+)?(?:sub|function)\s+(\w+)\s+(?:cdecl\s+)?lib\s+\x00(\d+)\x00(?:\s+alias\s+\x00(\d+)\x00)?', re.I)
IDENT = re.compile(r'[^\W\d]\w*')

# (feature, regex over a lower-cased logical line whose string literals are \x00<index>\x00)
S = r'\x00(\d+)\x00'
NOTMEMBER = r'(?<![.\w])'
PATTERNS = [
    ('file.open', re.compile(NOTMEMBER + r'open\b[^.]*?\bfor\s+(input|output|append|binary|random)\b')),
    ('file.kill', re.compile(r'(?:^|:|\bthen\s+|\belse\s+)\s*kill\b')),
    ('file.mkdir', re.compile(r'(?:^|:|\bthen\s+|\belse\s+)\s*mkdir\b')),
    ('file.rmdir', re.compile(r'(?:^|:|\bthen\s+|\belse\s+)\s*rmdir\b')),
    ('file.chdir', re.compile(r'(?:^|:|\bthen\s+|\belse\s+)\s*chdir\b')),
    ('file.chdrive', re.compile(r'(?:^|:|\bthen\s+|\belse\s+)\s*chdrive\b')),
    ('file.filecopy', re.compile(r'(?:^|:|\bthen\s+|\belse\s+)\s*filecopy\b')),
    ('file.setattr', re.compile(r'(?:^|:|\bthen\s+|\belse\s+)\s*setattr\b')),
    ('file.name-as', re.compile(r'(?:^|:|\bthen\s+|\belse\s+)\s*name\s+\S.*\s+as\s+\S')),
    ('file.dir', re.compile(NOTMEMBER + r'dir\$?\s*(\(|$)')),
    ('file.curdir', re.compile(NOTMEMBER + r'curdir\$?\b')),
    ('file.filelen', re.compile(NOTMEMBER + r'filelen\s*\(')),
    ('file.filedatetime', re.compile(NOTMEMBER + r'filedatetime\s*\(')),
    ('file.getattr', re.compile(NOTMEMBER + r'getattr\s*\(')),
    ('file.print#', re.compile(NOTMEMBER + r'(print|write)\s*#')),
    ('file.input#', re.compile(NOTMEMBER + r'(line\s+input|input)\s*#')),
    ('file.get-put#', re.compile(r'(?:^|:|\bthen\s+|\belse\s+)\s*(get|put)\s*#')),
    ('env.environ', re.compile(NOTMEMBER + r'environ\$?\s*\(')),
    ('process.shell', re.compile(r'(?<![.\w])(?:vba\.)?shell\b\s*[(\x00\w]')),
    ('ui.sendkeys', re.compile(r'\bsendkeys\b')),
    ('process.appactivate', re.compile(NOTMEMBER + r'appactivate\b')),
    ('com.createobject', re.compile(r'\bcreateobject\s*\(')),
    ('com.getobject', re.compile(r'\bgetobject\s*\(')),
    ('settings.vb', re.compile(NOTMEMBER + r'(getsetting|savesetting|deletesetting|getallsettings)\b')),
    ('ui.msgbox', re.compile(r'\bmsgbox\b')),
    ('ui.inputbox', re.compile(r'\binputbox\b')),
    ('ui.form-show', re.compile(r'\b\w+\.show\b|^\s*load\s+\w+')),
    ('wait.doevents', re.compile(r'\bdoevents\b')),
    ('wait.application-wait', re.compile(r'\bapplication\.wait\b')),
    ('wait.ontime', re.compile(r'\.ontime\b')),
    ('ui.onkey', re.compile(r'\.onkey\b')),
    ('clock.now-timer', re.compile(NOTMEMBER + r'(now|timer)\b(?!\s*=)')),
    ('ffi.addressof', re.compile(r'\baddressof\b')),
    ('ffi.pointer', re.compile(NOTMEMBER + r'(varptr|strptr|objptr)\s*\(')),
    ('mac.macscript', re.compile(r'\b(macscript|applescripttask|macid)\b')),
    ('om.workbooks-open', re.compile(r'\bworkbooks\s*\.\s*(open|opentext|openxml|opendatabase)\b')),
    ('om.saveas', re.compile(r'\.(saveas|savecopyas)\b')),
    ('om.save', re.compile(r'\.save\b')),
    ('om.file-dialog', re.compile(r'\b(getopenfilename|getsaveasfilename|filedialog)\b|\bdialogs\s*\(')),
    ('om.path', re.compile(r'\.(path|fullname)\b')),
    ('om.pathseparator', re.compile(r'\bpathseparator\b')),
    ('om.defaultfilepath', re.compile(r'\b(defaultfilepath|startuppath|templatespath|librarypath|altstartuppath)\b')),
    ('om.username', re.compile(r'\bapplication\.username\b|\busername\b(?=\s*$|\s*[&)])')),
    ('om.operatingsystem', re.compile(r'\boperatingsystem\b')),
    ('om.followhyperlink', re.compile(r'\bfollowhyperlink\b')),
    ('om.querytables', re.compile(r'\bquerytables\b|\.connections\b|\bodbcconnection\b|\boledbconnection\b')),
    ('om.printout', re.compile(r'\.(printout|printpreview)\b|\bactiveprinter\b')),
    ('om.export-pdf', re.compile(r'\bexportasfixedformat\b')),
    ('om.chart-export', re.compile(r'\.export\b')),
    ('om.sendmail', re.compile(r'\.(sendmail|sendformail)\b|\bmailenvelope\b')),
    ('om.application-run', re.compile(r'\b(application\.)?run\s*[(\x00]|\.run\s')),
    ('om.executeexcel4macro', re.compile(r'\bexecuteexcel4macro\b')),
    ('om.vbproject', re.compile(r'\b(vbproject|vbcomponents|codemodule)\b|\.vbe\b')),
    ('om.commandbars', re.compile(r'\bcommandbars\b')),
    ('om.registerxll', re.compile(r'\bregisterxll\b')),
    ('om.addins', re.compile(r'\baddins\b')),
    ('om.statusbar', re.compile(r'\bstatusbar\b')),
    ('om.picture-file', re.compile(r'\b(addpicture|loadpicture|savepicture)\b|\bpictures\s*\.\s*insert\b')),
    ('om.clipboard', re.compile(r'\b(cutcopymode|dataobject|putinclipboard|getfromclipboard)\b|\.copypicture\b')),
    ('om.chart', re.compile(r'\b(chartobjects|addchart2?|setsourcedata|seriescollection|charttype)\b|\bcharts\s*\.\s*add\b')),
    ('om.speech', re.compile(r'\.speech\b')),
]
SUB_OPEN_MODE = re.compile(r'\bfor\s+(input|output|append|binary|random)\b')
CREATE = re.compile(r'\b(createobject|getobject)\s*\(\s*(?:\x00(\d+)\x00\s*[),]|,\s*\x00(\d+)\x00)?')
ENVIRON = re.compile(r'\benviron\$?\s*\(\s*(?:\x00(\d+)\x00)?')
RUN = re.compile(r'\b(?:application\.)?run\s*\(?\s*(\x00\d+\x00)?')
AS_CLASS = re.compile(r'\b(?:as|new)\s+(?:new\s+)?([a-z_]\w*(?:\.\w+)?)')

# Class names that mean an external (non Excel, non VBA) COM library when early bound.
EARLY = {
    'dictionary': 'Scripting.Dictionary', 'scripting.dictionary': 'Scripting.Dictionary',
    'filesystemobject': 'Scripting.FileSystemObject', 'scripting.filesystemobject': 'Scripting.FileSystemObject',
    'textstream': 'Scripting.FileSystemObject', 'scripting.textstream': 'Scripting.FileSystemObject',
    'regexp': 'VBScript.RegExp', 'vbscript_regexp_55.regexp': 'VBScript.RegExp',
    'domdocument': 'MSXML2.DOMDocument', 'msxml2.domdocument': 'MSXML2.DOMDocument',
    'msxml2.domdocument60': 'MSXML2.DOMDocument', 'domdocument60': 'MSXML2.DOMDocument',
    'msxml2.xmlhttp': 'MSXML2.XMLHTTP', 'xmlhttp': 'MSXML2.XMLHTTP', 'msxml2.xmlhttp60': 'MSXML2.XMLHTTP',
    'msxml2.serverxmlhttp': 'MSXML2.ServerXMLHTTP', 'serverxmlhttp60': 'MSXML2.ServerXMLHTTP',
    'winhttp.winhttprequest': 'WinHttp.WinHttpRequest', 'winhttprequest': 'WinHttp.WinHttpRequest',
    'adodb.connection': 'ADODB.Connection', 'adodb.recordset': 'ADODB.Recordset', 'adodb.stream': 'ADODB.Stream',
    'adodb.command': 'ADODB.Command',
    'word.application': 'Word.Application', 'word.document': 'Word.Application',
    'outlook.application': 'Outlook.Application', 'outlook.mailitem': 'Outlook.Application',
    'powerpoint.application': 'PowerPoint.Application', 'access.application': 'Access.Application',
    'wshshell': 'WScript.Shell', 'iwshruntimelibrary.wshshell': 'WScript.Shell',
    'shell32.shell': 'Shell.Application', 'internetexplorer': 'InternetExplorer.Application',
    'shdocvw.internetexplorer': 'InternetExplorer.Application', 'htmldocument': 'MSHTML.HTMLDocument',
    'mshtml.htmldocument': 'MSHTML.HTMLDocument', 'msforms.dataobject': 'MSForms.DataObject',
}


def path_shape(s: str):
    """Shape classes of a string literal that looks like a path, never the literal."""
    shapes = []
    if re.match(r'^[A-Za-z]:[\\/]', s) or re.match(r'^[A-Za-z]:$', s):
        shapes.append('path.drive-letter')
    elif re.match(r'^\\\\[\w.$-]+\\', s):
        shapes.append('path.unc')
    elif re.match(r'^(https?|ftp)://', s, re.I):
        shapes.append('path.url')
    elif re.match(r'^(Macintosh HD:|/Users/|/Volumes/|/Library/|/Applications/)', s):
        shapes.append('path.mac')
    elif re.match(r'^(/home/|/tmp/|/usr/|/var/|/etc/)', s):
        shapes.append('path.posix')
    elif re.match(r'^HK(EY_|LM\\|CU\\)', s):
        shapes.append('path.registry')
    elif re.match(r'^winmgmts:', s, re.I):
        shapes.append('path.wmi-moniker')
    elif s == '\\' or (len(s) < 200 and re.search(r'\\[\w .-]+(\.\w{2,4})?$|^\\|\\$', s) and not re.search(r'\\[ntr]\b', s)):
        shapes.append('path.backslash-literal')
    return shapes


def logical_lines(text: str):
    text = text.replace('\r\n', '\n').replace('\r', '\n')
    text = re.sub(r'[ \t]_\n', ' ', text)
    return text.split('\n')


DIALECT = {'mac': False, 'win16': False, 'win32': True, 'win64': True, 'vba6': False, 'vba7': True, 'true': True, 'false': False}
COND_TOKEN = re.compile(r'\s*(<>|<=|>=|[()=<>]|[a-z_]\w*|\d+)')
COND_WORDS = {'and': 'and', 'or': 'or', 'not': 'not', '=': '==', '<>': '!=', '(': '(', ')': ')', '<': '<', '>': '>', '<=': '<=', '>=': '>='}


def cond_value(expr: str, consts: dict):
    """True, False, or None when the expression names a constant this scan does not know."""
    out = []
    pos = 0
    expr = expr.strip().lower()
    while pos < len(expr):
        m = COND_TOKEN.match(expr, pos)
        if not m:
            return None
        tok = m.group(1)
        pos = m.end()
        if tok in COND_WORDS:
            out.append(COND_WORDS[tok])
        elif tok.isdigit():
            out.append(tok)
        elif consts.get(tok) is not None:
            out.append(str(consts[tok]))
        else:
            return None
    try:
        return bool(eval(' '.join(out), {'__builtins__': {}}, {}))  # only the whitelisted tokens above reach eval
    except Exception:
        return None


def preprocess(lines):
    """Drops the #If branches the Windows dialect constants make dead. An unknown condition keeps every branch."""
    consts = dict(DIALECT)
    stack = []  # frames: [any branch taken, this branch live, condition unknown]
    for line in lines:
        low = line.strip().lower()
        live = all(f[1] for f in stack)
        if low.startswith('#const'):
            m = re.match(r"#const\s+(\w+)\s*=\s*([^']*)", low)
            if m and live:
                consts[m.group(1)] = cond_value(m.group(2), consts)
            continue
        if low.startswith('#if'):
            v = cond_value(re.sub(r'\bthen\b.*$', '', low[3:]), consts)
            stack.append([v is True, v is not False, v is None])
            yield ('cond', low)
            continue
        if low.startswith('#elseif') and stack:
            f = stack[-1]
            v = cond_value(re.sub(r'\bthen\b.*$', '', low[7:]), consts)
            if f[2] or v is None:
                f[1], f[2] = True, True
            else:
                f[1] = (not f[0]) and v
                f[0] = f[0] or v
            yield ('cond', low)
            continue
        if low.startswith('#else') and stack:
            f = stack[-1]
            f[1] = True if f[2] else not f[0]
            continue
        if low.startswith('#end') and stack:
            stack.pop()
            continue
        if live:
            yield ('code', line)


def strip_line(line: str):
    """Blank string literals to \\x00<i>\\x00 and drop the comment. Returns (code, strings)."""
    strings = []

    def keep(m):
        strings.append(m.group(0)[1:-1].replace('""', '"'))
        return f'\x00{len(strings) - 1}\x00'

    code = STRING.sub(keep, line)
    cut = code.find("'")
    if cut >= 0:
        code = code[:cut]
    m = re.match(r'^\s*(?:\d+\s+)?rem(\s|$)', code, re.I)
    if m:
        code = ''
    used = set(int(i) for i in re.findall(r'\x00(\d+)\x00', code))
    return code, {i: s for i, s in enumerate(strings) if i in used}


MEMBER = re.compile(r'\.\s*([a-z_]\w*)')
NOT_A_CALL = re.compile(r'(\b(as|new)\s+|\.\s*)$')
WATCH_MEMBERS = set()
WATCH_VBA = set()


def scan_module(name: str, kind: str, text: str, declares: dict, project_features: collections.Counter, spellings: collections.Counter):
    """Returns procedures: list of dict(name, module, public, kind, features Counter, idents set)."""
    procs = []
    cur = None
    for what, line in preprocess(logical_lines(text)):
        if what == 'cond':
            for word in ('mac', 'win64', 'win32', 'vba7'):
                if re.search(r'\b' + word + r'\b', line):
                    project_features['cond.' + word] += 1
            continue
        if line.startswith('Attribute '):
            continue
        code, strings = strip_line(line)
        if not code.strip():
            continue
        low = code.lower()
        for m in MEMBER.finditer(low):
            if m.group(1) in WATCH_MEMBERS:
                spellings['om:' + m.group(1)] += 1
        for m in IDENT.finditer(low):
            # a name after As or New is a type, and after a dot a member of some object: neither is the library function
            if m.group(0) in WATCH_VBA and not NOT_A_CALL.search(low, 0, m.start()):
                spellings['vba:' + m.group(0)] += 1
        if cur is None:
            md = DECLARE.match(code)
            if md:
                lib = strings.get(int(md.group(3)), '').lower()
                lib = re.sub(r'\.(dll|drv|exe)$', '', lib.split('\\')[-1].split('/')[-1])
                alias = strings.get(int(md.group(4)), md.group(2)) if md.group(4) else md.group(2)
                declares[md.group(2).lower()] = f'{lib}!{alias}'
                project_features['declare.statement'] += 1
                if md.group(1):
                    project_features['declare.ptrsafe'] += 1
                if '.dylib' in lib or lib.startswith('lib') or '/' in strings.get(int(md.group(3)), ''):
                    project_features['declare.mac-dylib'] += 1
                continue
            mp = PROC.match(code)
            if mp:
                cur = {
                    'name': mp.group(3), 'module': name, 'modkind': kind,
                    'public': (mp.group(1) or 'public').lower() != 'private',
                    'kind': re.sub(r'\s+', ' ', mp.group(2).lower()),
                    'features': collections.Counter(), 'idents': set(), 'detail': collections.Counter(),
                }
                # the header line carries no body
            continue
        if END_PROC.match(code):
            procs.append(cur)
            cur = None
            continue
        f = cur['features']
        for feat, rx in PATTERNS:
            m = rx.search(low)
            if not m:
                continue
            if feat == 'file.open':
                mode = SUB_OPEN_MODE.search(low).group(1)
                f['file.open-read' if mode == 'input' else 'file.open-write' if mode in ('output', 'append') else 'file.open-binary'] += 1
            else:
                f[feat] += 1
        for m in CREATE.finditer(low):
            idx = m.group(2) or m.group(3)
            if idx is not None and strings.get(int(idx)):
                cur['detail']['progid:' + strings[int(idx)].strip().lower()] += 1
            else:
                cur['detail']['progid:<dynamic>'] += 1
        for m in ENVIRON.finditer(low):
            if m.group(1) is not None:
                cur['detail']['environ:' + strings.get(int(m.group(1)), '').upper()] += 1
            else:
                cur['detail']['environ:<dynamic>'] += 1
        for m in AS_CLASS.finditer(low):
            cls = EARLY.get(m.group(1))
            if cls:
                cur['detail']['early:' + cls] += 1
                f['com.early-bound'] += 1
        if 'om.application-run' in f:
            for m in RUN.finditer(low):
                cur['detail']['run:literal' if m.group(1) else 'run:dynamic'] += 1
        for s in strings.values():
            for shape in path_shape(s):
                f[shape] += 1
        cur['idents'].update(i.lower() for i in IDENT.findall(low))
    return procs


def close_over_calls(procs, declares):
    """May-reach per procedure: direct features joined with those of every procedure it can call."""
    by_name = collections.defaultdict(list)
    for i, p in enumerate(procs):
        by_name[p['name'].lower()].append(i)
    for p in procs:
        for ident in p['idents']:
            if ident in declares:
                p['features']['ffi.declare-call'] += 1
                p['detail']['declare:' + declares[ident]] += 1
    edges = []
    for i, p in enumerate(procs):
        targets = set()
        for ident in p['idents']:
            for j in by_name.get(ident, ()):
                if j != i:
                    targets.add(j)
        edges.append(targets)
    reach = [set(p['features']) for p in procs]
    detail = [set(p['detail']) for p in procs]
    changed = True
    while changed:
        changed = False
        for i, targets in enumerate(edges):
            for j in targets:
                if not reach[j] <= reach[i]:
                    reach[i] |= reach[j]
                    changed = True
                if not detail[j] <= detail[i]:
                    detail[i] |= detail[j]
                    changed = True
    return reach, detail, sum(len(t) for t in edges)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--list', type=Path, required=True)
    ap.add_argument('--root', type=Path, required=True)
    ap.add_argument('--out', type=Path, required=True)
    ap.add_argument('--watch-members', type=Path, help='object-model member names counted as .Name spellings, one per line')
    ap.add_argument('--watch-vba', type=Path, help='VBA library names counted as identifier spellings, one per line')
    args = ap.parse_args()
    if args.watch_members:
        WATCH_MEMBERS.update(args.watch_members.read_text().lower().split())
    if args.watch_vba:
        WATCH_VBA.update(args.watch_vba.read_text().lower().split())
    args.out.mkdir(parents=True, exist_ok=True)
    status = collections.Counter()
    seen_projects = {}
    with open(args.out / 'procedures.jsonl', 'w') as out:
        for rel in args.list.read_text().split():
            path = args.root / rel
            sha = path.name.split('.')[0]
            try:
                modules, refs = read_project(path)
            except Exception as e:
                status['unreadable: ' + type(e).__name__] += 1
                out.write(json.dumps({'sha': sha, 'error': type(e).__name__}) + '\n')
                continue
            if not modules:
                status['no modules'] += 1
                continue
            status['read'] += 1
            norm = hashlib.sha256()
            for mname, _, text in sorted(modules):
                norm.update(mname.lower().encode())
                norm.update(re.sub(r'\s+', ' ', re.sub(r'(?m)^Attribute .*$', '', text)).encode('utf-8', 'replace'))
            key = norm.hexdigest()[:16]
            first = key not in seen_projects
            seen_projects.setdefault(key, sha)
            declares = {}
            pf = collections.Counter()
            spellings = collections.Counter()
            procs = []
            for mname, kind, text in modules:
                procs += scan_module(mname, kind, text, declares, pf, spellings)
            reach, detail, edge_count = close_over_calls(procs, declares)
            out.write(json.dumps({
                'sha': sha, 'project': key, 'first': first, 'references': refs, 'modules': len(modules),
                'project_features': dict(pf), 'spellings': dict(spellings), 'declares': sorted(set(declares.values())), 'call_edges': edge_count,
                'procedures': [{
                    'entry': f"{p['module']}.{p['name']}", 'public': p['public'], 'kind': p['kind'], 'modkind': p['modkind'],
                    'direct': dict(p['features']), 'direct_detail': dict(p['detail']),
                    'reach': sorted(reach[i]), 'reach_detail': sorted(detail[i]),
                } for i, p in enumerate(procs)],
            }) + '\n')
    json.dump({'status': dict(status), 'distinct_projects': len(seen_projects)}, open(args.out / 'scan-summary.json', 'w'), indent=1)
    print(json.dumps({'status': dict(status), 'distinct_projects': len(seen_projects)}))


if __name__ == '__main__':
    sys.exit(main())
