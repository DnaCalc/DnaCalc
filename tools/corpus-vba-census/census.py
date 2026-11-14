#!/usr/bin/env python3
"""Static VBA/XLM inventory and MS-OVBA reference census for workbook corpora.

Requires olefile and oletools from the selected Python environment. This script
never launches Office and never extracts or executes workbook code.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import struct
import sys
import zipfile
from collections import Counter, defaultdict
from pathlib import Path

import olefile
import oletools.olevba as olevba

WORKBOOK_EXTS = {'.xls', '.xlsm', '.xlsb', '.xla', '.xlam', '.xlt', '.xltm', '.xlsx', '.xltx'}
VBA_EXTS = {'.xls', '.xlsm', '.xlsb', '.xla', '.xlam', '.xlt', '.xltm'}
GUID_RE = re.compile(r'\{([0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12})\}')


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open('rb') as f:
        for block in iter(lambda: f.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def local_set(path: Path, root: Path, indexed: dict[str, str]) -> str:
    try:
        rel = path.relative_to(root)
    except ValueError:
        return 'r2-' + (path.parent.name or 'unknown')
    # Prefer the established classifier set label where the index knows this hash.
    # Otherwise retain useful source subdirectories for the harvested trees.
    # The indexed mapping is applied by the caller after hashing.
    bits = rel.parts
    if bits[0] == '_r2' and len(bits) > 1:
        return 'r2-' + bits[1]
    if bits[0] == 'new' and len(bits) > 1:
        return 'new-' + bits[1]
    if bits[0] == 'libs' and len(bits) > 1:
        return bits[1]
    return bits[0]


def indexed_sets(index_path: Path) -> dict[str, str]:
    out = {}
    if not index_path.exists():
        return out
    with index_path.open(encoding='utf-8') as f:
        for line in f:
            try:
                row = json.loads(line)
                if row.get('sha256') and row.get('set'):
                    out.setdefault(row['sha256'], row['set'])
            except (ValueError, TypeError):
                continue
    return out


def indexed_vba_flags(index_path: Path) -> dict[str, bool]:
    out = {}
    if not index_path.exists():
        return out
    with index_path.open(encoding='utf-8') as f:
        for line in f:
            try:
                row = json.loads(line)
                if row.get('sha256') and row.get('vba'):
                    out[row['sha256']] = True
            except (ValueError, TypeError):
                continue
    return out


def biff_xlm(ole: olefile.OleFileIO) -> bool:
    workbook = next((name for name in ('Workbook', 'Book') if ole.exists(name)), None)
    if workbook is None:
        return False
    data = ole.openstream(workbook).read(8 * 1024 * 1024)
    pos = 0
    first = True
    while pos + 4 <= len(data):
        rid, size = struct.unpack_from('<HH', data, pos)
        body = data[pos + 4:pos + 4 + size]
        if rid in (0x0809, 0x0009, 0x0209, 0x0409):
            if not first:
                break
            first = False
        elif rid == 0x0085 and len(body) >= 6 and body[5] == 1:
            return True
        elif rid == 0x000A:
            break
        pos += 4 + size
    return False


def raw_vba_marker(path: Path) -> bool:
    needles = (b'_\x00V\x00B\x00A\x00_\x00P\x00R\x00O\x00J\x00E\x00C\x00T\x00',
               b'V\x00B\x00A\x00\x00\x00')
    overlap = b''
    try:
        with path.open('rb') as f:
            while True:
                block = f.read(1024 * 1024)
                if not block:
                    return False
                data = overlap + block
                if needles[0] in data:
                    return True
                if needles[1] in data and b'd\x00i\x00r\x00' in data:
                    return True
                overlap = data[-64:]
    except OSError:
        return False


def inspect_container(path: Path) -> tuple[bool, bool, str | None]:
    """Return (has VBA marker, has XLM sheets, container error class)."""
    try:
        if zipfile.is_zipfile(path):
            with zipfile.ZipFile(path) as z:
                names = [n.lower() for n in z.namelist()]
            vba = any(n.endswith('vbaproject.bin') for n in names)
            xlm = any(n.startswith('xl/macrosheets/') or n.startswith('xl/intlmacrosheets/') for n in names)
            return vba, xlm, None
        if olefile.isOleFile(str(path)):
            with olefile.OleFileIO(str(path)) as ole:
                entries = [[p.upper() for p in item] for item in ole.listdir(streams=True, storages=True)]
                vba = any(item and item[0] == '_VBA_PROJECT_CUR' or 'VBA' in item[:2] for item in entries)
                try:
                    xlm = biff_xlm(ole)
                except Exception:
                    xlm = False
            return vba, xlm, None
        if path.suffix.lower() in {'.xls', '.xla', '.xlt'} and raw_vba_marker(path):
            return True, False, 'truncated-ole-container'
        return False, False, 'unsupported-container'
    except Exception as e:
        # A raw directory-entry marker can identify an unreadable OLE VBA project,
        # but does not make its contents readable.
        marker = raw_vba_marker(path)
        return marker, False, 'unreadable-container:' + type(e).__name__


def _records(data: bytes):
    """Yield (id, body) for the records of a decompressed MS-OVBA dir stream (2.3.4.2)."""
    pos, n = 0, len(data)
    while pos + 6 <= n:
        rid, size = struct.unpack_from('<HI', data, pos)
        pos += 6
        if rid == 0x0009:  # PROJECTVERSION: its size field says 4, the record carries 6 bytes
            size = 6
        if pos + size > n:
            raise EOFError('dir record 0x%04X runs past the stream' % rid)
        yield rid, data[pos:pos + size]
        pos += size
        if rid == 0x0010:  # dir stream terminator
            return


def _sized(body: bytes, off: int = 0):
    (length,) = struct.unpack_from('<I', body, off)
    return body[off + 4:off + 4 + length], off + 4 + length


def parse_dir_stream(raw: bytes) -> dict:
    """Project name, module count and references of one VBA project, read record by record."""
    data = bytes(olevba.decompress_stream(bytearray(raw)))
    state = {'codepage': 1252}
    name, modules = None, None
    refs, pending_name, pending_original = [], None, None

    def mbcs(b: bytes) -> str:
        try:
            return b.decode('cp%d' % state['codepage'])
        except (LookupError, UnicodeDecodeError):
            return b.decode('latin-1')

    for rid, body in _records(data):
        if rid == 0x0003 and len(body) >= 2:
            state['codepage'] = struct.unpack_from('<H', body)[0]
        elif rid == 0x0004:
            name = mbcs(body)
        elif rid == 0x0016:
            pending_name = mbcs(body)
        elif rid == 0x003E:
            pending_name = body.decode('utf-16-le', 'replace')
        elif rid == 0x0033:
            pending_original = mbcs(body)
        elif rid == 0x000D:
            libid, _ = _sized(body)
            refs.append({'kind': 'registered', 'name': pending_name, 'libid': mbcs(libid)})
            pending_name = None
        elif rid == 0x000E:
            absolute, off = _sized(body)
            relative, off = _sized(body, off)
            major, minor = struct.unpack_from('<IH', body, off)
            refs.append({'kind': 'project', 'name': pending_name, 'libid': mbcs(absolute),
                         'relative': mbcs(relative), 'major': major, 'minor': minor})
            pending_name = None
        elif rid == 0x0030:
            # REFERENCECONTROL: the twiddled part (0x002F) is skipped; the extended part holds the libid
            libid, _ = _sized(body)
            refs.append({'kind': 'control', 'name': pending_name, 'libid': mbcs(libid),
                         'original': pending_original})
            pending_name, pending_original = None, None
        elif rid == 0x000F and len(body) >= 2:
            modules = struct.unpack_from('<H', body)[0]
            break  # module records follow; the references are complete
    if modules is None:
        raise EOFError('no PROJECTMODULES record')
    return {'project': name, 'modules': modules, 'references': refs}


def vba_dir_streams(path: Path):
    """Yield (label, raw dir stream) for every VBA project in a workbook package."""
    def from_ole(ole, label):
        for entry in ole.listdir(streams=True, storages=False):
            if len(entry) >= 2 and entry[-1].lower() == 'dir' and entry[-2].upper() == 'VBA':
                yield label + '/'.join(entry), ole.openstream(entry).read()
    if zipfile.is_zipfile(path):
        with zipfile.ZipFile(path) as z:
            for member in z.namelist():
                if member.lower().endswith('vbaproject.bin'):
                    with olefile.OleFileIO(z.read(member)) as ole:
                        yield from from_ole(ole, member + ':')
    elif olefile.isOleFile(str(path)):
        with olefile.OleFileIO(str(path)) as ole:
            yield from from_ole(ole, '')
    else:
        raise ValueError('not a zip or OLE package')


def parse_libid(text: str) -> dict:
    fields = text.split('#')
    guid = None
    version = None
    lcid = None
    file_name = None
    description = None
    match = GUID_RE.search(text)
    if match:
        guid = '{' + match.group(1).upper() + '}'
        before = text[:match.start()]
        after = text[match.end():].lstrip('#')
        tail = after.split('#')
        if tail:
            if re.fullmatch(r'\d+\.\d+', tail[0]):
                version = tail[0]
            if len(tail) > 1:
                try:
                    lcid = int(tail[1], 0)
                except ValueError:
                    try:
                        lcid = int(tail[1], 10)
                    except ValueError:
                        lcid = None
            if len(tail) > 2:
                path = tail[2].replace('\\', '/')
                file_name = path.rsplit('/', 1)[-1] or None
            if len(tail) > 3:
                description = tail[3] or None
    return {'guid': guid, 'version': version, 'lcid': lcid,
            'file': file_name, 'description': description}


def project_rows(path: Path, sha: str, set_name: str, fmt: str, project_class, xlm: bool) -> list[dict]:
    rows = []

    def row(status, project=None, modules=None, refs=()):
        return {'sha256': sha, 'set': set_name, 'format': fmt, 'project': project, 'modules': modules,
                'references': list(refs), 'xlm_sheets': bool(xlm), 'parse_status': status}
    try:
        for label, raw in vba_dir_streams(path):
            try:
                parsed = parse_dir_stream(raw)
            except Exception as exc:
                status = 'truncated' if isinstance(exc, (EOFError, struct.error)) else 'unreadable:' + type(exc).__name__
                rows.append(row(status))
                continue
            refs = []
            for item in parsed['references']:
                info = parse_libid(item.get('libid') or '')
                if item['kind'] == 'project':
                    if info['version'] is None:
                        info['version'] = f"{item['major']}.{item['minor']}"
                    info['file'] = None
                elif item['kind'] == 'control':
                    original = parse_libid(item.get('original') or '')
                    info['cache_guid'] = info['guid']
                    if original['guid'] is not None:
                        info.update({k: v for k, v in original.items() if v is not None})
                refs.append({'kind': item['kind'], 'name': item.get('name'), **info})
            rows.append(row('readable', parsed['project'], parsed['modules'], refs))
        if not rows:
            rows.append(row('unreadable:no-project-record'))
    except Exception as exc:
        status = 'unreadable:' + type(exc).__name__
        try:
            if type(exc).__name__ == 'OleFileError' and 1040000 <= path.stat().st_size <= 1048576:
                status = 'unreadable:file-cut-at-1MiB'
        except OSError:
            pass
        rows.append(row(status))
    return rows


def scan_local(root: Path, indexed: dict[str, str]):
    for path in root.rglob('*'):
        if not path.is_file():
            continue
        ext = path.suffix.lower()
        try:
            rel_parts = path.relative_to(root).parts
        except ValueError:
            rel_parts = ()
        if ext not in WORKBOOK_EXTS and not (ext == '.bin' and rel_parts and rel_parts[0] == 'fuse-vba'):
            continue
        try:
            sha = sha256_file(path)
        except OSError:
            continue
        name = indexed.get(sha) or local_set(path, root, indexed)
        fmt = 'xls' if ext == '.bin' else ext.lstrip('.')
        yield path, sha, name, fmt


def add_listing_xlsx(listing: Path, workbooks: dict, per_set: dict):
    """Count un-downloaded R2 xlsx-family listing rows, without opening objects."""
    if not listing.exists():
        return Counter()
    c = Counter()
    try:
        lines = listing.read_text(encoding='utf-8').splitlines()
    except OSError:
        return c
    for line in lines:
        if '\\t' not in line:
            continue
        path, size = line.rsplit('\\t', 1)
        ext = Path(path).suffix.lower()
        if ext not in {'.xlsx', '.xltx'}:
            continue
        c['listed_objects'] += 1
        c['bytes'] += int(size)
        c[ext] += 1
    c['unique_by_sha256'] = 0  # These objects were intentionally not downloaded.
    return c


def reference_key(ref: dict):
    if ref.get('kind') == 'project':
        return ref.get('name') or 'UNKNOWN_PROJECT_REFERENCE'
    return ref.get('guid') or 'UNKNOWN_' + str(ref.get('kind', 'reference')).upper()


def coverage_summaries(projects: list[dict], vba_workbooks: set[str]):
    by_book = defaultdict(set)
    bad_projects = set()
    meta = defaultdict(lambda: {'names': set(), 'versions': set(), 'workbooks': set()})
    for row in projects:
        if row.get('parse_status') != 'readable':
            bad_projects.add(row['sha256'])
            continue
        sha = row['sha256']
        by_book[sha]  # Preserve readable projects with an empty reference set.
        for ref in row.get('references', []):
            key = reference_key(ref)
            by_book[sha].add(key)
            item = meta[key]
            if ref.get('name'):
                item['names'].add(ref['name'])
            if ref.get('version') is not None:
                item['versions'].add(str(ref['version']))
            item['workbooks'].add(sha)
    always_labels = {'vba', 'excel', 'stdole', 'office'}
    always_guids = {'{000204EF-0000-0000-C000-000000000046}',
                    '{00020813-0000-0000-C000-000000000046}',
                    '{00020430-0000-0000-C000-000000000046}',
                    '{2DF8D04C-5BFA-101B-BDE5-00AA0044DE52}'}
    always_keys = {key for key, item in meta.items()
                   if key.upper() in always_guids or key.casefold() in always_labels or
                   any(n.casefold() in always_labels for n in item['names'])}
    def solve(given: bool):
        sets = {}
        for sha in vba_workbooks:
            refs = by_book.get(sha)
            # A VBA workbook whose project could not be read cannot be claimed covered.
            sets[sha] = None if (refs is None or sha in bad_projects) else (refs - always_keys if given else set(refs))
        counts = Counter(frozenset(r) for r in sets.values() if r is not None)
        small = {r: c for r, c in counts.items() if len(r) <= 5}
        # Exact, per k: an optimal k-library set is (up to padding) the union of the reference
        # sets it covers. Depth-first over reference sets in descending count, each step adding
        # one set that still fits in k libraries; prune when the current score plus every
        # remaining set that could still fit cannot beat the best found. Memory stays small.
        order = sorted(small.items(), key=lambda p: (-p[1], sorted(p[0])))

        def value(u):
            return sum(c for r, c in order if r <= u)

        def exact(k):
            best = [value(frozenset()), frozenset()]
            seen = set()

            def dfs(i, u, val):
                if val > best[0] or (val == best[0] and len(u) < len(best[1])):
                    best[0], best[1] = val, u
                fits = [(j, r, c) for j, (r, c) in enumerate(order[i:], i)
                        if not r <= u and len(u | r) <= k]
                if val + sum(c for _, _, c in fits) <= best[0]:
                    return
                for j, r, c in fits:
                    v = u | r
                    if v in seen:
                        continue
                    seen.add(v)
                    dfs(j + 1, v, value(v))
            dfs(0, frozenset(), best[0])
            return best[0], sorted(best[1]), len(seen)

        rows, greedy_keys = [], []
        for k in range(1, 6):
            best = exact(k)
            searched = best[2]
            while len(greedy_keys) < k:
                pool = sorted({key for r in small for key in r} - set(greedy_keys))
                if not pool:
                    break
                gains = [(sum(c for r, c in small.items() if r <= set(greedy_keys + [key])), key) for key in pool]
                top = max(g for g, _ in gains)
                greedy_keys.append(min(key for g, key in gains if g == top))
            greedy_set = set(greedy_keys[:k])
            rows.append({'k': k, 'exact_libraries': best[1], 'exact_workbooks': best[0], 'unions_searched': searched,
                         'greedy_libraries': greedy_keys[:k],
                         'greedy_workbooks': sum(c for r, c in small.items() if r <= greedy_set)})
        return {'given_always': given, 'always_library_keys': sorted(always_keys),
                'readable_vba_workbooks': sum(1 for r in sets.values() if r is not None),
                'distinct_reference_sets': len(counts),
                'workbooks_unreadable_or_truncated': sum(1 for r in sets.values() if r is None),
                'workbooks_covered_by_empty_set': counts.get(frozenset(), 0),
                'answers': rows}

    return {'all_references': solve(False), 'four_always_present_given': solve(True)}, meta


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--corpora', type=Path, default=Path.home() / 'corpora')
    ap.add_argument('--index', type=Path, default=Path.home() / 'corpus-meta/all-files.kode-fi.jsonl')
    ap.add_argument('--listing', type=Path, default=Path.home() / 'corpus-meta/r2-windows-workbooks.tsv')
    ap.add_argument('--out', type=Path, default=Path.home() / 'corpus-meta/vba-census')
    ap.add_argument('--summaries-from-existing', action='store_true', help='rebuild references and coverage from projects.jsonl')
    args = ap.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    if args.summaries_from_existing:
        projects_path = args.out / 'projects.jsonl'
        projects = [json.loads(line) for line in projects_path.open(encoding='utf-8') if line.strip()]
        for row in projects:
            if str(row.get('set', '')).startswith('libs-'):
                row['set'] = row['set'][5:]
        with projects_path.open('w', encoding='utf-8') as out:
            for row in projects:
                out.write(json.dumps(row, ensure_ascii=False, sort_keys=True) + '\n')
        summary_path = args.out / 'summary.json'
        summary = json.loads(summary_path.read_text(encoding='utf-8'))
        vba_workbooks = {row['sha256'] for row in projects}
        coverage, ref_meta = coverage_summaries(projects, vba_workbooks)
        with (args.out / 'references.tsv').open('w', encoding='utf-8') as out:
            out.write('library_key\tdisplay_name\tworkbooks\tpercent_of_vba_workbooks\tversions_seen\n')
            for key, item in sorted(ref_meta.items(), key=lambda pair: (-len(pair[1]['workbooks']), pair[0])):
                names = sorted(item['names'])
                out.write('\t'.join((key, ' | '.join(names) or key, str(len(item['workbooks'])),
                                     f"{(100 * len(item['workbooks']) / len(vba_workbooks)) if vba_workbooks else 0:.4f}",
                                     ', '.join(sorted(item['versions'])))) + '\n')
        normalized_sets = {}
        for name, counts in summary.get('sets', {}).items():
            target = name[5:] if name.startswith('libs-') else name
            target_counts = normalized_sets.setdefault(target, Counter())
            target_counts.update(counts)
        summary['sets'] = {name: dict(counts) for name, counts in sorted(normalized_sets.items())}
        for counts in summary.get('sets', {}).values():
            counts['vba_percent'] = round(100 * counts.get('vba_workbooks', 0) / counts.get('workbooks', 1), 4) if counts.get('workbooks') else 0.0
        overall_count = summary.get('unique_sha256_workbooks_local_and_downloaded', 0)
        overall_vba = len(vba_workbooks)
        summary['overall_sha256_known'] = {
            'workbooks': overall_count, 'vba_workbooks': overall_vba,
            'vba_percent': round(100 * overall_vba / overall_count, 4) if overall_count else 0.0,
            'xlm_workbooks': sum(c.get('xlm_workbooks', 0) for c in summary.get('sets', {}).values()),
            'container_errors': sum(c.get('container_errors', 0) for c in summary.get('sets', {}).values())}
        summary['r2_un_downloaded_xlsx_listing'] = dict(add_listing_xlsx(args.listing, {}, {}))
        summary['coverage'] = coverage
        summary['vba_workbooks_sha256'] = overall_vba
        summary['readable_project_rows'] = sum(row.get('parse_status') == 'readable' for row in projects)
        summary['truncated_project_rows'] = sum(row.get('parse_status') == 'truncated' for row in projects)
        summary['unreadable_project_rows'] = sum(str(row.get('parse_status', '')).startswith('unreadable') for row in projects)
        summary['project_rows'] = len(projects)
        (args.out / 'coverage.json').write_text(json.dumps(coverage, indent=2, sort_keys=True) + '\n', encoding='utf-8')
        summary_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + '\n', encoding='utf-8')
        print(json.dumps(summary, sort_keys=True))
        return
    indexed = indexed_sets(args.index)
    indexed_vba = indexed_vba_flags(args.index)
    project_class = None
    unique = {}
    memberships = defaultdict(set)
    for path, sha, set_name, fmt in scan_local(args.corpora, indexed):
        unique.setdefault(sha, (path, set_name, fmt))
        memberships[set_name].add(sha)
    # A local R2 staging copy already sits under _r2 and participates above.
    projects = []
    totals = defaultdict(lambda: Counter())
    flags = {}
    for sha, (path, set_name, fmt) in sorted(unique.items()):
        vba, xlm, error = inspect_container(path)
        if indexed_vba.get(sha):
            vba = True
        flags[sha] = (vba, xlm, error)
        for member_set in (s for s, hashes in memberships.items() if sha in hashes):
            c = totals[member_set]
            c['workbooks'] += 1
            if xlm:
                c['xlm_workbooks'] += 1
            if error:
                c['container_errors'] += 1
        if vba:
            for member_set in (s for s, hashes in memberships.items() if sha in hashes):
                totals[member_set]['vba_workbooks'] += 1
            projects.extend(project_rows(path, sha, set_name, fmt, project_class, xlm))
    with (args.out / 'projects.jsonl').open('w', encoding='utf-8') as out:
        for row in projects:
            out.write(json.dumps(row, ensure_ascii=False, sort_keys=True) + '\n')
    vba_workbooks = {row['sha256'] for row in projects}
    early = {'sets': {k: dict(v) for k, v in sorted(totals.items())},
             'unique_sha256_workbooks_local_and_downloaded': len(unique), 'vba_workbooks_sha256': len(vba_workbooks),
             'parse_status': dict(Counter(row.get('parse_status') for row in projects)), 'coverage': 'pending'}
    (args.out / 'summary.json').write_text(json.dumps(early, indent=2, sort_keys=True) + '\n', encoding='utf-8')
    coverage, ref_meta = coverage_summaries(projects, vba_workbooks)
    with (args.out / 'references.tsv').open('w', encoding='utf-8') as out:
        out.write('library_key\tdisplay_name\tworkbooks\tpercent_of_vba_workbooks\tversions_seen\n')
        for key, item in sorted(ref_meta.items(), key=lambda pair: (-len(pair[1]['workbooks']), pair[0])):
            names = sorted(item['names'])
            out.write('\t'.join((key, ' | '.join(names) or key, str(len(item['workbooks'])),
                                 f"{(100 * len(item['workbooks']) / len(vba_workbooks)) if vba_workbooks else 0:.4f}",
                                 ', '.join(sorted(item['versions'])))) + '\n')
    (args.out / 'coverage.json').write_text(json.dumps(coverage, indent=2, sort_keys=True) + '\n', encoding='utf-8')
    for counts in totals.values():
        counts['vba_percent'] = round(100 * counts['vba_workbooks'] / counts['workbooks'], 4) if counts['workbooks'] else 0.0
    overall_vba = len(vba_workbooks)
    overall = {'workbooks': len(unique), 'vba_workbooks': overall_vba,
               'vba_percent': round(100 * overall_vba / len(unique), 4) if unique else 0.0,
               'xlm_workbooks': sum(c['xlm_workbooks'] for c in totals.values()),
               'container_errors': sum(c['container_errors'] for c in totals.values())}
    summary = {'overall_sha256_known': overall,
               'unique_sha256_workbooks_local_and_downloaded': len(unique),
               'sets': {k: dict(v) for k, v in sorted(totals.items())},
               'r2_un_downloaded_xlsx_listing': dict(add_listing_xlsx(args.listing, unique, memberships)),
               'project_rows': len(projects),
               'vba_workbooks_sha256': len(vba_workbooks),
               'readable_project_rows': sum(row.get('parse_status') == 'readable' for row in projects),
               'truncated_project_rows': sum(row.get('parse_status') == 'truncated' for row in projects),
               'unreadable_project_rows': sum(str(row.get('parse_status', '')).startswith('unreadable') for row in projects),
               'parse_status': dict(Counter(row.get('parse_status') for row in projects)),
               'coverage': coverage}
    (args.out / 'summary.json').write_text(json.dumps(summary, indent=2, sort_keys=True) + '\n', encoding='utf-8')
    print(json.dumps(summary, sort_keys=True))


if __name__ == '__main__':
    main()
