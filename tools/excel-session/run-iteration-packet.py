"""Queue P14 pure controls and seeded conditions; retain unrun obligations explicitly.

Original MIT tooling over iteration-job.ps1 and the existing Set-StoredValues helper.
The queue is fresh-required per condition, with ten independently seeded workbook trials.
Process memory is shared within a condition and trial order is retained. No answers are supplied.
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
    shutil.copyfile(source, output/'iteration-job.ps1')
    shutil.copyfile(event_source, output/'events-job.ps1')
    shutil.copyfile(condition_source, output/'condition-job.ps1')
    write(output/'sources-sha256.json', {str(p.relative_to(checkout)):sha(p) for p in (
        source, event_source, condition_source, checkout/'tools/excel-session/host.ps1', checkout/'tools/excel-session/submit.ps1',
        checkout/'crates/calc-engine/tests/fixtures/make-dc-y2o-oracles.ps1',
        checkout/'platform/calc-oracle-excel/worker/capture-worker.ps1')})
    conditions=[]
    def condition(ident, formulas, seeds, iterations=5, change=.001, **options):
        row=dict(id=ident, formulas=formulas, seeds=seeds, iterations=iterations,
                 change_bits=bits(change), iteration=True, operation='Rebuild', commands=2, range='A1:D1',
                 reverse_entry=False, reverse_chain=False)
        row.update(options)
        conditions.append(row)
    # The requested initial controls precede the new questions.
    controls=[]
    for name in ('dc_86f','dc_10y_iteration'):
        path=checkout/f'crates/calc-engine/tests/fixtures/{name}/oracle.xlsx'
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
    event_input=checkout/'crates/calc-engine/tests/fixtures/excel-calc-probes/P06/manual-false-save-log-01-tested.xlsm'
    event_controls=[dict(id='P22-P06-'+case,case=case,kind='events',input=str(event_input),input_sha256=sha(event_input))
                    for case in (f'manual-{before}-{method}-{events}' for before in ('false','true')
                                 for method in ('save','saveas','copy') for events in ('off','log','edit'))]
    write(output/'conditions.json',dict(controls=controls,prepared=conditions,event_controls=event_controls,trials=10,
        omissions=['P14 observer/counter UDF lanes, callback order and mixed-sweep reads',
                   'P14 MTC observer lanes and live native/UI cancellation',
                   'P14 reversed tab order and dynamic-reference post-edit dependencies',
                   'P14/P25 cross-workbook cyclic domains',
                   'New P22 Change/Calculate event placement, cancellation and reentrancy; existing P06 save controls are replayed separately',
                   'Existing OxVba-dependent supervisor is not invoked: that checkout is excluded by owner instruction.'],
        execution='Lane owned-PID queue; existing capture typed reads and Set-StoredValues helper; pure COM observations.'))
    inputs=output/'inputs';inputs.mkdir()
    preparation=output/'prepare/request.json'
    write(preparation,dict(action='prepare',conditions=conditions,inputs_directory=str(inputs)))
    params=json.dumps(dict(Request=str(preparation),Result=str(preparation.with_name('result.json'))))
    with (output/'prepare/console.log').open('w',encoding='utf-8') as log:
        subprocess.run(['pwsh','-NoProfile','-STA','-File',str(checkout/'tools/excel-session/submit.ps1'),
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
        groups=[[trial] for trial in trial_requests] if is_event else [trial_requests]
        for group in groups:
            suffix=f'-{group[0]["trial"]:02d}' if is_event else ''
            ident=f'eng9-{run_id}-{index:03d}'+suffix
            directory=output/'condition-jobs'/(row['id']+suffix)
            request=directory/'request.json'
            write(request,dict(condition=row['id'],worker=str(event_source if is_event else source),trials=group))
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
                    subprocess.run(['pwsh','-NoProfile','-STA','-File',str(checkout/'tools/excel-session/submit.ps1'),
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
                        log.write(json.dumps(dict(id=ident,condition=condition_id,trial=trial,success=record['success'],
                                                  error=record['error'],launches=record['launches_after']-record['launches_before']))+'\n')
            time.sleep(.5)
    finally:
        unrun=[dict(condition=r['id'],trials=10,reason='preparation failed',error=r['error']) for r in failed_preparations]
        for ident,condition_id,trial,directory in jobs:
            queued=session/'queue'/(ident+'.json')
            if queued.exists():
                queued.rename(queued.with_suffix('.unrun'))
                unrun.append(dict(id=ident,condition=condition_id,trials=trial))
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
        receipts=[json.loads((directory/'session/receipt.json').read_text(encoding='utf-8-sig')) for ident,_,_,directory in jobs if ident in completed]
        trials=[]
        for ident,_,_,directory in jobs:
            path=directory/'result.json'
            if ident in completed and path.is_file():trials.extend(json.loads(path.read_text(encoding='utf-8-sig')).get('trials',[]))
        expected_jobs=sum(10 if r.get('kind')=='events' else 1 for r in requested_rows)
        write(output/'run-status.json',dict(requested_conditions=len(requested_rows),requested_jobs=expected_jobs,queued_jobs=len(jobs),completed_jobs=len(completed),requested_trials=10*len(requested_rows),completed_trials=len(trials),
            successful_trials=sum(r['success'] for r in trials),failed_trials=sum(not r['success'] for r in trials),
            excel_launches=sum(r['launches_after']-r['launches_before'] for r in receipts)+preparation_record['launches_after']-preparation_record['launches_before'],
            preparation_launches=preparation_record['launches_after']-preparation_record['launches_before'],unrun=unrun,
            failed_jobs=sum(not r['success'] for r in receipts),failed_preparations=failed_preparations,
            planned_jobs_complete=len(completed)==expected_jobs and all(r['success'] for r in receipts),packet_complete=False,complete=False,
            process_scope='Pure conditions: one fresh process, ten workbook trials with baseline resets. P06 event lifecycle trials: each in a fresh process. Trial order and shared pure-condition process memory are recorded.',
            omissions='See conditions.json; unrun and failed trials are not passes.'))
        write(output/'results-sha256.json',{p.relative_to(output).as_posix():sha(p) for p in output.rglob('*') if p.is_file()})


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    for flag in ('checkout','packet','output','session','until'):parser.add_argument('--'+flag,required=True)
    run(parser.parse_args())
