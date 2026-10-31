"""Retain supervisor text and exact saved XML; no recalculation or expected values."""
from pathlib import Path
import argparse
import collections
import hashlib
import json
import re
import zipfile


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    p = argparse.ArgumentParser()
    p.add_argument('probe', type=Path)
    p.add_argument('run_id')
    args = p.parse_args()
    root = args.probe
    result_path = root / 'raw' / args.run_id / 'results.json'
    result = json.loads(result_path.read_text(encoding='utf-8-sig'))
    case = result['cases'][0]
    raw = case.get('run_value') or ''
    (root / 'raw-results.tsv').write_text(raw, encoding='utf-8')
    faults = [line for line in raw.splitlines() if '\tfault\t' in line or 'driver-fault:' in line]
    files = []
    for file in sorted(root.iterdir()):
        if file.suffix not in ('.xlsm', '.xlsx', '.xlam') or file.name.startswith('~$'):
            continue
        data = file.read_bytes()
        parts = {}
        with zipfile.ZipFile(file) as z:
            for name in z.namelist():
                if name in ('xl/workbook.xml', 'xl/calcChain.xml', 'xl/metadata.xml') or re.fullmatch(r'xl/worksheets/sheet\d+\.xml', name):
                    xml = z.read(name).decode('utf-8')
                    parts[name] = {'sha256': sha(z.read(name)), 'xml': xml}
        files.append({'file': file.name, 'sha256': sha(data), 'bytes': len(data), 'parts': parts})
    (root / 'saved-caches.json').write_text(json.dumps(files, indent=2, ensure_ascii=False) + '\n', encoding='utf-8')
    overview = dict(run_id=args.run_id, supervisor_compile=case.get('compile_status'),
                    supervisor_run=case.get('run_status'), supervisor_cleanup=case.get('cleanup_status'),
                    supervisor_passed=case.get('passed'),
                    explanation='Descriptor expected_value is OBSERVATION_ONLY, so passed=false is expected; compile/run/cleanup and fault records must be inspected separately.',
                    raw_sha256=sha(raw.encode('utf-8')), raw_lines=len(raw.splitlines()),
                    artifacts=len(files), faults=faults,
                    rows_by_case=dict(collections.Counter(line.split('\t')[0] for line in raw.splitlines() if '\t' in line)))
    (root / 'capture.json').write_text(json.dumps(overview, indent=2) + '\n', encoding='utf-8')
    with zipfile.ZipFile(root / 'supervisor-raw.zip', 'w', zipfile.ZIP_DEFLATED, compresslevel=9) as z:
        for f in sorted((root / 'raw').rglob('*')):
            if f.is_file():
                z.write(f, f.relative_to(root).as_posix())
    print(json.dumps({k: overview[k] for k in ('run_id', 'supervisor_compile', 'supervisor_run', 'supervisor_cleanup', 'artifacts', 'faults')}))


if __name__ == '__main__':
    main()
