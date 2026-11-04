"""Queue P14 pure controls and seeded conditions; retain unrun obligations explicitly.

Original MIT tooling over iteration-job.ps1 and the existing Set-StoredValues helper.
The eng9 profile shares a fresh process within each condition. The eng10 profile requires a
fresh owned process per trial. Counter/observer and UI lanes remain explicit omissions.
No answers are supplied.
"""
import argparse
import datetime as dt
import hashlib
import json
from pathlib import Path
import shutil
import struct
import subprocess
import time
import uuid
import zipfile
import xml.etree.ElementTree as ET


def bits(value):
    return struct.pack('>d', value).hex()


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False)+'\n', encoding='utf-8')


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def reviewed_unstarted(receipt_path, record, reviews):
    """Recognize an explicitly reviewed startup failure without counting observations."""
    review = reviews.get(record['id'])
    if review is None:
        return False
    job = record['job']
    request = Path(job['parameters']['Request'])
    result = Path(job['parameters']['Result'])
    if (record['success'] or record.get('baseline_before') is not None
            or record.get('excel') is not None or record.get('result_sha256')
            or result.exists() or (receipt_path.parent/'worker.log').exists()
            or Path(job['script']).name != 'condition-job.ps1'):
        raise ValueError('Reviewed startup failure contains worker-entry evidence: '+record['id'])
    if (sha(receipt_path) != review['receipt_sha256'] or sha(request) != review['request_sha256']
            or sha(Path(review['host_source'])) != record['host_script_sha256']
            or not review['no_trial_results_at_review']):
        raise ValueError('Startup failure review binding differs: '+record['id'])
    rows = json.loads(request.read_text(encoding='utf-8-sig'))['trials']
    if {str(Path(t['request'])):sha(Path(t['request'])) for t in rows} != review['trial_request_sha256']:
        raise ValueError('Startup failure trial requests differ: '+record['id'])
    return True


def eng10_conditions(add):
    """New questions only; values below are declared seeds, never expected answers."""
    thresholds = ('3fbfffffffffffff','3fc0000000000000','3fc0000000000001','0000000000000000')
    for operation in ('Calculate','Full','Rebuild','Range'):
        for label,formula,count in (('eighth','=A1+0.125',5),('half','=A1/2+1',100)):
            for threshold in thresholds:
                add(f'E10-A2-{operation}-{label}-{threshold}',dict(A1=formula),dict(A1=0),count,
                    change_bits=threshold,operation=operation,range='A1',formula_entry='Formula2')
    for count in (5,100):
        for change in (1,8):
            for independent in (False,True):
                formulas=dict(A1='=A1/2+1',B1='=A1*1000000',C1='=C1+0.125',D1='=C1+1')
                if independent:formulas['E1']='=1000000'
                add(f'E10-A3-n{count}-d{change}-independent-{int(independent)}',formulas,
                    {a:0 for a in formulas},count,change,range='A1:E1',formula_entry='Formula2')
    for count in (1,2,5):
        for change in (0,.001):
            for peer in (False,True):
                formulas=dict(A1='=IF(ISERROR(A1),1/0,NA())')
                if peer:formulas.update(B1='=ISERROR(A1)',C1='=C1+0.125',D1='=C1+1')
                add(f'E10-B6-error-n{count}-{bits(change)}-peer-{int(peer)}',formulas,
                    {a:0 for a in formulas},count,change,formula_entry='Formula2')
            for label,formula,seed in (
                ('number-text-error','=IF(ISNUMBER(A1),"text",NA())',0),
                ('error-text','=IF(ISERROR(A1),"text",NA())',['e','#N/A'])):
                formulas=dict(A1=formula,B1='=ISERROR(A1)',C1='=C1+0.125',D1='=C1+1')
                add(f'E10-B6-{label}-n{count}-{bits(change)}',formulas,
                    dict(A1=seed,B1=0,C1=0,D1=0),count,change,formula_entry='Formula2')
        for threshold in ('3fefffffffffffff','3ff0000000000000','3ff0000000000001','0000000000000000'):
            formulas=dict(A1='=NOT(A1)',B1='=ISERROR(A1)',C1='=C1+0.125',D1='=C1+1')
            add(f'E10-B6-boolean-n{count}-{threshold}',formulas,{a:0 for a in formulas},
                count,change_bits=threshold,formula_entry='Formula2')
    ranges=(('cycle','A1:B1'),('cycle-reader','A1:C1'),('mixed','A1:D1'),
            ('reader','C1'),('acyclic','D1'),('reader-acyclic','C1:D1'),('all-readers','A1:E1'))
    for asymmetric in (False,True):
        for count in (1,2,5):
            for label,target in ranges:
                formulas=dict(A1='=B1+1',B1='=A1+1',C1='=A1+1',D1='=2+2',E1='=C1+1')
                seeds=dict(A1=77 if asymmetric else 0,B1=55 if asymmetric else 0,
                           C1=999 if asymmetric else 0,D1=0,E1=999 if asymmetric else 0)
                add(f'E10-Range-{label}-n{count}-'+('asymmetric' if asymmetric else 'zero'),
                    formulas,seeds,count,operation='Range',range=target,formula_entry='Formula2',
                    provenance='After-command observations do not prove that held cyclic or outside-range cells ran.')


def supplied_conditions(paths, add):
    """Use authored question specifications without repeating the older populations."""
    seen=set()
    for path in paths:
        document=json.loads(Path(path).read_text(encoding='utf-8-sig'))
        for row in document['conditions']:
            if row['id'] in seen or row.get('expected') is not None:
                raise ValueError('Duplicate condition or non-null expected answer: '+row['id'])
            if not row.get('formulas') or not row.get('seeds') or row.get('formula_entry')!='Formula2':
                raise ValueError('A condition needs exact formulas, seeds and Formula2 form: '+row['id'])
            seen.add(row['id'])
            item=dict(row)
            ident=item.pop('id');formulas=item.pop('formulas');seeds=item.pop('seeds')
            iterations=item.pop('iterations')
            add(ident,formulas,seeds,iterations,**item)


def run(args):
    checkout=Path(args.checkout).resolve()
    packet=Path(args.packet).resolve()
    output=Path(args.output).resolve()
    session=Path(args.session).resolve()
    output.mkdir(parents=True, exist_ok=False)
    run_id=uuid.uuid4().hex
    shutil.copyfile(__file__,output/'run-iteration-packet.py')
    shutil.copytree(packet, output/'packet')
    write(output/'packet-sha256.json', {p.relative_to(packet).as_posix():sha(p) for p in packet.rglob('*') if p.is_file()})
    source=checkout/'tools/excel-session/iteration-job.ps1'
    event_source=checkout/'tools/excel-session/events-job.ps1'
    condition_source=checkout/'tools/excel-session/condition-job.ps1'
    client=checkout/'tools/excel-session/submit.ps1'
    source_root=checkout
    if args.profile=='eng10':
        # Execute the frozen reader, seed helper and client after later repository pulls too.
        source_root=output/'source'
        for original in (source,condition_source,client,client.with_name('host.ps1'),
                         checkout/'crates/calc-engine/tests/fixtures/make-dc-y2o-oracles.ps1',
                         checkout/'platform/calc-oracle-excel/worker/capture-worker.ps1'):
            target=source_root/original.relative_to(checkout)
            target.parent.mkdir(parents=True,exist_ok=True)
            target.write_bytes(original.read_bytes())
        source=source_root/'tools/excel-session/iteration-job.ps1'
        condition_source=source_root/'tools/excel-session/condition-job.ps1'
        client=source_root/'tools/excel-session/submit.ps1'
    shutil.copyfile(source, output/'iteration-job.ps1')
    shutil.copyfile(event_source, output/'events-job.ps1')
    shutil.copyfile(condition_source, output/'condition-job.ps1')
    sources={'tools/excel-session/iteration-job.ps1':source,
             'tools/excel-session/events-job.ps1':event_source,
             'tools/excel-session/condition-job.ps1':condition_source,
             'tools/excel-session/host.ps1':client.with_name('host.ps1'),
             'tools/excel-session/submit.ps1':client,
             'crates/calc-engine/tests/fixtures/make-dc-y2o-oracles.ps1':source_root/'crates/calc-engine/tests/fixtures/make-dc-y2o-oracles.ps1',
             'platform/calc-oracle-excel/worker/capture-worker.ps1':source_root/'platform/calc-oracle-excel/worker/capture-worker.ps1'}
    write(output/'sources-sha256.json', {name:sha(p) for name,p in sources.items()})
    conditions=[]
    def condition(ident, formulas, seeds, iterations=5, change=.001, **options):
        row=dict(id=ident, formulas=formulas, seeds=seeds, iterations=iterations,
                 change_bits=bits(change), iteration=True, operation='Rebuild', commands=2, range='A1:D1',
                 reverse_entry=False, reverse_chain=False)
        row.update(options)
        conditions.append(row)
    # The requested initial controls precede the new questions.
    controls=[]
    for name in (() if getattr(args,'conditions_file',None) else ('dc_86f','dc_10y_iteration')):
        path=checkout/f'crates/calc-engine/tests/fixtures/{name}/oracle.xlsx'
        if args.profile=='eng10':
            frozen=output/'controls'/name/'oracle.xlsx'
            frozen.parent.mkdir(parents=True,exist_ok=True)
            frozen.write_bytes(path.read_bytes())
            path=frozen
        with zipfile.ZipFile(path) as archive:
            root=ET.fromstring(archive.read('xl/workbook.xml'))
            calc=root.find('{*}calcPr').attrib
        controls.append(dict(id='control-'+name, input=str(path), input_sha256=sha(path), iteration=calc.get('iterate') in ('1','true'),
                             iterations=int(calc.get('iterateCount','100')), change_bits=bits(float(calc.get('iterateDelta','.001'))),
                             operation='Rebuild', commands=1, range='A1:D1'))
    for asymmetric in (False,True):
        for count in (1,2,3,5,100):
            condition(f'A1-sweeps-{count}-'+('asymmetric' if asymmetric else 'zero'),
                      dict(A1='=B1+1', B1='=A1+1', C1='=C1+1', D1='=SUM(A1:B1)'),
                      dict(A1=77 if asymmetric else 0, B1=55 if asymmetric else 0,
                           C1=100 if asymmetric else 0, D1=999 if asymmetric else 0), count)
    for formula,label,count in (('=A1+0.125','eighth',5),('=A1/2+1','half',100)):
        for threshold in ('3fbfffffffffffff','3fc0000000000000','3fc0000000000001','0000000000000000'):
            condition('A2-'+label+'-'+threshold,dict(A1=formula),dict(A1=0),count,change_bits=threshold)
    for label,formulas in (
        ('both-amplifiers',dict(A1='=A1/2+1',B1='=A1*1048576',C1='=C1+0.125',D1='=C1*1048576')),
        ('both-alone',dict(A1='=A1/2+1',C1='=C1+0.125')),
        ('half-amplifier',dict(A1='=A1/2+1',B1='=A1*1048576')),
        ('eighth-amplifier',dict(C1='=C1+0.125',D1='=C1*1048576')),
        ('half-alone',dict(A1='=A1/2+1')),('eighth-alone',dict(C1='=C1+0.125'))):
        condition('A3-'+label,formulas,{a:0 for a in formulas},change=.2)
    condition('A4-pure',dict(A1='=A1/2+1',B1='=A1*1048576',C1='=B1+1'),dict(A1=0,B1=0,C1=0))
    for label,formula,seed in (
        ('stable-text','=IF(A1=0,"x","x")',0),('toggle-text','=IF(A1="a","b","a")',['str','a']),
        ('boolean','=NOT(A1)',0),('NA','=IF(A1=0,NA(),NA())',0),
        ('changing-error','=IF(ISERROR(A1),1/0,NA())',0),
        ('number-to-text','=IF(ISNUMBER(A1),"text",1)',0),('untaken-self','=IF(TRUE,1,A1+1)',0)):
        condition('B6-'+label,dict(A1=formula),dict(A1=seed))
    for dynamic in (True,False):
        condition('B7-'+('indirect' if dynamic else 'static'),
                  dict(A1='=INDIRECT(IF(A1<2,"B1","C1"))+1' if dynamic else '=IF(A1<2,B1,C1)+1',B1='=A1/2'),
                  dict(A1=0,B1=0),values=dict(C1=10))
    # Remaining calculation scopes and reverse-entry/chain histories are independent conditions.
    first=[dict(row) for row in conditions if row['id'].startswith('A1-sweeps')]
    for row in first:
        for operation in ('Calculate','Full','Sheet','Range'):
            conditions.append(dict(row,id=row['id']+'-'+operation,operation=operation))
        conditions.append(dict(row,id=row['id']+'-reverse',reverse_entry=True,reverse_chain=True))
    event_controls=[]
    if args.profile=='eng9':
        event_input=checkout/'crates/calc-engine/tests/fixtures/excel-calc-probes/P06/manual-false-save-log-01-tested.xlsm'
        event_controls=[dict(id='P22-P06-'+case,case=case,kind='events',input=str(event_input),input_sha256=sha(event_input))
                        for case in (f'manual-{before}-{method}-{events}' for before in ('false','true')
                                     for method in ('save','saveas','copy') for events in ('off','log','edit'))]
    fresh_per_trial = args.profile == 'eng10'
    if fresh_per_trial:
        conditions.clear()
        if getattr(args,'conditions_file',None):
            supplied_conditions(args.conditions_file,condition)
            write(output/'supplied-specifications.json',[dict(path=str(Path(p).resolve()),sha256=sha(Path(p))) for p in args.conditions_file])
        else:
            eng10_conditions(condition)
        event_controls=[]
    write(output/'conditions.json',dict(controls=controls,prepared=conditions,event_controls=event_controls,trials=10,
        fresh_process_per_trial=fresh_per_trial,profile=args.profile,
        omissions=(['Counter, PairLog and PeekPair observers; no SCC execution counts or mid-cycle visibility are inferred.',
                    'Physical F9/UI command, MTC-on/worker variations, tab/chain histories and native cancellation remain unrun.',
                    'Range observations may be held or outside the requested range; no calculated provenance is inferred from equality.',
                    'A refused preparation or setting leaves the requested fresh trials unrun; no threshold is substituted.',
                    'OxVba-dependent supervisors are not invoked; that checkout is excluded.'] if fresh_per_trial else
                   ['P14 observer/counter UDF lanes, callback order and mixed-sweep reads',
                   'P14 MTC observer lanes and live native/UI cancellation',
                   'P14 reversed tab order and dynamic-reference post-edit dependencies',
                   'P14/P25 cross-workbook cyclic domains',
                   'New P22 Change/Calculate event placement, cancellation and reentrancy; existing P06 save controls are replayed separately',
                   'Existing OxVba-dependent supervisor is not invoked: that checkout is excluded by owner instruction.']),
        execution='Lane owned-PID queue; existing capture typed reads and Set-StoredValues helper; pure COM observations.'))
    if args.dry_run:
        write(output/'preview-status.json',dict(profile=args.profile,conditions=len(controls)+len(conditions)+len(event_controls),
            trials=10*(len(controls)+len(conditions)+len(event_controls)),fresh_process_per_trial=fresh_per_trial,excel_launches=0))
        return
    inputs=output/'inputs';inputs.mkdir()
    preparation=output/'prepare/request.json'
    write(preparation,dict(action='prepare',conditions=conditions,inputs_directory=str(inputs)))
    params=json.dumps(dict(Request=str(preparation),Result=str(preparation.with_name('result.json'))))
    with (output/'prepare/console.log').open('w',encoding='utf-8') as log:
        subprocess.run(['pwsh','-NoProfile','-STA','-File',str(client),
                        '-SessionDirectory',str(session),'-Script',str(source),'-Parameters',params,
                        '-Environment','iteration-setup','-PacketId',str(output),'-TimeoutSeconds','900'],cwd=checkout,stdout=log,stderr=subprocess.STDOUT,check=True)
    receipt_line=next(line for line in (output/'prepare/console.log').read_text(encoding='utf-8-sig').splitlines()
                      if line.startswith('dnacalc-excel-session/1 receipt '))
    preparation_receipt=Path(receipt_line.split(' receipt ',1)[1])
    preparation_record=json.loads(preparation_receipt.read_text(encoding='utf-8-sig'))
    if preparation_record['job'].get('packet_id')!=str(output):raise ValueError('Preparation receipt belongs to another packet')
    shutil.copytree(preparation_receipt.parent,output/'prepare/session')
    preparation_result=json.loads(preparation.with_name('result.json').read_text(encoding='utf-8-sig'))
    prepared={r['id']:r for r in preparation_result['prepared'] if r['success']}
    failed_preparations=[r for r in preparation_result['prepared'] if not r['success']]
    write(output/'preparation-failures.json',failed_preparations)
    for row in conditions:
        if row['id'] in prepared:row['input_sha256']=prepared[row['id']]['sha256']
    jobs=[]
    sweeps=[r for r in conditions if r['id'].startswith('A1-sweeps') and r['operation']=='Rebuild' and not r['reverse_entry']]
    scopes=[r for r in conditions if r['id'].startswith('A1-sweeps-1-zero-')]
    finite=[r for r in conditions if r['id'].startswith(('A2-','A3-','A4-'))]
    nonnumeric=[r for r in conditions if r['id'].startswith(('B6-','B7-'))]
    priority=sweeps+scopes+finite+nonnumeric
    remainder=[r for r in conditions if r not in priority]
    requested_rows=controls+priority+event_controls+remainder
    for index,row in enumerate(requested_rows):
        if row['id'] not in prepared and 'input' not in row:continue
        trial_requests=[]
        for trial in range(1,11):
            directory=output/'trials'/row['id']/f'{trial:02d}'
            request=directory/'request.json'
            input_path=row.get('input',str(inputs/(row['id']+'.xlsx')))
            is_event=row.get('kind')=='events'
            write(request,dict(case=row['case'],trial=trial,input=input_path,input_sha256=row['input_sha256']) if is_event else
                  dict(action='trial',condition=row,trial=trial,input=input_path,input_sha256=row['input_sha256']))
            trial_requests.append(dict(trial=trial,request=str(request),result=str(directory/'result.json')))
        # VBA event subscriptions are lifecycle state: those trials each require a fresh process.
        groups=[[trial] for trial in trial_requests] if is_event or fresh_per_trial else [trial_requests]
        for group in groups:
            suffix=f'-{group[0]["trial"]:02d}' if is_event or fresh_per_trial else ''
            ident=f'{args.profile}-{run_id}-{index:03d}'+suffix
            directory=output/'condition-jobs'/(row['id']+suffix)
            request=directory/'request.json'
            write(request,dict(condition=row['id'],worker=str(event_source if is_event else source),trials=group,
                process_scope='One fresh owned Excel process for this single trial.' if fresh_per_trial else
                              'One fresh process per condition; workbook state and baseline reset between trials.'))
            job=dict(id=ident,script=str(condition_source),parameters=dict(Request=str(request),Result=str(directory/'result.json')),
                     environment='iteration-manual-mtc-off',working_directory=str(checkout),fresh_required=True,timeout_seconds=600,packet_id=str(output))
            temp=session/'queue'/(ident+'.tmp');write(temp,job);temp.rename(temp.with_suffix('.json'))
            jobs.append((ident,row['id'],len(group),directory))
    deadline=dt.datetime.fromisoformat(args.until.replace('Z','+00:00'))
    completed=set()
    heartbeat=0
    try:
        while len(completed)<len(jobs) and dt.datetime.now(dt.timezone.utc)<deadline:
            if time.monotonic()>=heartbeat:
                with (output/'host-recovery.log').open('a',encoding='utf-8') as log:
                    subprocess.run(['pwsh','-NoProfile','-STA','-File',str(client),
                                    '-SessionDirectory',str(session),'-Start'],cwd=checkout,stdout=log,stderr=subprocess.STDOUT,check=True,timeout=60)
                heartbeat=time.monotonic()+60
            for ident,condition_id,trial,directory in jobs:
                receipt=session/'completed'/ident/'receipt.json'
                if ident not in completed and receipt.is_file():
                    record=json.loads(receipt.read_text(encoding='utf-8-sig'))
                    if record['id']!=ident or record['job'].get('packet_id')!=str(output) or Path(record['job']['parameters']['Request']).resolve()!=directory/'request.json':
                        raise ValueError('Receipt is not bound to this campaign/request')
                    shutil.copytree(receipt.parent,directory/'session',dirs_exist_ok=True)
                    completed.add(ident)
                    with (output/'progress.jsonl').open('a',encoding='utf-8') as log:
                        trial_number=json.loads((directory/'request.json').read_text(encoding='utf-8-sig'))['trials'][0]['trial'] if fresh_per_trial else trial
                        log.write(json.dumps(dict(id=ident,condition=condition_id,trial=trial_number,trial_count=trial,success=record['success'],
                                                  error=record['error'],launches=record['launches_after']-record['launches_before']))+'\n')
            time.sleep(.5)
    finally:
        unrun=[dict(condition=r['id'],trials=10,reason='preparation failed',error=r['error']) for r in failed_preparations]
        for ident,condition_id,trial,directory in jobs:
            queued=session/'queue'/(ident+'.json')
            if queued.exists():
                try:
                    queued.rename(queued.with_suffix('.unrun'))
                    unrun.append(dict(id=ident,condition=condition_id,trials=trial))
                except FileNotFoundError:
                    pass  # The host may have claimed it after the directory read.
        # Finish an already active condition; never interrupt a measured operation at the cutoff.
        for ident,condition_id,trial,directory in jobs:
            receipt=session/'completed'/ident/'receipt.json'
            if ident not in completed and (session/'running'/(ident+'.json')).exists():
                limit=time.monotonic()+600
                while not receipt.exists() and time.monotonic()<limit:time.sleep(.2)
                if receipt.exists():
                    record=json.loads(receipt.read_text(encoding='utf-8-sig'))
                    if record['id']!=ident or record['job'].get('packet_id')!=str(output) or Path(record['job']['parameters']['Request']).resolve()!=directory/'request.json':
                        raise ValueError('Final receipt is not bound to this campaign/request')
                    shutil.copytree(receipt.parent,directory/'session',dirs_exist_ok=True);completed.add(ident)
        # Reconcile receipts that completed after the last poll but before cutoff cleanup.
        for ident,condition_id,trial,directory in jobs:
            receipt=session/'completed'/ident/'receipt.json'
            if ident not in completed and receipt.is_file():
                record=json.loads(receipt.read_text(encoding='utf-8-sig'))
                if record['id']!=ident or record['job'].get('packet_id')!=str(output) or Path(record['job']['parameters']['Request']).resolve()!=directory/'request.json':
                    raise ValueError('Cutoff receipt is not bound to this campaign/request')
                shutil.copytree(receipt.parent,directory/'session',dirs_exist_ok=True)
                completed.add(ident)
        # A caller may already have deferred a request, or its queue file may be missing.
        # Every requested job without a completed receipt still belongs in the unrun list.
        unrun_ids={row.get('id') for row in unrun}
        for ident,condition_id,trial,directory in jobs:
            if ident not in completed and ident not in unrun_ids:
                unrun.append(dict(id=ident,condition=condition_id,trials=trial,reason='No completed session receipt at cutoff; includes externally deferred or missing requests.'))
        receipts=[json.loads((directory/'session/receipt.json').read_text(encoding='utf-8-sig')) for ident,_,_,directory in jobs if ident in completed]
        outer_success={r['id']:r['success'] for r in receipts}
        trials=[]
        for ident,_,_,directory in jobs:
            path=directory/'result.json'
            if ident in completed and path.is_file():
                trials.extend(dict(t,host_success=outer_success[ident]) for t in json.loads(path.read_text(encoding='utf-8-sig')).get('trials',[]))
        expected_jobs=sum(10 if fresh_per_trial or r.get('kind')=='events' else 1 for r in requested_rows)
        write(output/'run-status.json',dict(profile=args.profile,requested_conditions=len(requested_rows),requested_jobs=expected_jobs,queued_jobs=len(jobs),completed_jobs=len(completed),requested_trials=10*len(requested_rows),completed_trials=len(trials),
            successful_trials=sum(r['success'] and r['host_success'] for r in trials),
            worker_successful_trials=sum(r['success'] for r in trials),
            failed_trials=sum(not (r['success'] and r['host_success']) for r in trials),
            excel_launches=sum(r['launches_after']-r['launches_before'] for r in receipts)+preparation_record['launches_after']-preparation_record['launches_before'],
            preparation_launches=preparation_record['launches_after']-preparation_record['launches_before'],unrun=unrun,
            failed_jobs=sum(not r['success'] for r in receipts),failed_preparations=failed_preparations,
            planned_jobs_complete=len(completed)==expected_jobs and all(r['success'] for r in receipts),packet_complete=False,complete=False,
            process_scope=('Every queued trial uses one fresh owned Excel process; compare PID/start-time tuples in receipts.' if fresh_per_trial else
                           'Pure conditions: one fresh process, ten workbook trials with baseline resets. P06 event lifecycle trials: each in a fresh process. Trial order and shared pure-condition process memory are recorded.'),
            omissions='See conditions.json; unrun and failed trials are not passes.'))
        write(output/'results-sha256.json',{p.relative_to(output).as_posix():sha(p) for p in output.rglob('*') if p.is_file()})



def resume_eng10(args):
    """Continue only frozen, never-started single-trial jobs; retain all earlier outputs."""
    checkout=Path(args.checkout).resolve();output=Path(args.output).resolve();session=Path(args.session).resolve()
    load=lambda p:json.loads(p.read_text(encoding='utf-8-sig'))
    spec=load(output/'conditions.json')
    if not spec.get('fresh_process_per_trial') or spec.get('event_controls'):
        raise ValueError('Recovery requires the eng10 single-trial pure profile')
    manifest=output/'delivery-sha256.json'
    if not manifest.exists():manifest=output/'results-sha256.json'
    for name,digest in load(manifest).items():
        if sha(output/name)!=digest:raise ValueError('Retained file changed: '+name)
    sources=load(output/'sources-sha256.json')
    for name,digest in sources.items():
        path=output/'source'/name
        if name=='tools/excel-session/events-job.ps1':path=output/'events-job.ps1'
        if sha(path)!=digest:raise ValueError('Frozen source changed: '+name)
    wrapper=output/'source/tools/excel-session/condition-job.ps1'
    worker=output/'source/tools/excel-session/iteration-job.ps1'
    client=output/'source/tools/excel-session/submit.ps1'
    declared={c['id'] for c in spec['controls']+spec['prepared']}
    requests={}
    for path in sorted((output/'condition-jobs').glob('*/request.json')):
        group=load(path)
        if group['condition'] not in declared or Path(group['worker']).resolve()!=worker or len(group['trials'])!=1:
            raise ValueError('Unexpected condition group: '+str(path))
        trial=group['trials'][0];request=Path(trial['request']).resolve();raw=load(request)
        key=(group['condition'],trial['trial'])
        if key in requests or request!=output/'trials'/key[0]/f'{key[1]:02d}'/'request.json':
            raise ValueError('Duplicate or unexpected trial request')
        if sha(Path(raw['input']))!=raw['input_sha256']:raise ValueError('Frozen input changed')
        requests[key]=(path,group,trial)
    if len(requests)!=1200:raise ValueError('Expected all 1200 original eng10 trial requests')
    completed={};jobs={}
    for path in sorted((session/'completed').glob('*/receipt.json')):
        rec=load(path)
        if rec.get('job',{}).get('packet_id')!=str(output) or Path(rec['job']['script']).resolve()!=wrapper:continue
        group=load(Path(rec['job']['parameters']['Request']));trial=group['trials'][0]
        key=(group['condition'],trial['trial']);result=Path(rec['job']['parameters']['Result'])
        if key in completed or key not in requests or not rec['success'] or not rec['fresh_required']:
            raise ValueError('Duplicate, failed or nonfresh retained trial; explicit review required')
        value=load(result)
        if sha(result)!=rec['result_sha256'] or rec['script_sha256']!=sha(wrapper) or rec['host_script_sha256']!=sources['tools/excel-session/host.ps1']:
            raise ValueError('Retained receipt/source hash differs')
        if not (rec['baseline']==rec['baseline_before']==rec['baseline_after']) or not rec['preserve_automation_security']:
            raise ValueError('Retained host baseline differs')
        if len(value['trials'])!=1 or not value['trials'][0]['success']:
            raise ValueError('Retained worker trial failed')
        nested=value['trials'][0]
        if sha(Path(nested['result']))!=nested['result_sha256'] or load(Path(nested['result'])).get('fatal'):
            raise ValueError('Retained worker result changed or failed')
        completed[key]=rec
    for path in sorted((session/'queue').glob('*.unrun')):
        job=load(path)
        if job.get('packet_id')!=str(output):continue
        group_path=Path(job['parameters']['Request']).resolve();group=load(group_path)
        trial=group['trials'][0];key=(group['condition'],trial['trial'])
        if key not in requests or key in jobs or key in completed or group_path!=requests[key][0]:
            raise ValueError('Unrun job does not identify an unmeasured original trial')
        if Path(job['script']).resolve()!=wrapper or not job['fresh_required'] or len(group['trials'])!=1:
            raise ValueError('Unrun job changed its source or process scope')
        if (session/'running'/(job['id']+'.json')).exists() or (session/'completed'/job['id']).exists() or Path(trial['result']).exists() or group_path.with_name('result.json').exists():
            raise ValueError('Unrun job already entered or produced output')
        jobs[key]=(path,job,sha(path))
    if set(jobs)!=set(requests)-set(completed):raise ValueError('Not every unmeasured trial has an untouched deferred job')
    # Cover the remaining conditions before adding further replicates. Each still uses a new process.
    plan=sorted(jobs,key=lambda k:(k[1],k[0]))
    if args.dry_run:
        print(json.dumps(dict(profile='eng10',qualified_retained=len(completed),unstarted_jobs=len(plan),excel_launches=0)))
        return
    campaign=output/'recovery'/uuid.uuid4().hex;campaign.mkdir(parents=True)
    shutil.copyfile(__file__,campaign/'driver.py')
    for name in ('run-status.json','results-sha256.json'):
        if (output/name).exists():shutil.copyfile(output/name,campaign/('initial-'+name))
    write(campaign/'initial.json',dict(retained_trials=len(completed),
        plan=[dict(condition=k[0],trial=k[1],id=jobs[k][1]['id'],queue_sha256=jobs[k][2],
                   group_sha256=sha(requests[k][0]),trial_request_sha256=sha(Path(requests[k][2]['request']))) for k in plan],
        limits='Only deferred jobs with no worker entry, receipt or result are eligible. Earlier failed orchestration remains retained. No completed trial is repeated.'))
    deadline=dt.datetime.fromisoformat(args.until.replace('Z','+00:00'));new=[];failures=[]
    try:
        for key in plan:
            if dt.datetime.now(dt.timezone.utc)>=deadline:break
            deferred,job,digest=jobs[key];group_path,group,trial=requests[key]
            if sha(deferred)!=digest or Path(trial['result']).exists() or group_path.with_name('result.json').exists():
                raise ValueError('Deferred job changed or entered before recovery')
            with (campaign/'host.log').open('a',encoding='utf-8') as log:
                subprocess.run(['pwsh','-NoProfile','-STA','-File',str(client),'-SessionDirectory',str(session),'-Start'],
                    cwd=checkout,stdout=log,stderr=subprocess.STDOUT,check=True,timeout=60)
            queued=deferred.with_suffix('.json');deferred.rename(queued)
            receipt=session/'completed'/job['id']/'receipt.json';limit=time.monotonic()+660
            while True:
                try:
                    if receipt.is_file():break
                except OSError as error:
                    if getattr(error,'winerror',None)!=1450:raise
                    with (campaign/'resource-errors.jsonl').open('a',encoding='utf-8') as log:
                        log.write(json.dumps(dict(job=job['id'],utc=dt.datetime.now(dt.timezone.utc).isoformat(),error=str(error)))+'\n')
                if time.monotonic()>limit:raise TimeoutError('Original single-trial job did not finish: '+job['id'])
                time.sleep(1)
            rec=load(receipt)
            if rec['job']!=job or rec['id']!=job['id']:raise ValueError('Recovery receipt binding differs')
            shutil.copytree(receipt.parent,group_path.parent/'session',dirs_exist_ok=True)
            shutil.copytree(receipt.parent,campaign/'session'/job['id'])
            new.append(dict(condition=key[0],trial=key[1],job=job['id'],success=rec['success'],
                launches=rec['launches_after']-rec['launches_before']))
            with (campaign/'progress.jsonl').open('a',encoding='utf-8') as log:log.write(json.dumps(new[-1])+'\n')
            if not rec['success']:
                failures.append(dict(job=job['id'],error=rec.get('error')));break
    finally:
        write(campaign/'status.json',dict(retained_trials=len(completed),new_receipts=len(new),
            new_excel_launches=sum(r['launches'] for r in new),failures=failures,receipts=new,
            limits='Receipt counts are orchestration only; independent final qualification must check raw trial results and baselines. Original producer status is unchanged.'))


def resume(args):
    """Resume frozen requests, observing every condition three times before filling to ten."""
    checkout=Path(args.checkout).resolve()
    output=Path(args.output).resolve()
    session=Path(args.session).resolve()
    load=lambda p: json.loads(p.read_text(encoding='utf-8-sig'))
    reviews=load(Path(args.reviewed_unstarted)) if args.reviewed_unstarted else {}
    campaign=output/'resume'/uuid.uuid4().hex
    campaign.mkdir(parents=True)
    shutil.copyfile(__file__,campaign/'driver.py')
    # Validate the delivered tree before adding anything; never overwrite a raw trial.
    for name,digest in load(output/'delivery-sha256.json').items():
        if sha(output/name)!=digest: raise ValueError('Delivered file changed: '+name)
    requests={}
    for p in sorted((output/'trials').glob('*/*/request.json')):
        r=load(p)
        ident=r.get('condition',{}).get('id') or 'P22-P06-'+r['case']
        key=(ident,r['trial'])
        if key in requests: raise ValueError('Duplicate trial request')
        if sha(Path(r['input']))!=r['input_sha256']: raise ValueError('Frozen input changed: '+str(p))
        requests[key]=p
    if len(requests)!=1040: raise ValueError('Expected all 1040 frozen trial requests')
    completed={};attempted=set();existing=[];retained_unstarted=[]
    for p in sorted((output/'all-session-receipts').glob('*/receipt.json')):
        rec=load(p)
        result=rec.get('job',{}).get('parameters',{}).get('Result')
        if not result or Path(rec.get('job',{}).get('script','')).name!='condition-job.ps1': continue
        if reviewed_unstarted(p,rec,reviews):
            retained_unstarted.append(dict(id=rec['id'],receipt_sha256=sha(p),error=rec.get('error')))
            existing.append(rec)
            continue
        raw=Path(result)
        if sha(raw)!=rec['result_sha256']: raise ValueError('Condition result changed')
        value=load(raw)
        for t in value.get('trials',[]):
            key=(value['condition'],t['trial'])
            if key in completed: raise ValueError('Trial already measured twice')
            if not t['success'] or load(Path(t['result'])).get('fatal'): raise ValueError('Existing failed trial needs explicit review')
            if sha(Path(t['result']))!=t['result_sha256']: raise ValueError('Trial result changed')
            if not rec['success'] or not (rec['baseline']==rec['baseline_before']==rec['baseline_after']==t['baseline_before']==t['baseline_after']):
                raise ValueError('Existing trial baseline differs')
            completed[key]=dict(job=rec['id'],result=t['result'],sha256=t['result_sha256'])
        existing.append(rec)
    for key,p in requests.items():
        if p.with_name('result.json').exists() and key not in completed: raise ValueError('Unqualified existing raw result: '+str(p))
    initial=set(completed)
    write(campaign/'initial.json',dict(completed_trials=len(initial),conditions=len({k[0] for k in initial}),
        retained_unstarted_failures=retained_unstarted,
        frozen_requests={str(p.relative_to(output)):sha(p) for p in requests.values()},
        process_scope='Fresh process per remaining pure-condition group (trials 1-3, then 4-10); fresh process per P06 trial. Baselines reset and checked between workbook trials. Existing trials are never repeated.'))
    deadline=dt.datetime.fromisoformat(args.until.replace('Z','+00:00'))
    receipts=[];failures=[];jobs=[]
    conditions=sorted({k[0] for k in requests})
    for phase,numbers in (('coverage',range(1,4)),('fill',range(4,11))):
        for condition_id in conditions:
            keys=[(condition_id,n) for n in numbers if (condition_id,n) not in completed]
            groups=[[k] for k in keys] if condition_id.startswith('P22-P06-') else ([keys] if keys else [])
            for group in groups:jobs.append((phase,condition_id,group))
    write(campaign/'plan.json',[dict(phase=p,condition=c,trials=[k[1] for k in g]) for p,c,g in jobs])
    if args.dry_run:
        print(json.dumps(dict(campaign=str(campaign),existing_trials=len(completed),remaining_jobs=len(jobs),remaining_trials=1040-len(completed))))
        return
    try:
        for index,(phase,condition_id,group) in enumerate(jobs):
            if dt.datetime.now(dt.timezone.utc)>=deadline: break
            directory=campaign/f'{index:03d}-{condition_id}'
            trials=[]
            for key in group:
                p=requests[key]
                if p.with_name('result.json').exists():raise ValueError('Refusing to overwrite '+str(p))
                trials.append(dict(trial=key[1],request=str(p),result=str(p.with_name('result.json'))))
            worker=checkout/'tools/excel-session'/('events-job.ps1' if condition_id.startswith('P22-P06-') else 'iteration-job.ps1')
            write(directory/'request.json',dict(condition=condition_id,worker=str(worker),trials=trials,
                process_scope='One fresh process for this remaining trial group; workbook state and recorded baseline reset between trials. The earlier and resumed groups use different processes. P06 groups contain exactly one trial.'))
            params=json.dumps(dict(Request=str(directory/'request.json'),Result=str(directory/'result.json')))
            with (directory/'console.log').open('w',encoding='utf-8') as log:
                command=['pwsh','-NoProfile','-STA','-File',str(checkout/'tools/excel-session/submit.ps1'),
                    '-SessionDirectory',str(session),'-Script',str(checkout/'tools/excel-session/condition-job.ps1'),
                    '-Parameters',params,'-Environment','iteration-manual-mtc-off','-FreshRequired',
                    '-PacketId',str(output),'-TimeoutSeconds','600']
                result=subprocess.run(command,cwd=checkout,stdout=log,stderr=subprocess.STDOUT)
            line=next((s for s in (directory/'console.log').read_text(encoding='utf-8-sig').splitlines() if s.startswith('dnacalc-excel-session/1 receipt ')),None)
            if not line: raise ValueError('Session returned no receipt; inspect '+str(directory/'console.log'))
            receipt=Path(line.split(' receipt ',1)[1]);rec=load(receipt)
            if rec['job'].get('packet_id')!=str(output) or Path(rec['job']['parameters']['Request'])!=directory/'request.json': raise ValueError('Resume receipt binding differs')
            shutil.copytree(receipt.parent,directory/'session')
            shutil.copytree(receipt.parent,output/'all-session-receipts'/rec['id'])
            receipts.append(rec)
            raw=directory/'result.json'
            if raw.exists():
                if sha(raw)!=rec['result_sha256']: raise ValueError('Resume result hash differs')
                value=load(raw)
                for t in value.get('trials',[]):
                    key=(condition_id,t['trial']);attempted.add(key)
                    if not t['success']:
                        failures.append(dict(condition=condition_id,trial=t['trial'],error=t.get('error')));continue
                    if sha(Path(t['result']))!=t['result_sha256']:raise ValueError('Resume trial hash differs')
                    if not rec['success'] or not (rec['baseline']==rec['baseline_before']==rec['baseline_after']==t['baseline_before']==t['baseline_after']):raise ValueError('Resume baseline differs')
                    if load(Path(t['result'])).get('fatal'):raise ValueError('Successful wrapper contains a fatal result')
                    completed[key]=dict(job=rec['id'],result=t['result'],sha256=t['result_sha256'])
            if result.returncode or not rec['success']:
                failures.append(dict(condition=condition_id,error=rec.get('error')))
            progress=dict(utc=dt.datetime.now(dt.timezone.utc).isoformat(),phase=phase,condition=condition_id,
                total_completed_trials=len(completed),conditions_at_least_three=sum(sum((c,n) in completed for n in range(1,11))>=3 for c in conditions),
                new_launches=sum(r['launches_after']-r['launches_before'] for r in receipts),failed_trials=len(failures))
            with (campaign/'progress.jsonl').open('a',encoding='utf-8') as log:log.write(json.dumps(progress)+'\n')
            write(campaign/'status.json',progress)
            print(json.dumps(progress),flush=True)
            if failures: break  # Retain unexpected failures for review, never retry silently.
    finally:
        write(campaign/'inventory.json',dict(requested_trials=1040,completed_trials=len(completed),new_trials=len(set(completed)-initial),
            retained_unstarted_failures=retained_unstarted,
            new_launches=sum(r['launches_after']-r['launches_before'] for r in receipts),failures=failures,
            unrun=[dict(condition=k[0],trial=k[1]) for k in requests if k not in completed],
            trials=[dict(condition=k[0],trial=k[1],**v) for k,v in sorted(completed.items())]))


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    for flag in ('checkout','packet','output','session','until'):parser.add_argument('--'+flag,required=True)
    parser.add_argument('--resume',action='store_true')
    parser.add_argument('--profile',choices=('eng9','eng10'),default='eng9')
    parser.add_argument('--dry-run',action='store_true')
    parser.add_argument('--conditions-file',action='append',help='Authored JSON conditions for the fresh-per-trial eng10 route; replaces prior controls/populations. Repeat for several files.')
    parser.add_argument('--reviewed-unstarted',help='Hash-bound reviews of failed startup jobs with no worker entry or results; failures remain retained.')
    args=parser.parse_args()
    if args.conditions_file and (args.profile!='eng10' or args.resume):parser.error('--conditions-file requires a new eng10-profile run; resume uses its frozen requests')
    (resume_eng10 if args.resume and args.profile=='eng10' else resume if args.resume else run)(args)
