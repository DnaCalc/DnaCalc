"""Retain supervisor text and exact saved XML; no recalculation or expected values."""
from pathlib import Path
import argparse
import collections
import csv
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
    p.add_argument('--prefix', default='')
    p.add_argument('--no-archive', action='store_true')
    args = p.parse_args()
    root = args.probe
    result_path = root / 'raw' / args.run_id / 'results.json'
    result = json.loads(result_path.read_text(encoding='utf-8-sig'))
    case = result['cases'][0]
    raw = case.get('run_value') or ''
    (root / (args.prefix + 'raw-results.tsv')).write_bytes(raw.encode('utf-8'))
    faults = [line for line in raw.splitlines() if '\tfault\t' in line or 'driver-fault:' in line]
    unavailable = [line for line in raw.splitlines() if '\tunrunnable\t' in line]
    external=[]
    external_path=root/('external-'+args.run_id+'.jsonl')
    if external_path.exists():
        external=[json.loads(line) for line in external_path.read_text(encoding='utf-8-sig').splitlines() if line]
    files = []
    for file in sorted(root.iterdir()):
        if file.suffix not in ('.xlsm', '.xlsx', '.xlam') or file.name.startswith('~$'):
            continue
        data = file.read_bytes()
        parts = {}
        with zipfile.ZipFile(file) as z:
            for name in z.namelist():
                if name in ('xl/workbook.xml', 'xl/calcChain.xml', 'xl/metadata.xml', 'xl/sharedStrings.xml') or name.startswith('xl/richData/') or re.fullmatch(r'xl/worksheets/sheet\d+\.xml', name):
                    xml = z.read(name).decode('utf-8')
                    parts[name] = {'sha256': sha(z.read(name)), 'xml': xml}
        files.append({'file': file.name, 'sha256': sha(data), 'bytes': len(data), 'parts': parts})
    (root / (args.prefix + 'saved-caches.json')).write_text(json.dumps(files, indent=2, ensure_ascii=False) + '\n', encoding='utf-8')
    overview = dict(run_id=args.run_id, supervisor_compile=case.get('compile_status'),
                    supervisor_run=case.get('run_status'), supervisor_cleanup=case.get('cleanup_status'),
                    supervisor_passed=case.get('passed'),
                    supervisor_transport_error=case.get('transport_error'),
                    explanation='Descriptor expected_value is OBSERVATION_ONLY, so passed=false is expected; compile/run/cleanup and fault records must be inspected separately.',
                    raw_sha256=sha(raw.encode('utf-8')), raw_lines=len(raw.splitlines()),
                    artifacts=len(files), faults=faults,unrunnable=unavailable,
                    external_controller_events=dict(collections.Counter(e.get('event') for e in external)),
                    external_controller_failures=[e for e in external if e.get('event') in ('controller-fault','operation-unavailable','partial-step')],
                    rows_by_case=dict(collections.Counter(line.split('\t')[0] for line in raw.splitlines() if '\t' in line)))
    (root / (args.prefix + 'capture.json')).write_text(json.dumps(overview, indent=2) + '\n', encoding='utf-8')
    grouped = collections.defaultdict(list)
    for number,line in enumerate(raw.splitlines(),1):
        fields=line.split('\t',3)
        if len(fields)==4:
            grouped[fields[0]].append((number,fields[1],fields[2],fields[3]))
    observations=[]
    for ident,rows in grouped.items():
        artifacts=[v for _,_,kind,v in rows if kind=='artifact']
        main=Path(artifacts[-1]).name if artifacts else None
        # Windows paths must also work when this dumper runs on another platform.
        if main: main=main.replace('\\','/').split('/')[-1]
        stem=main.rsplit('.',1)[0] if main else None
        observations.append(dict(case_id=ident,fixture_hash=case.get('module_sha256'),
            fixture_reference='supervisor-raw.zip: raw/'+args.run_id+'/windows-scalar-interop/OracleProbe.bas',
            platform_profile='machine.json and per-case profile/settings rows',
            setup_operations={'source':'probe.bas or recorded injected source','raw_lines':[n for n,s,k,v in rows if k=='stage' and not s.startswith(('after','tested','followup'))]},
            accepted_initial_observations=[{'line':n,'stage':s,'value':v} for n,s,k,v in rows if k=='cell' and s in ('seed','accepted-seed')],
            tested_operation=[{'line':n,'stage':s} for n,s,k,v in rows if k=='stage' and s.startswith('tested')],
            live_observations=[n for n,s,k,v in rows if k in ('cell','spill','spill-flag','formula-form','array-flags','opened-formula-form','opened-array-flags')],
            callback_trace=[n for n,s,k,v in rows if k in ('entry','exit','read','size','counter')],
            event_trace=[n for n,s,k,v in rows if k=='event'],
            saved_artifact_hashes=[{'file':f['file'],'sha256':f['sha256']} for f in files if stem and (f['file']==main or f['file'].startswith(stem+'-'))],
            outcome_class='fault-recorded' if any(k in ('fault','unrunnable') for _,_,k,_ in rows) else 'captured; no automatic behavioral verdict',
            competing_explanations='README.md',design_decision='README.md'))
    (root/(args.prefix+'observations.json')).write_text(json.dumps(observations,indent=2)+'\n',encoding='utf-8')
    native=[]
    for f in sorted(root.glob('*-native.tsv')):
        records=list(csv.DictReader(f.read_text(encoding='utf-8-sig').splitlines(),delimiter='\t'))
        native.append(dict(file=f.name,sha256=sha(f.read_bytes()),records=len(records),
            kinds=dict(collections.Counter(r.get('kind') for r in records)),
            threads=sorted({r.get('thread') for r in records}),
            handles=sorted({r.get('handle') for r in records if r.get('handle') not in (None,'0000000000000000')}),
            order=[dict(sequence=r.get('sequence'),kind=r.get('kind'),site=r.get('site'),tick=r.get('tick'),status=r.get('status'),type=r.get('type'),value=r.get('value')) for r in records]))
    if native:
        (root/(args.prefix+'native-observations.json')).write_text(json.dumps(native,indent=2)+'\n',encoding='utf-8')
    if not args.no_archive:
        with zipfile.ZipFile(root / 'supervisor-raw.zip', 'w', zipfile.ZIP_DEFLATED, compresslevel=9) as z:
            for f in sorted((root / 'raw').rglob('*')):
                if f.is_file() and not f.name.endswith('.run-claim'):
                    z.write(f, f.relative_to(root).as_posix())
    print(json.dumps({k: overview[k] for k in ('run_id', 'supervisor_compile', 'supervisor_run', 'supervisor_cleanup', 'artifacts', 'faults')}))


if __name__ == '__main__':
    main()
