#!/usr/bin/env python3
"""Read a dc-4j0e.9 owned-Excel result and inspect the saved BIFF formula tokens.

Requires the same `olefile` Python package used by tools/corpus-vba-census. This reports Excel
observations and the stored tokens; no DNA Calc Compatibility Mode comparison is claimed until
that model exists. The input is the job's JSON, never an EUSES workbook opened in Office.
"""
import argparse
import base64
import hashlib
import io
import json
import struct
import xml.etree.ElementTree as ET
import zipfile
from pathlib import Path

import olefile


def legacy_records(data):
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
    return records, len(stream)


def tokens(data, count, sheet_index=0, row_stride=1):
    records, stream_length = legacy_records(data)
    sheets = [struct.unpack_from('<I', body)[0] for _, rid, body in records if rid == 0x85 and len(body) >= 4]
    if not sheets:
        raise ValueError('no BOUNDSHEET records')
    if sheet_index >= len(sheets):
        return {}
    start = sheets[sheet_index]
    end = min((offset for offset in sheets if offset > start), default=stream_length)
    out = {}
    for pos, rid, body in records:
        if start <= pos < end and rid == 6 and len(body) >= 22:
            row, column = struct.unpack_from('<HH', body)
            if column == 2 and row % row_stride == 0 and row // row_stride < count:
                length = struct.unpack_from('<H', body, 20)[0]
                if 22 + length > len(body):
                    raise ValueError('truncated formula tokens')
                out[row // row_stride] = body[22:22 + length].hex()
    return out


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--cases', type=Path, help='optional local cases file to verify against the embedded input')
    p.add_argument('--result', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--extract-dir', type=Path)
    args = p.parse_args()
    result = json.loads(args.result.read_text(encoding='utf-8-sig'))
    embedded_cases = base64.b64decode(result.get('cases_base64') or '', validate=True)
    if args.cases:
        case_bytes = args.cases.read_bytes()
        if embedded_cases and embedded_cases != case_bytes:
            raise SystemExit('embedded Excel cases do not match the supplied cases file')
    elif embedded_cases:
        case_bytes = embedded_cases
    else:
        raise SystemExit('result has no embedded input; supply --cases for this older result')
    cases = json.loads(case_bytes.decode('utf-8-sig'))['formulas']
    schema = result.get('schema_version', 1)
    if schema not in (1, 2):
        raise SystemExit('unsupported Excel result schema: ' + str(schema))
    cases_hash = hashlib.sha256(case_bytes).hexdigest()
    if schema == 2 and result.get('cases_sha256') != cases_hash:
        raise SystemExit('Excel result cases_sha256 does not match the supplied cases file')
    if result.get('fatal'):
        raise SystemExit('Excel job failed: ' + str(result['fatal']))
    observations = result['cases']
    by_name = {}
    for row in observations:
        if row['name'] in by_name:
            raise SystemExit('duplicate observation: ' + row['name'])
        by_name[row['name']] = row
    prefixes = ('formula/', 'reopened/', 'converted/')
    if schema == 2:
        prefixes += ('modern/',)
    if result.get('formula2_cases'):
        prefixes += ('formula2/', 'reopened_formula2/', 'converted_formula2/', 'modern_formula2/')
    expected = {prefix + str(case[0]) for case in cases for prefix in prefixes}
    missing = sorted(expected - by_name.keys())
    legacy = base64.b64decode(result.get('legacy_base64') or '', validate=True)
    modern = base64.b64decode(result.get('converted_base64') or '', validate=True)
    limits = base64.b64decode(result.get('limits_base64') or '', validate=True)
    baseline = base64.b64decode(result.get('modern_baseline_base64') or '', validate=True)
    stored = tokens(legacy, len(cases)) if legacy else {}
    stored_formula2 = tokens(legacy, len(cases), sheet_index=1, row_stride=32) if legacy and result.get('formula2_cases') else {}
    limit_counts = None
    if limits:
        limit_records, _ = legacy_records(limits)
        limit_counts = {name: sum(rid == code for _, rid, _ in limit_records)
                        for name, code in [('xf', 0x00e0), ('style', 0x0293), ('defined_name', 0x0018)]}
    formulas = [{'id': case[0], 'submitted': case[1], 'entry': by_name.get('formula/' + case[0]),
                 'reopened': by_name.get('reopened/' + case[0]), 'converted': by_name.get('converted/' + case[0]),
                 'modern_baseline': by_name.get('modern/' + case[0]),
                 'formula2_entry': by_name.get('formula2/' + case[0]),
                 'formula2_reopened': by_name.get('reopened_formula2/' + case[0]),
                 'formula2_converted': by_name.get('converted_formula2/' + case[0]),
                 'formula2_modern_baseline': by_name.get('modern_formula2/' + case[0]),
                 'stored_tokens_hex': stored.get(i), 'stored_formula2_tokens_hex': stored_formula2.get(i)}
                for i, case in enumerate(cases)]
    calc_id = None
    if modern:
        with zipfile.ZipFile(io.BytesIO(modern)) as z:
            root = ET.fromstring(z.read('xl/workbook.xml'))
            calc = root.find('{http://schemas.openxmlformats.org/spreadsheetml/2006/main}calcPr')
            calc_id = None if calc is None else calc.get('calcId')
    if args.extract_dir:
        args.extract_dir.mkdir(parents=True, exist_ok=True)
        for name, data in [('cases.json', case_bytes), ('compat.xls', legacy), ('converted.xlsx', modern), ('limits.xls', limits),
                           ('modern-baseline.xlsx', baseline)]:
            if not data:
                continue
            dest = args.extract_dir / name
            if dest.exists():
                raise SystemExit('refusing to overwrite ' + str(dest))
            dest.write_bytes(data)
    report = {'formulas': formulas, 'other_observations': [r for r in observations if not r['name'].startswith(prefixes)],
              'missing_observations': missing, 'converted_calc_id': calc_id, 'limits_record_counts': limit_counts,
              'modern_baseline_available': bool(baseline),
              'cases_sha256': cases_hash, 'cases_verified': schema == 2,
              'cases_source': 'local' if args.cases else 'embedded',
              'not_run': result.get('not_run', []),
              'product_comparison': 'not available: Compatibility Mode is not modelled'}
    args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    print(f"Read {len(observations)} observations; {len(missing)} expected formula observations missing; {len(stored)}/{len(cases)} stored formula token streams found. Product comparison not available.")
    if result.get('formula2_cases'):
        print(f"Formula2: {len(stored_formula2)}/{len(cases)} stored anchor token streams found; array records remain in compat.xls.")


if __name__ == '__main__':
    main()
