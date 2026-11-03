"""Queue P14 pure controls and seeded conditions; retain unrun obligations explicitly.

Original MIT tooling over iteration-job.ps1 and the existing Set-StoredValues helper.
The queue is fresh-required for every individual trial. No expected answers are supplied.
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
    shutil.copytree(packet, output/'packet')
    write(output/'packet-sha256.json', {p.relative_to(packet).as_posix():sha(p) for p in packet.rglob('*') if p.is_file()})
    source=checkout/'tools/excel-session/iteration-job.ps1'
    shutil.copyfile(source, output/'iteration-job.ps1')
    write(output/'sources-sha256.json', {str(p.relative_to(checkout)):sha(p) for p in (
        source, checkout/'tools/excel-session/host.ps1', checkout/'tools/excel-session/submit.ps1',
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
        controls.append(dict(id='control-'+name, input=str(path), iteration=calc.get('iterate') in ('1','true'),
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
    write(output/'conditions.json',dict(controls=controls,prepared=conditions,trials=10,
        omissions=['P14 observer/counter UDF lanes, callback order and mixed-sweep reads',
                   'P14 MTC observer lanes and live native/UI cancellation',
                   'P14 reversed tab order and dynamic-reference post-edit dependencies',
                   'P14/P25 cross-workbook cyclic domains',
                   'P22 Save/Change/Calculate event placement, cancellation and reentrancy',
                   'Existing OxVba-dependent supervisor is not invoked: that checkout is excluded by owner instruction.'],
        execution='Lane owned-PID queue; existing capture typed reads and Set-StoredValues helper; pure COM observations.'))
    inputs=output/'inputs';inputs.mkdir()
    preparation=output/'prepare/request.json'
    write(preparation,dict(action='prepare',conditions=conditions,inputs_directory=str(inputs)))
    params=json.dumps(dict(Request=str(preparation),Result=str(preparation.with_name('result.json'))))
    with (output/'prepare/console.log').open('w',encoding='utf-8') as log:
        subprocess.run(['pwsh','-NoProfile','-STA','-File',str(checkout/'tools/excel-session/submit.ps1'),
                        '-SessionDirectory',str(session),'-Script',str(source),'-Parameters',params,
                        '-Environment','iteration-setup','-TimeoutSeconds','900'],cwd=checkout,stdout=log,stderr=subprocess.STDOUT,check=True)
    jobs=[]
    for index,row in enumerate(controls+conditions):
        for trial in range(1,11):
            ident=f'eng9-{index:03d}-{trial:02d}'
            directory=output/'trials'/row['id']/f'{trial:02d}'
            request=directory/'request.json'
            input_path=row.get('input',str(inputs/(row['id']+'.xlsx')))
            write(request,dict(action='trial',condition=row,trial=trial,input=input_path))
            job=dict(id=ident,script=str(source),parameters=dict(Request=str(request),Result=str(directory/'result.json')),
                     environment='iteration-manual-mtc-off',working_directory=str(checkout),fresh_required=True,timeout_seconds=180)
            temp=session/'queue'/(ident+'.tmp');write(temp,job);temp.rename(temp.with_suffix('.json'))
            jobs.append((ident,row['id'],trial,directory))
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
                    shutil.copytree(receipt.parent,directory/'session',dirs_exist_ok=True)
                    completed.add(ident)
                    record=json.loads(receipt.read_text(encoding='utf-8-sig'))
                    with (output/'progress.jsonl').open('a',encoding='utf-8') as log:
                        log.write(json.dumps(dict(id=ident,condition=condition_id,trial=trial,success=record['success'],
                                                  error=record['error'],launches=record['launches_after']-record['launches_before']))+'\n')
            time.sleep(.5)
    finally:
        unrun=[]
        for ident,condition_id,trial,directory in jobs:
            queued=session/'queue'/(ident+'.json')
            if queued.exists():
                queued.rename(queued.with_suffix('.unrun'))
                unrun.append(dict(id=ident,condition=condition_id,trial=trial))
        # Finish an already active condition; never interrupt a measured operation at the cutoff.
        for ident,condition_id,trial,directory in jobs:
            receipt=session/'completed'/ident/'receipt.json'
            if ident not in completed and (session/'running'/(ident+'.json')).exists():
                limit=time.monotonic()+180
                while not receipt.exists() and time.monotonic()<limit:time.sleep(.2)
                if receipt.exists():shutil.copytree(receipt.parent,directory/'session',dirs_exist_ok=True);completed.add(ident)
        receipts=[json.loads((directory/'session/receipt.json').read_text(encoding='utf-8-sig')) for ident,_,_,directory in jobs if ident in completed]
        write(output/'run-status.json',dict(requested_trials=len(jobs),completed_trials=len(completed),
            successful_trials=sum(r['success'] for r in receipts),failed_trials=sum(not r['success'] for r in receipts),
            excel_launches=sum(r['launches_after']-r['launches_before'] for r in receipts),unrun=unrun,
            complete=len(completed)==len(jobs),omissions='See conditions.json; unrun and failed trials are not passes.'))
        write(output/'results-sha256.json',{p.relative_to(output).as_posix():sha(p) for p in output.rglob('*') if p.is_file()})


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    for flag in ('checkout','packet','output','session','until'):parser.add_argument('--'+flag,required=True)
    run(parser.parse_args())
