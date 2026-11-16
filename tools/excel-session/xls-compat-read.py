#!/usr/bin/env python3
"""Read a dc-4j0e.9 owned-Excel result and inspect the saved BIFF formula tokens.

Requires the same `olefile` Python package used by tools/corpus-vba-census. This reports Excel
observations and the stored tokens; no DNA Calc Compatibility Mode comparison is claimed until
that model exists. The input is the job's JSON, never an EUSES workbook opened in Office.
"""
import argparse
import base64
import io
import json
import struct
import xml.etree.ElementTree as ET
import zipfile
from pathlib import Path

import olefile


def tokens(data, count):
    with olefile.OleFileIO(io.BytesIO(data)) as ole:
        names = {x[-1].lower(): x for x in ole.listdir()}
        stream = ole.openstream(names.get('workbook') or names['book']).read()
    records = []
    at = 0
    while at + 4 <= len(stream):
        rid, length = struct.unpack_from('<HH', stream, at)
        end = at + 4 + length
        if end > len(stream):
            raise ValueError('truncated BIFF record')
        records.append((at, rid, stream[at + 4:end]))
        at = end
    if any(rid == 0x2f for _, rid, _ in records):
        raise ValueError('encrypted workbook; tokens not inspected')
    sheets = [struct.unpack_from('<I', body)[0] for _, rid, body in records if rid == 0x85 and len(body) >= 4]
    if not sheets:
        raise ValueError('no BOUNDSHEET records')
    start = sheets[0]
    end = min((offset for offset in sheets if offset > start), default=len(stream))
    out = {}
    for pos, rid, body in records:
        if start <= pos < end and rid == 6 and len(body) >= 22:
            row, column = struct.unpack_from('<HH', body)
            if column == 2 and row < count:
                length = struct.unpack_from('<H', body, 20)[0]
                if 22 + length > len(body):
                    raise ValueError('truncated formula tokens')
                out[row] = body[22:22 + length].hex()
    return out


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--cases', type=Path, required=True)
    p.add_argument('--result', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--extract-dir', type=Path)
    args = p.parse_args()
    cases = json.loads(args.cases.read_text(encoding='utf-8'))['formulas']
    result = json.loads(args.result.read_text(encoding='utf-8-sig'))
    if result.get('fatal'):
        raise SystemExit('Excel job failed: ' + str(result['fatal']))
    observations = result['cases']
    by_name = {}
    for row in observations:
        if row['name'] in by_name:
            raise SystemExit('duplicate observation: ' + row['name'])
        by_name[row['name']] = row
    expected = {prefix + str(case[0]) for case in cases for prefix in ('formula/', 'reopened/', 'converted/')}
    missing = sorted(expected - by_name.keys())
    legacy = base64.b64decode(result.get('legacy_base64') or '', validate=True)
    modern = base64.b64decode(result.get('converted_base64') or '', validate=True)
    stored = tokens(legacy, len(cases)) if legacy else {}
    formulas = [{'id': case[0], 'submitted': case[1], 'entry': by_name.get('formula/' + case[0]),
                 'reopened': by_name.get('reopened/' + case[0]), 'converted': by_name.get('converted/' + case[0]),
                 'stored_tokens_hex': stored.get(i)}
                for i, case in enumerate(cases)]
    calc_id = None
    if modern:
        with zipfile.ZipFile(io.BytesIO(modern)) as z:
            root = ET.fromstring(z.read('xl/workbook.xml'))
            calc = root.find('{http://schemas.openxmlformats.org/spreadsheetml/2006/main}calcPr')
            calc_id = None if calc is None else calc.get('calcId')
    if args.extract_dir:
        args.extract_dir.mkdir(parents=True, exist_ok=True)
        for name, data in [('compat.xls', legacy), ('converted.xlsx', modern)]:
            if not data:
                continue
            dest = args.extract_dir / name
            if dest.exists():
                raise SystemExit('refusing to overwrite ' + str(dest))
            dest.write_bytes(data)
    report = {'formulas': formulas, 'other_observations': [r for r in observations if not r['name'].startswith(('formula/', 'reopened/', 'converted/'))],
              'missing_observations': missing, 'converted_calc_id': calc_id, 'not_run': result.get('not_run', []),
              'product_comparison': 'not available: Compatibility Mode is not modelled'}
    args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    print(f"Read {len(observations)} observations; {len(missing)} expected formula observations missing; {len(stored)}/{len(cases)} stored formula token streams found. Product comparison not available.")


if __name__ == '__main__':
    main()
