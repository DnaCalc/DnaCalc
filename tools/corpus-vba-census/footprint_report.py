#!/usr/bin/env python3
"""Portability tables for docs/design/PORTABLE_EXECUTION.md (dc-54aki).

Joins footprint.py scans (triage-clean and, statically only, triage-suspect projects)
with `dnacalc run --all --fixtures --macro-level guarded --json` results of the
triage-clean set, and writes the TSVs beside the design document. The verdict tables
below are the design's proposal in data form: one row per feature, COM class or
Declare entry point, one verdict per host. They are the seed of the declarations the
design moves into the descriptor core and CalcVBA's matrix; nothing reads them yet.

Usage: footprint_report.py --clean <scan dir> [--suspect <scan dir>] [--runs <dir>]
                           [--calcvba-matrix <matrix.rs>] [--reach <reach.json>] --out <dir>
"""
import argparse
import collections
import json
import re
from pathlib import Path

HOSTS = ['windows', 'macos', 'linux', 'server', 'web-hosted', 'web-wasm', 'wasi']
ORDER = ['native', 'model', 'host', 'shim', 'replace', 'mediated', 'reject']
RANK = {v: i for i, v in enumerate(ORDER)}


def v(text):
    """'native shim shim shim shim shim shim' -> list; a single word applies to every host."""
    parts = text.split()
    parts = parts * 7 if len(parts) == 1 else parts
    assert len(parts) == 7 and all(p in RANK for p in parts), text
    return parts


# feature -> (reach capabilities, host requirement, verdicts W M L S H B I, mechanism, owner)
FILES = v('native shim shim shim shim shim shim')
FEATURES = {
    'file.open-read': (['fs.read'], 'fs path.windows', FILES, 'path map', 'dc-8svfs.3'),
    'file.open-write': (['fs.write'], 'fs path.windows codepage', FILES, 'path map', 'dc-8svfs.3 dc-8svfs.5'),
    'file.open-binary': (['fs.read', 'fs.write'], 'fs path.windows', FILES, 'path map', 'dc-8svfs.3'),
    'file.print#': (['fs.write'], 'codepage', v('model'), 'code page model', 'dc-8svfs.5'),
    'file.input#': (['fs.read'], 'codepage', v('model'), 'code page model', 'dc-8svfs.5'),
    'file.get-put#': (['fs.read', 'fs.write'], 'fs', v('native'), '', ''),
    'file.kill': (['fs.write'], 'fs path.windows', FILES, 'path map', 'dc-8svfs.3'),
    'file.mkdir': (['fs.write'], 'fs path.windows', FILES, 'path map', 'dc-8svfs.3'),
    'file.rmdir': (['fs.write'], 'fs path.windows', FILES, 'path map', 'dc-8svfs.3'),
    'file.chdir': ([], 'fs path.windows', FILES, 'path map', 'dc-8svfs.3'),
    'file.chdrive': ([], 'path.windows', FILES, 'path map', 'dc-8svfs.3'),
    'file.curdir': ([], 'path.windows', FILES, 'path map', 'dc-8svfs.3'),
    'file.filecopy': (['fs.read', 'fs.write'], 'fs path.windows', FILES, 'path map', 'dc-8svfs.3'),
    'file.setattr': (['fs.write'], 'fs fs.attributes', FILES, 'attribute store', 'dc-8svfs.3'),
    'file.getattr': (['fs.read'], 'fs fs.attributes', FILES, 'attribute store', 'dc-8svfs.3'),
    'file.name-as': (['fs.write'], 'fs path.windows', FILES, 'path map', 'dc-8svfs.3'),
    'file.dir': (['fs.read'], 'fs path.windows fs.case-insensitive', FILES, 'path map', 'dc-8svfs.3'),
    'file.filelen': (['fs.read'], 'fs path.windows', FILES, 'path map', 'dc-8svfs.3'),
    'file.filedatetime': (['fs.read'], 'fs path.windows', FILES, 'path map', 'dc-8svfs.3'),
    'path.drive-letter': ([], 'path.windows', FILES, 'path map', 'dc-8svfs.3'),
    'path.unc': ([], 'path.windows path.unc', FILES, 'path map (a mounted share only)', 'dc-8svfs.3'),
    'path.backslash-literal': ([], 'path.windows', FILES, 'path map', 'dc-8svfs.3'),
    'path.url': (['network'], 'network', v('native native native native native mediated native'), 'network facet', ''),
    'path.mac': ([], 'path.mac', v('shim native shim shim shim shim shim'), 'path map (Mac dialect)', 'dc-8svfs.12'),
    'path.posix': ([], 'path.posix', v('reject native native native native shim shim'), 'native mode only', ''),
    'path.registry': (['settings'], 'registry', v('native reject reject reject reject reject reject'), '', ''),
    'path.wmi-moniker': (['com.activate'], 'com.os', v('native reject reject reject reject reject reject'), '', ''),
    'env.environ': (['env'], 'env.windows', v('native shim shim shim shim shim shim'), 'Environ shim', 'dc-8svfs.6'),
    'settings.vb': (['settings'], 'settings', v('native shim shim shim shim shim shim'), 'settings store', 'dc-8svfs.4'),
    'process.shell': (['process'], 'process.windows', v('native reject reject reject reject reject reject'), 'per-project command map', ''),
    'process.appactivate': (['process'], 'ui.windows', v('native reject reject reject reject reject reject'), '', ''),
    'ui.sendkeys': (['ui.keys'], 'ui.windows', v('native reject reject reject reject reject reject'), '', ''),
    'ui.msgbox': ([], 'ui.dialog', v('host'), 'UI host', ''),
    'ui.inputbox': ([], 'ui.dialog', v('host'), 'UI host', ''),
    'ui.form-show': ([], 'ui.forms', v('native replace replace reject replace replace reject'), 'MSForms runtime', 'dc-1jia7 dc-oxh8w.73'),
    'ui.onkey': ([], 'ui.keys.own', v('host host host reject host host reject'), 'UI host', ''),
    'wait.doevents': ([], 'event-loop', v('host'), 'UI host', ''),
    'wait.application-wait': ([], 'block', v('native native native native native shim native'), 'worker wait', 'dc-8svfs.14'),
    'wait.ontime': ([], 'timer event-loop', v('host host host host host host reject'), 'UI host', ''),
    'clock.now-timer': ([], 'clock', v('host'), '', ''),
    'ffi.addressof': (['ffi'], 'native.code', v('native reject reject reject reject reject reject'), '', ''),
    'ffi.pointer': ([], '', v('native'), '', ''),
    'mac.macscript': (['process'], 'applescript', v('reject mediated reject reject reject reject reject'), 'Mac dialect', 'dc-8svfs.12'),
    'om.workbooks-open': (['fs.read', 'network'], 'fs path.windows', v('native shim shim shim shim mediated shim'), 'path map', 'dc-8svfs.3'),
    'om.saveas': (['fs.write', 'network'], 'fs path.windows', v('native shim shim shim shim mediated shim'), 'path map', 'dc-8svfs.3'),
    'om.save': (['fs.write'], 'fs', v('native native native native native mediated native'), '', ''),
    'om.file-dialog': ([], 'ui.filedialog', v('host host host reject host mediated reject'), 'UI host', ''),
    'om.path': ([], 'path.windows', v('native shim shim shim shim shim shim'), 'path map (reverse)', 'dc-8svfs.3'),
    'om.pathseparator': ([], 'path.windows', v('native shim shim shim shim shim shim'), 'persona', ''),
    'om.defaultfilepath': ([], 'path.windows', v('native shim shim shim shim shim shim'), 'persona', ''),
    'om.username': ([], 'identity', v('host'), 'persona', ''),
    'om.operatingsystem': ([], 'identity', v('native model model model model model model'), 'persona', ''),
    'om.followhyperlink': (['network', 'process'], 'open.external', v('native native native reject mediated mediated reject'), 'open facet', ''),
    'om.querytables': (['network', 'fs.read'], 'data.connection', v('native replace replace replace replace replace reject'), 'text and web queries only', ''),
    'om.printout': ([], 'print', v('host host host replace host host reject'), 'print facet', 'dc-971p.57'),
    'om.export-pdf': (['fs.write'], 'fs path.windows', v('native shim shim shim shim mediated shim'), 'path map', 'dc-8svfs.3'),
    'om.chart-export': (['fs.write'], 'fs path.windows', v('native shim shim shim shim mediated shim'), 'path map', 'dc-8svfs.3'),
    'om.picture-file': (['fs.read'], 'fs path.windows', v('native shim shim shim shim mediated shim'), 'path map', 'dc-8svfs.3'),
    'om.sendmail': (['network'], 'mail.mapi', v('native reject reject reject reject reject reject'), '', ''),
    'om.application-run': ([], '', v('model'), '', ''),
    'om.executeexcel4macro': ([], '', v('model'), '', ''),
    'om.vbproject': (['project.write'], '', v('model'), '', ''),
    'om.commandbars': ([], 'ui.commandbars', v('model model model model model model model'), '', ''),
    'om.registerxll': (['fs.read', 'ffi'], 'native.code.xll', v('native reject reject reject reject reject reject'), '', ''),
    'om.addins': ([], '', v('model'), '', ''),
    'om.statusbar': ([], 'ui.status', v('host'), '', ''),
    'om.clipboard': ([], 'clipboard.internal', v('model'), 'internal clipboard; the system clipboard is the clipboard facet', ''),
    'om.chart': ([], '', v('native'), '', ''),
    'om.speech': ([], 'speech', v('native reject reject reject reject reject reject'), '', ''),
    'com.early-bound': ([], '', v('native'), 'see the class', ''),
    'com.createobject': ([], '', v('native'), 'see the class', ''),
    'com.getobject': (['com.activate'], 'com.rot', v('native reject reject reject reject reject reject'), '', ''),
    'ffi.declare-call': ([], '', v('native'), 'see the entry point', ''),
}

# COM classes: prefix or exact ProgID (lower case) -> (name, reach, verdicts, state today off Windows, note)
REPLACE = v('native replace replace replace replace replace replace')
NETCLASS = v('native replace replace replace replace mediated replace')
OSONLY = v('native reject reject reject reject reject reject')
COM = [
    ('scripting.dictionary', 'Scripting.Dictionary', [], REPLACE, 'exists', 'engine class today'),
    ('scripting.filesystemobject', 'Scripting.FileSystemObject', ['fs.read', 'fs.write'], REPLACE, 'missing', 'over the file facet and path map; dc-8svfs.8'),
    ('vbscript.regexp', 'VBScript.RegExp', [], REPLACE, 'exists', 'engine class today'),
    ('adodb.stream', 'ADODB.Stream', ['fs.read', 'fs.write'], REPLACE, 'missing', 'bytes, charsets, LoadFromFile/SaveToFile over the file facet; dc-8svfs.8'),
    ('msxml2.domdocument', 'MSXML2.DOMDocument', ['fs.read', 'network'], REPLACE, 'missing', 'DOM and XPath; load and save over the facets; dc-8svfs.8'),
    ('microsoft.xmldom', 'MSXML2.DOMDocument', ['fs.read', 'network'], REPLACE, 'missing', 'old ProgID of the same class'),
    ('msxml2.xmlhttp', 'MSXML2.XMLHTTP', ['network'], NETCLASS, 'missing', 'over the network facet; fetch under CORS in the browser'),
    ('msxml2.serverxmlhttp', 'MSXML2.ServerXMLHTTP', ['network'], NETCLASS, 'missing', 'as XMLHTTP'),
    ('microsoft.xmlhttp', 'MSXML2.XMLHTTP', ['network'], NETCLASS, 'missing', 'old ProgID'),
    ('msxml.xmlhttprequest', 'MSXML2.XMLHTTP', ['network'], NETCLASS, 'missing', 'old ProgID'),
    ('winhttp.winhttprequest', 'WinHttp.WinHttpRequest', ['network'], NETCLASS, 'missing', 'over the network facet'),
    ('msxml2.mxxmlwriter', 'MSXML2 SAX', [], OSONLY, 'missing', ''),
    ('msxml2.saxxmlreader', 'MSXML2 SAX', [], OSONLY, 'missing', ''),
    ('wscript.shell', 'WScript.Shell', ['process', 'settings', 'env'], REPLACE, 'missing', 'partial: SpecialFolders, ExpandEnvironmentStrings, CurrentDirectory, Popup; Run, Exec, RegRead, RegWrite reject'),
    ('wscript.network', 'WScript.Network', ['env'], REPLACE, 'missing', 'UserName, ComputerName, UserDomain from the persona; drive and printer mapping reject'),
    ('scriptlet.typelib', 'Scriptlet.TypeLib', [], REPLACE, 'missing', 'the GUID idiom'),
    ('new:{1c3b4210-f441-11ce-b9ea-00aa006b1a69}', 'MSForms.DataObject', [], v('native replace replace reject mediated mediated reject'), 'missing', 'the clipboard idiom, over the clipboard facet'),
    ('msforms.dataobject', 'MSForms.DataObject', [], v('native replace replace reject mediated mediated reject'), 'missing', 'the clipboard idiom'),
    ('system.collections.arraylist', 'mscorlib ArrayList', [], REPLACE, 'missing', 'pure; on Windows needs .NET Framework 3.5'),
    ('system.text.', 'mscorlib text classes', [], REPLACE, 'missing', 'pure'),
    ('system.security.cryptography.', 'mscorlib hash classes', [], REPLACE, 'missing', 'pure'),
    ('shell.application', 'Shell.Application', ['process', 'fs.read', 'fs.write'], OSONLY, 'refused', 'two idioms could be replaced later: BrowseForFolder, zip through Namespace.CopyHere'),
    ('adodb.connection', 'ADODB.Connection', ['network', 'fs.read', 'fs.write'], OSONLY, 'refused', 'needs OLE DB providers'),
    ('adodb.recordset', 'ADODB.Recordset', ['network', 'fs.read'], OSONLY, 'refused', 'a disconnected in-memory Recordset could be replaced later'),
    ('adodb.command', 'ADODB.Command', ['network'], OSONLY, 'refused', ''),
    ('outlook.application', 'Outlook.Application', ['com.activate', 'network'], OSONLY, 'refused', 'another Office program'),
    ('word.application', 'Word.Application', ['com.activate'], OSONLY, 'refused', 'another Office program'),
    ('powerpoint.application', 'PowerPoint.Application', ['com.activate'], OSONLY, 'refused', 'another Office program'),
    ('access.application', 'Access.Application', ['com.activate'], OSONLY, 'refused', 'another Office program'),
    ('excel.application', 'Excel.Application', ['com.activate'], OSONLY, 'refused', 'CODE_EXECUTION_POLICY open question 7: map to the host Application?'),
    ('internetexplorer.application', 'InternetExplorer.Application', ['network', 'process'], OSONLY, 'refused', ''),
    ('htmlfile', 'MSHTML htmlfile', ['network'], OSONLY, 'refused', 'HTML parsing idiom; could be replaced by a parser without script later'),
    ('mshtml.htmldocument', 'MSHTML htmlfile', ['network'], OSONLY, 'refused', ''),
    ('cdo.', 'CDO mail', ['network'], OSONLY, 'refused', ''),
    ('winmgmts', 'WMI moniker', ['com.activate', 'process'], OSONLY, 'refused', ''),
    ('wbemscripting.', 'WMI', ['com.activate', 'process'], OSONLY, 'refused', ''),
    ('wia.', 'Windows Image Acquisition', ['fs.read', 'fs.write'], OSONLY, 'refused', ''),
    ('msscriptcontrol.', 'ScriptControl', ['process'], OSONLY, 'refused', 'runs script text'),
]
UNKNOWN_COM = ('other registered class', ['com.activate'], OSONLY, 'refused', 'third-party or unlisted')

# Declare entry points: base name (A/W suffix dropped) -> group
GROUPS = {
    #          windows   macos     linux     server    hosted    web-wasm  wasi
    'time': v('native replace replace replace replace replace replace'),
    'string': v('native replace replace replace replace replace replace'),
    'memory': v('native replace replace replace replace replace replace'),
    'identity': v('native replace replace replace replace replace replace'),
    'guid': v('native replace replace replace replace replace replace'),
    'path': v('native replace replace replace replace replace replace'),
    'clipboard': v('native replace replace reject mediated mediated reject'),
    'keys': v('native replace replace reject replace replace reject'),
    'open': v('native mediated mediated reject mediated mediated reject'),
    'download': v('native replace replace replace replace mediated replace'),
    'dialog': v('native replace replace reject replace mediated reject'),
    'window': v('native reject reject reject reject reject reject'),
    'gdi': v('native reject reject reject reject reject reject'),
    'code': v('native reject reject reject reject reject reject'),
    'handle': v('native reject reject reject reject reject reject'),
    'net': v('native reject reject reject reject reject reject'),
    'registry': v('native reject reject reject reject reject reject'),
    'other': v('native reject reject reject reject reject reject'),
    'own-dll': v('native reject reject reject reject reject reject'),
    'mac-dylib': v('reject native reject reject reject reject reject'),
}
GROUP_NOTE = {
    'time': 'no reach; tier 1', 'string': 'over the VM address space; tier 1',
    'memory': 'over the VM address space only; Guarded refuses (ffi)', 'identity': 'from the persona; tier 1',
    'guid': 'tier 1', 'path': 'through the path map and file facet; tier 1',
    'clipboard': 'the clipboard idiom with a managed global heap; tier 2', 'keys': 'key state of our own window; tier 2',
    'open': 'open verb on a document or URL only, through the open facet; tier 2', 'download': 'network and file facets; tier 2',
    'dialog': 'through the file dialog facet; tier 3', 'window': 'needs an HWND', 'gdi': 'needs a device context',
    'code': 'native code, memory protection or processes', 'handle': 'raw kernel handles', 'net': 'use the WinHttpRequest class',
    'registry': 'not the VB settings store', 'other': '', 'own-dll': "the project's own or a third-party DLL", 'mac-dylib': 'Mac dialect',
}
SYMBOLS = {}
for group, names in {
    'time': 'Sleep SleepEx GetTickCount GetTickCount64 QueryPerformanceCounter QueryPerformanceFrequency GetSystemTime GetLocalTime '
            'GetSystemTimeAsFileTime GetTimeZoneInformation SystemTimeToTzSpecificLocalTime TzSpecificLocalTimeToSystemTime timeGetTime '
            'FileTimeToSystemTime SystemTimeToFileTime FileTimeToLocalFileTime GetLastError SetLastError',
    'string': 'lstrlen lstrcpy lstrcpyn lstrcat MultiByteToWideChar WideCharToMultiByte SysAllocString SysFreeString SysStringLen '
              'SysReAllocString SysAllocStringByteLen SysStringByteLen StrCmpLogical CharUpper CharLower CompareString '
              'CryptStringToBinary CryptBinaryToString RtlComputeCrc32',
    'memory': 'RtlMoveMemory RtlZeroMemory RtlFillMemory CopyMemory GlobalAlloc GlobalLock GlobalUnlock GlobalFree GlobalSize '
              'CoTaskMemFree CoTaskMemAlloc VarPtr VariantCopy VariantClear',
    'identity': 'GetUserName GetComputerName GetCurrentProcessId GetCurrentThreadId GetVersionEx GetSystemMetrics GetLocaleInfo '
                'GetEnvironmentVariable SetEnvironmentVariable GlobalMemoryStatusEx GetUserDefaultLCID GetSystemDefaultLCID '
                'GetKeyboardLayoutName InternetGetConnectedState',
    'guid': 'CoCreateGuid StringFromGUID2 IIDFromString CLSIDFromString',
    'path': 'SetCurrentDirectory GetCurrentDirectory GetTempPath GetTempFileName GetShortPathName GetLongPathName GetWindowsDirectory '
            'GetSystemDirectory SHGetFolderPath SHGetSpecialFolderPath PathFileExists PathIsDirectory MakeSureDirectoryPathExists '
            'DeleteFile MoveFile MoveFileEx CopyFile CreateDirectory RemoveDirectory FindFirstFile FindFirstFileEx FindNextFile FindClose '
            'GetFileAttributes SetFileAttributes GetFileSizeEx GetPrivateProfileString WritePrivateProfileString GetFullPathName '
            'WNetGetConnection DeleteUrlCacheEntry',
    'clipboard': 'OpenClipboard EmptyClipboard SetClipboardData GetClipboardData CloseClipboard IsClipboardFormatAvailable RegisterClipboardFormat',
    'keys': 'GetAsyncKeyState GetKeyState GetKeyboardState',
    'open': 'ShellExecute ShellExecuteEx',
    'download': 'URLDownloadToFile',
    'dialog': 'GetOpenFileName GetSaveFileName SHBrowseForFolder SHGetPathFromIDList ChooseColor',
    'window': 'FindWindow FindWindowEx GetWindowLong SetWindowLong GetWindowLongPtr SetWindowLongPtr SetWindowPos DrawMenuBar GetWindow '
              'GetWindowText GetWindowTextLength SendMessage PostMessage MoveWindow GetActiveWindow GetClassName IsWindow ShowWindow '
              'ShowWindowAsync GetForegroundWindow SetForegroundWindow BringWindowToTop GetAncestor GetClientRect GetDesktopWindow '
              'GetCursorPos CreatePopupMenu DestroyMenu AppendMenu TrackPopupMenu TrackPopupMenuEx SetLayeredWindowAttributes '
              'IsWindowEnabled WindowFromPoint GetWindowThreadProcessId GetWindowRect RedrawWindow SetParent IsChild GetMenu '
              'GetMenuItemID GetSubMenu GetTopWindow SetMenuItemBitmaps CheckMenuRadioItem GetMenuItemCount GetSystemMenu EnableMenuItem '
              'RemoveMenu DestroyWindow IsIconic ReleaseCapture SetCursor LoadCursor SetWindowRgn DialogBoxParam SetTimer KillTimer '
              'SendInput keybd_event mouse_event SetKeyboardState LoadKeyboardLayout SystemParametersInfo CopyImage DwmSetWindowAttribute '
              'AccessibleObjectFromWindow WindowFromAccessibleObject SetWindowsHookEx UnhookWindowsHookEx CallNextHookEx',
    'gdi': 'GetDC ReleaseDC GetDeviceCaps DeleteObject CreateSolidBrush SelectObject CreatePen Arc Ellipse CloseEnhMetaFile DeleteDC '
           'GetStockObject CreateCompatibleBitmap CreateRoundRectRgn OleCreatePictureIndirect OleTranslateColor PlaySound',
    'code': 'LoadLibrary LoadLibraryEx GetProcAddress FreeLibrary GetModuleHandle DispCallFunc VirtualAlloc VirtualFree VirtualProtect '
            'VirtualQuery CreateProcess CreatePipe PeekNamedPipe OpenProcess WaitForSingleObject GetExitCodeProcess GetCurrentProcess '
            'CLRCreateInstance AddDllDirectory SetDefaultDllDirectories TerminateProcess CreateThread rtcCallByName GetProcessHandleCount '
            'GetProcessMemoryInfo',
    'handle': 'CreateFile ReadFile WriteFile CloseHandle OpenFile SHCreateStreamOnFile SetFilePointer FlushFileBuffers',
    'registry': 'RegCloseKey RegQueryValueEx RegOpenKeyEx RegCreateKey RegCreateKeyEx RegSetValueEx RegDeleteKey RegDeleteValue RegOpenKey',
}.items():
    for name in names.split():
        SYMBOLS[name.lower()] = group
SYSTEM_LIBS = {'kernel32', 'user32', 'gdi32', 'gdiplus', 'shell32', 'advapi32', 'ole32', 'oleaut32', 'shlwapi', 'winhttp', 'wininet', 'urlmon',
               'wsock32', 'ws2_32', 'comdlg32', 'winmm', 'crypt32', 'bcrypt', 'ntdll', 'psapi', 'oleacc', 'dwmapi', 'mpr', 'vbe7', 'user',
               'kernel', 'msvcrt', 'ucrtbase', 'mscoree', 'imagehlp', 'version', 'icmp', 'odbc32', 'cabinet', 'uxtheme', 'comctl32', 'imm32',
               'iphlpapi', 'netapi32', 'secur32', 'wtsapi32', 'winspool', 'hhctrl.ocx', 'olepro32', 'msimg32', 'kernelbase', 'dbghelp'}
NET_LIBS = {'winhttp', 'wininet', 'wsock32', 'ws2_32', 'icmp', 'iphlpapi'}


def symbol_group(decl: str) -> str:
    lib, sym = decl.split('!', 1)
    lib = lib.lower()
    if lib.endswith('.dylib') or lib.startswith('lib') and lib not in SYSTEM_LIBS and '/' in lib or lib == 'libc.dylib':
        return 'mac-dylib'
    if lib not in SYSTEM_LIBS:
        return 'own-dll'
    base = sym.lower()
    g = SYMBOLS.get(base) or SYMBOLS.get(base[:-1] if base[-1:] in 'aw' else base)
    if g:
        return g
    if lib in NET_LIBS:
        return 'net'
    if lib in ('gdi32', 'gdiplus', 'msimg32', 'winmm', 'uxtheme'):
        return 'gdi'
    if lib in ('user32', 'user', 'dwmapi', 'oleacc', 'comctl32', 'imm32'):
        return 'window'
    return 'other'


def com_entry(progid: str):
    p = progid.lower()
    for prefix, name, reach, verdicts, state, note in COM:
        if p.startswith(prefix):
            return name, reach, verdicts, state, note
    return UNKNOWN_COM


def reach_items(reach, detail):
    """Yields (label, reach capabilities, verdicts, mechanism) for every element of a procedure's may-reach set."""
    for f in reach:
        # A path literal is evidence of what a project expects, not a need: the need arises where a path reaches the file facet.
        if f in ('com.createobject', 'com.early-bound', 'ffi.declare-call') or f.startswith('cond.') or f.startswith('path.'):
            continue
        caps, _, verdicts, mech, _ = FEATURES[f]
        yield f, caps, verdicts, mech or f
    for d in detail:
        kind, _, value = d.partition(':')
        if kind in ('progid', 'early'):
            if value == '<dynamic>':
                continue
            name, caps, verdicts, _, _ = com_entry(value)
            yield 'com:' + name, caps, verdicts, 'class ' + name
        elif kind == 'declare':
            g = symbol_group(value)
            yield 'declare:' + value, ['ffi'], GROUPS[g], 'Declare ' + g


def classify(reach, detail):
    """Per host: the worst verdict over the may-reach set, the Guarded capabilities, and the mechanisms needed on Linux."""
    worst = [0] * 7
    caps = set()
    mech = set()
    blockers = set()
    for label, c, verdicts, m in reach_items(reach, detail):
        caps.update(c)
        for i, word in enumerate(verdicts):
            worst[i] = max(worst[i], RANK[word])
        if verdicts[2] in ('shim', 'replace', 'mediated'):
            mech.add(m)
        if verdicts[2] == 'reject':
            blockers.add(label)
    dynamic = any(d in ('progid:<dynamic>', 'run:dynamic', 'environ:<dynamic>') for d in detail)
    return [ORDER[w] for w in worst], caps, mech, blockers, dynamic


def load(scan: Path):
    rows = []
    for line in open(scan / 'procedures.jsonl'):
        d = json.loads(line)
        if 'error' not in d:
            rows.append(d)
    return rows


def run_outcome(inv):
    o = inv.get('outcome', {})
    kind = o.get('kind')
    if kind == 'runtime-error':
        desc = o.get('description', '')
        m = re.search(r'CapabilityUnavailable.*?capability: (\w+), operation: "([^"]*)"', desc)
        if m:
            return 'host facility absent', f'{m.group(1)} ({m.group(2)})'
        if inv.get('denials') or o.get('number') in (70,):
            return 'policy denial', ','.join(sorted({str(x.get('capability', x)) if isinstance(x, dict) else str(x) for x in inv.get('denials', [])})) or 'error 70'
        return 'VBA run-time error', str(o.get('number'))
    if kind == 'refused':
        return 'not modelled', (o.get('member') or '').split(':')[0]
    if kind == 'success' and inv.get('denials'):
        return 'success after a denial', ''
    return {'success': 'success', 'needs-arguments': 'needs arguments', 'not-callable': 'not callable'}.get(kind, str(kind)), ''


def tsv(path: Path, header, rows):
    with open(path, 'w') as f:
        f.write('\t'.join(header) + '\n')
        for r in rows:
            f.write('\t'.join(str(x) for x in r) + '\n')


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--clean', type=Path, required=True)
    ap.add_argument('--suspect', type=Path)
    ap.add_argument('--runs', type=Path)
    ap.add_argument('--calcvba-matrix', type=Path)
    ap.add_argument('--reach', type=Path)
    ap.add_argument('--out', type=Path, required=True)
    args = ap.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    sets = {'clean': load(args.clean)}
    if args.suspect:
        sets['suspect'] = load(args.suspect)
    summary = {'hosts': HOSTS, 'verdict_order': ORDER, 'sets': {}}

    # 1. frequency by distinct project: features, COM classes, Declare entry points, Environ names, spellings
    feat = collections.defaultdict(lambda: collections.Counter())
    com = collections.defaultdict(lambda: collections.Counter())
    decl = collections.defaultdict(lambda: collections.Counter())
    env = collections.defaultdict(lambda: collections.Counter())
    spell = collections.defaultdict(lambda: collections.Counter())
    for name, rows in sets.items():
        projects = [d for d in rows if d['first']]
        procs = sum(len(d['procedures']) for d in projects)
        summary['sets'][name] = {'workbooks': len(rows), 'distinct_projects': len(projects), 'procedures': procs,
                                 'projects_with_declare': sum(1 for d in projects if d['declares'])}
        for d in projects:
            fs, cs, es = set(), set(), set()
            for p in d['procedures']:
                for f in p['direct']:
                    fs.add(f)
                    feat[f][name + '.procedures'] += 1
                for det in p['direct_detail']:
                    kind, _, value = det.partition(':')
                    if kind in ('progid', 'early'):
                        cs.add(com_entry(value)[0] if value != '<dynamic>' else '<dynamic ProgID>')
                    elif kind == 'environ':
                        es.add(value)
            for f in fs:
                feat[f][name + '.projects'] += 1
            for c in cs:
                com[c][name] += 1
            for e in es:
                env[e][name] += 1
            for s in set(d['declares']):
                decl[s][name] += 1
            for s, n in d.get('spellings', {}).items():
                spell[s][name + '.projects'] += 1
                spell[s][name + '.uses'] += n

    def counts(counter, suffix=''):
        return [counter.get('clean' + suffix, 0), counter.get('suspect' + suffix, 0)]

    rows = []
    for f, (caps, need, verdicts, mech, owner) in FEATURES.items():
        rows.append([f, ' '.join(caps) or '-', need or '-'] + verdicts + counts(feat[f], '.projects') + counts(feat[f], '.procedures') + [mech or '-', owner or '-'])
    rows.sort(key=lambda r: -(r[10] + r[11]))
    tsv(args.out / 'corpus-features.tsv',
        ['feature', 'reach', 'needs'] + HOSTS + ['clean_projects', 'suspect_projects', 'clean_procedures', 'suspect_procedures', 'mechanism', 'owner'], rows)

    by_name = {}
    for prefix, name, reach, verdicts, state, note in COM:
        by_name.setdefault(name, (reach, verdicts, state, note))
    by_name[UNKNOWN_COM[0]] = UNKNOWN_COM[1:]
    by_name['<dynamic ProgID>'] = ([], v('native'), 'run time', 'resolved when the call runs; the static report says unknown')
    rows = []
    for name, (reach, verdicts, state, note) in by_name.items():
        rows.append([name, ' '.join(reach) or '-'] + list(verdicts) + counts(com[name]) + [state, note or '-'])
    rows.sort(key=lambda r: -(r[9] + r[10]))
    tsv(args.out / 'corpus-com-classes.tsv', ['class', 'reach'] + HOSTS + ['clean_projects', 'suspect_projects', 'off_windows_today', 'note'], rows)

    rows = []
    group_projects = collections.Counter()
    for s, c in decl.items():
        g = symbol_group(s)
        total = c.get('clean', 0) + c.get('suspect', 0)
        group_projects[g] += total
        if total >= 2 or g not in ('own-dll', 'other', 'window', 'gdi', 'net', 'code'):
            rows.append([s, g] + GROUPS[g] + counts(c) + [GROUP_NOTE[g] or '-'])
    rows.sort(key=lambda r: -(r[9] + r[10]))
    tsv(args.out / 'corpus-declares.tsv', ['entry_point', 'group'] + HOSTS + ['clean_projects', 'suspect_projects', 'note'], rows)
    summary['declare'] = {'distinct_entry_points': len(decl), 'listed': len(rows),
                          'entry_points_by_group': dict(collections.Counter(symbol_group(s) for s in decl)),
                          'project_uses_by_group': dict(group_projects)}
    # how many Declare-using projects each tier of the shim table covers completely
    tiers = [('tier 1: time string identity guid path', {'time', 'string', 'identity', 'guid', 'path'}),
             ('plus memory', {'memory'}), ('plus tier 2: clipboard keys open download', {'clipboard', 'keys', 'open', 'download'}),
             ('plus tier 3: dialog', {'dialog'})]
    cover = collections.OrderedDict()
    declaring = [d for rows in sets.values() for d in rows if d['first'] and d['declares']]
    allowed = set()
    for label, groups in tiers:
        allowed |= groups
        cover[label] = sum(1 for d in declaring if all(symbol_group(s) in allowed for s in d['declares']))
    summary['declare']['projects_with_declare'] = len(declaring)
    summary['declare']['projects_fully_covered_cumulative'] = cover
    summary['declare']['projects_blocked_by_group'] = dict(collections.Counter(
        g for d in declaring for g in {symbol_group(s) for s in d['declares']} if GROUPS[g][2] == 'reject').most_common())
    tsv(args.out / 'corpus-environ.tsv', ['name', 'clean_projects', 'suspect_projects'],
        sorted(([e] + counts(c) for e, c in env.items()), key=lambda r: -(r[1] + r[2])))

    # 2. the member list: CalcVBA's VBA matrix widened to seven hosts, then object-model spellings
    member_rows = []
    if args.calcvba_matrix:
        src = args.calcvba_matrix.read_text()
        for area, name, cls, support, mac in re.findall(
                r'Feature \{ area: "([^"]+)", name: "([^"]+)", class: FeatureClass::(\w+), support: \[([^\]]+)\], mac_dialect: "([^"]*)"', src):
            w, m, l, wasi, browser = [s.strip().lower() for s in support.split(',')]
            server = 'reject' if area == 'VBEGlobal' else l
            key = 'vba:' + name.split('/')[0].lower()
            c = spell.get(key, {})
            member_rows.append(['vba', area, name, cls.lower(), w, m, l, server, l, browser, wasi,
                                c.get('clean.projects', 0), c.get('suspect.projects', 0), c.get('clean.uses', 0) + c.get('suspect.uses', 0), mac or '-'])
    om_host = {
        'open': 'om.workbooks-open', 'opentext': 'om.workbooks-open', 'saveas': 'om.saveas', 'savecopyas': 'om.saveas', 'save': 'om.save',
        'getopenfilename': 'om.file-dialog', 'getsaveasfilename': 'om.file-dialog', 'filedialog': 'om.file-dialog', 'dialogs': 'om.file-dialog',
        'path': 'om.path', 'fullname': 'om.path', 'pathseparator': 'om.pathseparator', 'defaultfilepath': 'om.defaultfilepath',
        'startuppath': 'om.defaultfilepath', 'templatespath': 'om.defaultfilepath', 'librarypath': 'om.defaultfilepath',
        'username': 'om.username', 'operatingsystem': 'om.operatingsystem', 'followhyperlink': 'om.followhyperlink',
        'querytables': 'om.querytables', 'printout': 'om.printout', 'printpreview': 'om.printout', 'activeprinter': 'om.printout',
        'exportasfixedformat': 'om.export-pdf', 'export': 'om.chart-export', 'sendmail': 'om.sendmail', 'run': 'om.application-run',
        'executeexcel4macro': 'om.executeexcel4macro', 'vbproject': 'om.vbproject', 'vbe': 'om.vbproject', 'commandbars': 'om.commandbars',
        'registerxll': 'om.registerxll', 'addins': 'om.addins', 'statusbar': 'om.statusbar', 'addpicture': 'om.picture-file',
        'cutcopymode': 'om.clipboard', 'copypicture': 'om.clipboard', 'copy': 'om.clipboard', 'paste': 'om.clipboard', 'cut': 'om.clipboard',
        'pastespecial': 'om.clipboard', 'sendkeys': 'ui.sendkeys', 'wait': 'wait.application-wait', 'ontime': 'wait.ontime', 'onkey': 'ui.onkey',
        'inputbox': 'ui.inputbox', 'speech': 'om.speech', 'show': 'ui.form-show',
    }
    om = sorted(((k[3:], c) for k, c in spell.items() if k.startswith('om:')), key=lambda kc: -(kc[1].get('clean.projects', 0) + kc[1].get('suspect.projects', 0)))
    for name, c in om[:200]:
        f = om_host.get(name)
        verdicts = FEATURES[f][2] if f else v('native')
        member_rows.append(['excel-om', FEATURES[f][1] if f else '-', name, 'host-reach' if f else 'document', *verdicts,
                            c.get('clean.projects', 0), c.get('suspect.projects', 0), c.get('clean.uses', 0) + c.get('suspect.uses', 0),
                            ('reach ' + ' '.join(FEATURES[f][0])) if f and FEATURES[f][0] else '-'])
    tsv(args.out / 'corpus-members.tsv',
        ['face', 'area_or_need', 'member', 'class'] + HOSTS + ['clean_projects', 'suspect_projects', 'uses', 'note'], member_rows)
    summary['members'] = {'vba_rows': sum(1 for r in member_rows if r[0] == 'vba'), 'om_spellings_listed': sum(1 for r in member_rows if r[0] == 'excel-om'),
                          'om_spellings_seen': len(om)}

    # 3. per-procedure class on every host, static, for both sets (distinct projects)
    for name, rows in sets.items():
        per_host = [collections.Counter() for _ in HOSTS]
        proj_host = [collections.Counter() for _ in HOSTS]
        grant = collections.Counter()
        dyn = 0
        total = 0
        nproj = 0
        edges = 0
        direct_reach = 0
        any_reach = 0
        for d in rows:
            if not d['first']:
                continue
            nproj += 1
            edges += d['call_edges']
            pw = [0] * 7
            for p in d['procedures']:
                total += 1
                quiet = {f for f in list(p['direct']) + p['reach'] if f.startswith('path.') or f.startswith('cond.')} | {'om.chart', 'ffi.pointer'}
                direct_reach += bool(set(p['direct']) - quiet)
                any_reach += bool(set(p['reach']) - quiet)
                worst, caps, _, _, dynamic = classify(p['reach'], p['reach_detail'])
                dyn += dynamic
                for i, w in enumerate(worst):
                    per_host[i][w] += 1
                    pw[i] = max(pw[i], RANK[w])
                grant['needs a grant' if caps else 'no grant'] += 1
                for c in caps:
                    grant[c] += 1
            for i in range(7):
                proj_host[i][ORDER[pw[i]]] += 1
        summary['sets'][name]['static_class_by_host'] = {
            'procedures': {h: dict(per_host[i]) for i, h in enumerate(HOSTS)},
            'projects_worst_procedure': {h: dict(proj_host[i]) for i, h in enumerate(HOSTS)},
            'guarded': dict(grant), 'procedures_with_dynamic_target': dyn, 'procedure_total': total, 'project_total': nproj,
            'call_edges': edges, 'procedures_with_direct_host_reach': direct_reach, 'procedures_with_host_reach_after_closure': any_reach}
        tsv(args.out / f'static-class-{name}.tsv', ['scope', 'verdict'] + HOSTS,
            [['procedures', w] + [per_host[i][w] for i in range(7)] for w in ORDER]
            + [['projects (worst procedure)', w] + [proj_host[i][w] for i in range(7)] for w in ORDER])

    # 4. the triage-clean run on Linux today, joined to the static class
    if args.runs:
        cross = collections.Counter()
        details = collections.defaultdict(collections.Counter)
        mech_count = collections.Counter()
        mech_only = collections.Counter()
        block_count = collections.Counter()
        grant = collections.Counter()
        adm = collections.Counter()
        wb_cross = collections.Counter()
        examples = collections.defaultdict(list)
        unmatched = 0
        # One workbook stands for each distinct project: the first copy whose project was admitted, else the first copy.
        runs = {}
        rep = {}
        for d in sets['clean']:
            try:
                runs[d['sha']] = json.load(open(args.runs / (d['sha'] + '.json')))
            except Exception:
                runs[d['sha']] = None
            run = runs[d['sha']]
            admitted = run is not None and 'invocations' in run and not run.get('error')
            adm['workbooks: ' + ('no result' if run is None else 'admitted' if admitted else 'not admitted')] += 1
            if d['project'] not in rep or (admitted and not rep[d['project']][1]):
                rep[d['project']] = (d['sha'], admitted)
        for sha, admitted in rep.values():
            adm['projects: ' + ('admitted' if admitted else 'no result' if runs[sha] is None else 'not admitted')] += 1
        for d in sets['clean']:
            run = runs[d['sha']]
            d['first'] = rep[d['project']][0] == d['sha']
            static = {p['entry'].lower(): p for p in d['procedures']}
            if run is None or not run.get('invocations'):
                if d['first'] and not rep[d['project']][1]:
                    for p in d['procedures']:
                        if p['public'] and p['modkind'] == 'std':
                            worst, caps, mech, blockers, _ = classify(p['reach'], p['reach_detail'])
                            cross[(worst[2], 'not run: project not admitted')] += 1
                continue
            for inv in run['invocations']:
                p = static.get(inv['entry'].lower())
                if p is None:
                    unmatched += 1
                    worst, caps, mech, blockers = ['native'] * 7, set(), set(), set()
                else:
                    worst, caps, mech, blockers, _ = classify(p['reach'], p['reach_detail'])
                outcome, det = run_outcome(inv)
                wb_cross[(worst[2], outcome)] += 1
                if not d['first']:
                    continue
                cross[(worst[2], outcome)] += 1
                if det:
                    details[outcome][det] += 1
                grant[('needs a grant' if caps else 'no grant', outcome)] += 1
                for m in mech:
                    mech_count[m] += 1
                if len(mech) == 1 and worst[2] in ('shim', 'replace', 'mediated'):
                    mech_only[next(iter(mech))] += 1
                for b in blockers:
                    block_count[b] += 1
                key = (worst[2], outcome)
                if len(examples[key]) < 3 and p is not None:
                    examples[key].append(f"{d['sha'][:12]} {inv['entry']} [{' '.join(sorted(set(p['reach']) - {'cond.mac'}))[:120]}]")
        outcomes = sorted({o for _, o in cross}, key=lambda o: -sum(n for (c, oo), n in cross.items() if oo == o))
        tsv(args.out / 'linux-run-by-class.tsv', ['linux_static_class'] + outcomes + ['total'],
            [[c] + [cross[(c, o)] for o in outcomes] + [sum(cross[(c, o)] for o in outcomes)] for c in ORDER]
            + [['total'] + [sum(cross[(c, o)] for c in ORDER) for o in outcomes] + [sum(cross.values())]])
        summary['linux_run'] = {
            'projects': dict(adm), 'entries_unmatched_to_source': unmatched,
            'distinct_project_entries': {f'{c} / {o}': n for (c, o), n in sorted(cross.items())},
            'every_workbook_entries': {f'{c} / {o}': n for (c, o), n in sorted(wb_cross.items())},
            'guarded': {f'{g} / {o}': n for (g, o), n in sorted(grant.items())},
            'outcome_detail': {o: dict(c.most_common(12)) for o, c in details.items()},
            'mechanism_needed_by_entries': dict(mech_count.most_common()),
            'mechanism_alone_unblocks': dict(mech_only.most_common()),
            'linux_blockers': dict(block_count.most_common()),
            'examples': {f'{c} / {o}': e for (c, o), e in sorted(examples.items())},
        }
    json.dump(summary, open(args.out / 'summary.json', 'w'), indent=1, sort_keys=True)
    print(json.dumps({k: summary[k] for k in summary if k not in ('sets',)}, indent=1)[:6000])


if __name__ == '__main__':
    main()
