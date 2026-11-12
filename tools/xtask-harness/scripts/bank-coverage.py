#!/usr/bin/env python3
"""Seeded catalogue-driven Excel service coverage. Supply the catalogue TSV export and baseline status.

Outputs uncompressed answer files and a capture log to an empty run directory. Each row
retains the input read back by Excel; changed inputs and unread answers are excluded.
"""
import argparse, concurrent.futures, csv, hashlib, importlib.util, json, math, pathlib, random, sys
ROOT = pathlib.Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'tools/excel-service'))
from client import Client, number, text, boolean, blank, error, number_array
import policy
spec = importlib.util.spec_from_file_location('drift', pathlib.Path(__file__).with_name('witness-drift.py'))
drift = importlib.util.module_from_spec(spec); spec.loader.exec_module(drift)

EDGES = [0, 1, -1, 2, -2, .5, -.5, 1e-100, -1e-100, 1e100, -1e100,
         math.nextafter(1, 0), math.nextafter(1, 2), 10, 100, 365, 25569, 45000]
WORDS = ['0', '1', '-1', '0.5', 'hello', '', 'TRUE', '1e2', '1,234', 'abc def', 'i', '12.34',
         '*', 'a?', '~*', '>1', '<=0', '<>', '=A*', 'A,B', ' abc ']
ERRORS = ['#N/A', '#VALUE!', '#DIV/0!', '#NUM!', '#REF!', '#NAME?', '#NULL!']
ARRAY_FUNCS = {'LINEST', 'LOGEST', 'FORECAST.ETS', 'FORECAST.ETS.CONFINT',
               'FORECAST.ETS.SEASONALITY', 'FORECAST.ETS.STAT'}
# These cannot be represented as stable expected values or callable arguments in
# the current witness format. Keep them visible in the capture log.
OBSERVATIONS_ONLY = {'NOW', 'TODAY', 'RAND', 'RANDBETWEEN', 'RANDARRAY'}
CALLABLES = {'BYCOL', 'BYROW', 'MAP', 'MAKEARRAY', 'REDUCE', 'SCAN', 'ISOMITTED', 'GROUPBY', 'PIVOTBY'}

def numeric(rng, name, i):
    # Mix domain-relevant values with unbounded and close-to-integer controls.
    if i < len(EDGES): return float(EDGES[i])
    name = name.lower()
    if any(x in name for x in ('date', 'settlement', 'maturity', 'issue', 'period')):
        if rng.random() < .65: return float(rng.randrange(1, 80000))
    if any(x in name for x in ('probability', 'alpha', 'rate', 'percent', 'significance')):
        if rng.random() < .7: return rng.random()
    if any(x in name for x in ('num', 'count', 'basis', 'type', 'digits', 'frequency', 'tails', 'index')):
        if rng.random() < .65: return float(rng.randrange(-10, 200))
    k = rng.randrange(6)
    if k == 0: return float(rng.randrange(-10000, 10001))
    if k == 1: return rng.uniform(-1000, 1000)
    if k == 2: return rng.random()
    if k == 3: return math.ldexp(rng.uniform(-1, 1), rng.randrange(-300, 301))
    if k == 4: return math.nextafter(float(rng.randrange(-100, 101)), rng.choice([-math.inf, math.inf]))
    return rng.uniform(-2, 2)

def generate(entry, stage):
    name, _, _, lo, hi, _, params = entry
    lo, hi = int(lo), min(int(hi), 8)
    if name in {"TRUE", "FALSE", "NA", "PI"}: return [[]]
    if hi == 0: return [[]]
    lo = max(1, lo)
    names = [p.split(':')[0] for p in params.split(',') if p]
    rng = random.Random(int.from_bytes(hashlib.sha256((name + stage + 'bank-coverage-20261112').encode()).digest()[:8], 'big'))
    rows = []
    count = 160 if stage == 'special' else 1400
    for i in range(count):
        if name == 'GETPIVOTDATA':
            rows.append([text(f'measure{i}'), number(i)])
            continue
        if name in {'COUNTIFS', 'SUMIFS', 'AVERAGEIFS', 'MAXIFS', 'MINIFS'}:
            criteria = ['>0', '<0', '<>0', '=0', '*', 'a?', '~*', '<=' + str(i % 97)][i % 8]
            criterion_range = number((i - 700) / 11)
            if name == 'COUNTIFS': row = [criterion_range, text(criteria)]
            else: row = [number((i - 700) / 7), criterion_range, text(criteria)]
            rows.append(row); continue
        if name in {"ROWS", "COLUMNS", "ARRAYTOTEXT", "INDEX", "MATCH", "XMATCH",
                    "VLOOKUP", "HLOOKUP", "LOOKUP", "XLOOKUP"}:
            values = [float((i + j) % 13) + i / 10000 for j in range(1 + i % 5)]
            if i % 3 == 0 and len(values) > 1: values[-1] = values[0]
            if i % 4 == 0: values.reverse()
            if i % 2: values.sort()
            col = number_array([[v] for v in values])
            mat = number_array([[v, v * 2 + i] for v in values])
            if name in {"ROWS", "COLUMNS", "ARRAYTOTEXT"}: row = [mat]
            elif name == "INDEX": row = [mat, number(1 + i % len(values)), number(1 + i % 2)]
            elif name in {"MATCH", "XMATCH"}: row = [number(values[0]), col, number(0)]
            elif name == "VLOOKUP": row = [number(values[0]), mat, number(2), boolean(False)]
            elif name == "HLOOKUP": row = [number(values[0]), number_array([values, [v * 2 + i for v in values]]), number(2), boolean(False)]
            elif name == "LOOKUP": row = [number(values[0]), col, col]
            else: row = [number(values[0]), col, col, text('missing')]
            rows.append(row); continue
        arity = lo + (i % (max(lo, hi)-lo+1))
        row = []
        for j in range(arity):
            label = names[j] if j < len(names) else ''
            value = numeric(rng, label, i if j == 0 else (i + j*7) % 29)
            # A broad first POISSON batch caused an Excel worker deadline. Keep its
            # second batch bounded so that the service can recover useful answers.
            if name == 'POISSON': value = rng.uniform(0, 100)
            if name in ARRAY_FUNCS and ((name in {'LINEST', 'LOGEST'} and j < 2) or (name.startswith('FORECAST.ETS') and j in (1,2))):
                vals = [float(k + 1) if j == 2 else float(k + 1 + rng.random()) for k in range(5)]
                row.append(number_array([[v] for v in vals])); continue
            if stage != 'special' and i % 8 == 1 and j == (i//8) % arity:
                row.append(text(rng.choice(WORDS) if i % 3 else str(round(value, 8))))
            elif stage != 'special' and i % 8 == 2 and j == (i//8) % arity:
                row.append(boolean(bool(i % 3)))
            elif stage == 'special' and j == (i//2) % arity:
                row.append(blank() if i % 2 == 0 else error(ERRORS[(i//2) % len(ERRORS)]))
            else: row.append(number(value))
        rows.append(row)
    return rows

def generate_spill(name, count=1200):
    rng = random.Random(int.from_bytes(hashlib.sha256((name + 'spill-bank-coverage-20261112').encode()).digest()[:8], 'big'))
    rows = []
    for i in range(count):
        def matrix():
            return number_array([[rng.randrange(-10, 11) + rng.random() for _ in range(2)] for _ in range(2)])
        if name == 'FILTER':
            row = [matrix(), number_array([[float(i % 2)], [1.]])]
        elif name == 'SORTBY':
            row = [matrix(), number_array([[float(i)], [float(-i)]]), number((-1.) ** i)]
        elif name in {'TAKE', 'DROP', 'CHOOSEROWS', 'CHOOSECOLS'}:
            extent = 1 if name == 'DROP' else 1 + i % 2
            row = [matrix(), number(float(extent) * (-1 if i % 3 == 0 else 1))]
        elif name in {'HSTACK', 'VSTACK'}:
            row = [matrix(), matrix()]
        elif name in {'TOCOL', 'TOROW'}:
            row = [matrix(), number(i % 4), boolean(i % 2 == 0)]
        elif name in {'WRAPCOLS', 'WRAPROWS'}:
            row = [number_array([[rng.random() for _ in range(1 + i % 6)]]), number(1 + i % 4), number(i)]
        elif name == 'EXPAND':
            row = [matrix(), number(2 + i % 4), number(2 + i % 3), number(i)]
        elif name == 'REGEXEXTRACT':
            row = [text(f'item{i}x{rng.randrange(1000)}'), text(r'[0-9]+'), number(i % 3)]
        elif name == 'TEXTSPLIT':
            row = [text(f'{i}a,b{rng.randrange(10000)},c'), text(',')]
        elif name == 'SEQUENCE':
            row = [number(rng.uniform(-2, 6)), number(rng.uniform(-2, 6)), number(i / 13), number(rng.uniform(-5, 5))]
        elif name == 'MUNIT':
            row = [number(rng.uniform(-10, 7))]
        elif name in {'MMULT', 'FREQUENCY'}:
            row = [matrix(), matrix()]
        elif name in {'TREND', 'GROWTH'}:
            row = [matrix(), matrix(), matrix()]
        elif name == 'SORT':
            row = [matrix(), number(rng.choice([1, 2])), number(rng.choice([-1, 1]))]
        else:
            row = [matrix()]
        rows.append(row)
    return rows

def generate_variants(entry):
    name = entry[0]
    rows = generate_spill(name, 320) if name in policy.SPILL_FUNCTIONS else generate(entry, 'scalar')[:320]
    for i, row in enumerate(rows):
        position = (i // 5) % len(row)
        kind = i % 5
        row[position] = (number((i - 160) / 7) if kind == 0 else
                         text(['2', '-1.25', 'hello'][i % 3]) if kind == 1 else
                         boolean(bool((i // 5) % 2)) if kind == 2 else
                         blank() if kind == 3 else error(ERRORS[(i // 5) % len(ERRORS)]))
    return rows

def witness_arg(arg):
    kind = arg['type']
    if kind == 'number': return '0x' + arg['bits']
    if kind == 'text': return arg['value']
    if kind == 'boolean': return arg['value']
    if kind == 'blank': return None
    if kind == 'error': return {'error': drift.ERRORS[arg['code']]}
    if kind in ('array', 'mixed_array'): return [[witness_arg(x) for x in row] for row in arg['values']]
    raise ValueError(kind)

def ask_checked(client, name, rows, *, result_index=None, shape='scalar', **_):
    # The convenience reply names one representative environment even when a
    # request used several. Future captures refuse that ambiguity; the original
    # bank2 captures predate this check and document the limitation.
    pending = client._socket().submit_rows(name, rows, result_index=result_index,
                                           priority='bulk', echo=True, spill=shape == 'array')
    reply = pending.result(timeout=client.timeout)
    if len(pending.env_ids) != 1:
        raise ValueError('request did not have one recorded worker environment')
    return reply

def main():
    p=argparse.ArgumentParser(); p.add_argument('--catalogue',type=pathlib.Path,required=True); p.add_argument('--baseline',type=pathlib.Path,required=True); p.add_argument('--out',type=pathlib.Path,required=True); p.add_argument('--stage',choices=['scalar','more','enrich','special','spill','spillmore','variants'],required=True); p.add_argument('--only'); o=p.parse_args()
    o.out.mkdir(parents=True,exist_ok=True)
    entries=list(csv.reader(o.catalogue.open(),delimiter='\t'))
    status={r['function']:r for r in csv.DictReader(o.baseline.open(),delimiter='\t') if r['bank']=='full'}
    selected=[]
    for e in entries:
        n=e[0]; s=status.get(n)
        if o.only and n != o.only: continue
        if n in policy.REMOTE_SERVICE or e[1].startswith(('Operator','Binding')): continue
        variants = {'FREQUENCY','LINEST','LOGEST','MMULT','MODE.MULT','MUNIT','SEQUENCE','SORT','TEXTSPLIT','TRANSPOSE','TREND','UNIQUE'}
        is_spill = n in policy.SPILL_FUNCTIONS
        if o.stage == 'variants' and n not in variants: continue
        if o.stage.startswith('spill') != is_spill and o.stage != 'variants': continue
        if s and int(s['rows'])>=1000 and not (s['worst_ulp'].isdigit() and int(s['worst_ulp'])>10000): continue
        selected.append(e)
    c=Client(transport='socket',timeout=900)
    def capture(e):
        name=e[0]; path=o.out / ('answers-' + name.lower() + '-' + o.stage + '.json')
        if name in OBSERVATIONS_ONLY: return name, 'needs volatile observation/property format'
        if name in CALLABLES: return name, 'needs callable argument protocol'
        if path.exists(): return name, 'already captured'
        rows=generate_variants(e) if o.stage == 'variants' else generate_spill(name,1600)[1200:] if o.stage == 'spillmore' else generate_spill(name) if o.stage == 'spill' else generate(e,o.stage)
        try:
            kw={'echo_inputs':True,'priority':'bulk'}
            spill = o.stage.startswith('spill') or (o.stage == 'variants' and name in policy.SPILL_FUNCTIONS)
            if name in {'LINEST','LOGEST'} and not spill: kw['result_index']=(1,1)
            if spill:
                kw['shape']='array'
                # Large spill batches hit the worker deadline even though scalar batches
                # are fast. Thirty calls keep each spill calculation bounded in practice.
                replies=[ask_checked(c, name, rows[i:i+30], **kw) for i in range(0,len(rows),30)]
                if len({part.get('env_id') for part in replies}) != 1:
                    raise ValueError('spill batches used different worker environments')
                reply=replies[0].copy()
                reply['rows']=[row for part in replies for row in part['rows']]
                reply['inputs']=[row for part in replies for row in part['inputs']]
            else:
                reply=ask_checked(c,name,rows,**kw)
            accepted=[]; rejected={}
            for i,(requested, observed, readback) in enumerate(zip(rows,reply['rows'],reply['inputs'])):
                selected = drift.select_spill(observed, [1,1]) if spill else observed
                result=drift.answer(selected) if selected else None
                if requested == [] and readback is None: readback = []
                if drift.without_mode(requested)!=drift.without_mode(readback): why='changed input'
                elif result is None: why='unread or unsupported answer: '+str(observed.get('type'))
                else:
                    w={'id':f'admit2-{o.stage}-{name}-{i:04}', 'args':[witness_arg(a) for a in readback], 'expected_bits':result}
                    if name in {'LINEST','LOGEST'} or spill: w['result_index']=[1,1]
                    accepted.append(w); continue
                rejected[why]=rejected.get(why,0)+1
            env={'excel_build':reply.get('excel_build'),'platform':'Windows','excel_input_plumbing':'cell_value2_bulk'}
            doc={'function':name,'witnesses':accepted,'capture_provenance':{'environment':env,'calculation_settings':reply.get('calculation_settings'),'env_id':reply.get('env_id'),'timestamp':reply.get('timestamp'),'service':'touchstone/1'},'capture_summary':{'requested':len(rows),'accepted':len(accepted),'rejected':rejected,'seed':'bank-coverage-20261112'}}
            path.write_text(json.dumps(doc,ensure_ascii=True,separators=(',',':'))+'\n')
            return name, f"accepted {len(accepted)}/{len(rows)} rejected {rejected}"
        except Exception as exc: return name, f'FAILED {type(exc).__name__}: {exc}'
    print('selected',len(selected),flush=True)
    with concurrent.futures.ThreadPoolExecutor(max_workers=2 if o.stage.startswith('spill') or o.stage == 'variants' else 4) as pool:
        for name,result in pool.map(capture,selected): print(name,result,flush=True)
if __name__=='__main__': main()
