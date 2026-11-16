#!/usr/bin/env python3
"""Recapture bank rows whose answer depends on where the argument cells were (dc-ymjf.2.85).

  python3 tools/xtask-harness/scripts/position-recapture.py --evidence <CalcFunc-Evidence> \
      --function ROW --out <file> [--limit N]

The bulk captures (`touchstone/1-bulk`) put call i's arguments on row r of sheet S, where r
depends on how the broker cut the batch; that row is not recorded, so ROW and the reference
forms of OFFSET and CELL cannot be judged. This asks every such bank row again, with the same
arguments in the same typed form, as its own one-call serial request: the serial worker lays a
request's calls out in request order from row 1 (Program.cs EvaluateSerial / EvaluateBatch
with serialCalc), so a one-call request has its formula in S!A1 and its arguments from S!B1.
The file records that layout as `cell_layout: touchstone/1-serial-single`; the harness keeps
each row's id and counts it once, the recaptured answer superseding the bulk one. Rows whose
inputs Excel read back differently, or that Excel could not answer, are listed as rejected and
stay failures. Each answer is S!A1's value (`result_selection: anchor`): a spill wider than one
column meets the argument cells and is #SPILL!, as in the bulk layout.
"""
import argparse, concurrent.futures, gzip, importlib.util, json, pathlib, sys

HERE = pathlib.Path(__file__).resolve()
sys.path.insert(0, str(HERE.parents[3] / 'tools' / 'excel-service'))
from client import Client, number, number_array, text, boolean, blank, error  # noqa: E402

spec = importlib.util.spec_from_file_location('drift', HERE.with_name('witness-drift.py'))
drift = importlib.util.module_from_spec(spec)
spec.loader.exec_module(drift)
CODES = {v: k for k, v in drift.ERRORS.items()}

LAYOUT = 'touchstone/1-serial-single'
SOURCES = {
    'ROW': ['readings/excel-20527/admit2-answers-row-%s.json' % s for s in ('enrich', 'more', 'scalar', 'special')],
    'OFFSET': ['readings/excel-20527/admit2-answers-offset-%s.json' % s for s in ('enrich', 'more', 'scalar', 'special')],
    'CELL': ['readings/excel-20527/admit2-answers-cell-%s.json' % s for s in ('enrich', 'more', 'scalar', 'special')],
    # The spill banks' calls reached Excel's bulk layout (formula in column A, arguments to its right),
    # so a spill wider than one column met the argument cells: #SPILL! at an unrecorded row.
    # Whether a referenced row is hidden, filtered or holds a nested subtotal is a fact of the sheet
    # the bulk layout does not record; the one-call layout's sheet holds only the call.
    'SUBTOTAL': ['readings/excel-20527/admit2-answers-subtotal-%s.json' % s for s in ('enrich', 'more', 'scalar', 'special')],
    'AGGREGATE': ['readings/excel-20527/admit2-answers-aggregate-%s.json' % s for s in ('enrich', 'more', 'scalar', 'special')],
    # WEEKNUM with a return type outside 1-21 reads a state the previous WEEKNUM or ISOWEEKNUM of
    # the same calculation pass left (dc-amhpx); a call alone in its pass has no previous call.
    'WEEKNUM': ['runs/w111-%s-20260929/answers/answers-weeknum.json' % s
                for s in ('date-boundaries', 'date-conversion', 'date-integer', 'broad')],
    'TRANSPOSE': ['readings/excel-20527/bank2-answers-transpose-spill.json'],
    'SEQUENCE': ['readings/excel-20527/bank2-answers-sequence-spill.json'],
    'TREND': ['readings/excel-20527/bank2-answers-trend-spill.json'],
    'FREQUENCY': ['readings/excel-20527/bank2-answers-frequency-spill.json'],
    # These malformed ETS calls spill in the old shared-row layout. A single-call
    # capture records the placement that determines their anchor value.
    **{f: ['readings/excel-20527/bank2-answers-%s-%s.json' % (f.lower(), s)
           for s in ('more', 'scalar', 'special')]
       for f in ('FORECAST.ETS.STAT', 'FORECAST.ETS.SEASONALITY')},
}


def typed(arg):
    """A bank argument in the typed form the bulk capture sent it (cells, not literals)."""
    if isinstance(arg, bool):
        return boolean(arg)
    if arg is None:
        return blank()
    if isinstance(arg, dict) and set(arg) == {'error'}:
        return error(CODES[arg['error']])
    if isinstance(arg, list):
        rows = arg if all(isinstance(r, list) for r in arg) else [arg]
        if not all(isinstance(v, str) and v.startswith('0x') and len(v) == 18 for r in rows for v in r):
            raise ValueError('only numeric arrays travel as cells: %r' % (arg,))
        return number_array([[v[2:] for v in r] for r in rows])
    if isinstance(arg, str):
        if arg.startswith('0x') and len(arg) == 18:
            return number(arg[2:])
        return text(arg)
    raise ValueError('argument form %r is not a scalar cell' % (arg,))


def outside_return_types(w):
    """A WEEKNUM row whose return type is a number below 1 or from 21 up: a superset of the
    harness's order-dependent rows (witness.rs order_dependent_weeknum)."""
    if len(w['args']) < 2:
        return False
    t = w['args'][1]
    if not (isinstance(t, str) and t.startswith('0x') and len(t) == 18):
        return False
    import struct
    x = struct.unpack('>d', bytes.fromhex(t[2:]))[0]
    return x == x and (x < 1 or x >= 21)


def bank_arg(read):
    kind = read['type']
    if kind == 'array':
        return [[bank_arg(x) for x in row] for row in read['values']]
    if kind == 'number':
        return '0x' + read['bits'].lower()
    if kind in ('text', 'boolean'):
        return read['value']
    if kind == 'blank':
        return None
    if kind == 'error':
        return {'error': drift.ERRORS[read['code']]}
    raise ValueError(kind)


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--evidence', type=pathlib.Path, required=True)
    p.add_argument('--function', required=True, choices=sorted(SOURCES))
    p.add_argument('--out', type=pathlib.Path, required=True)
    p.add_argument('--limit', type=int)
    p.add_argument('--workers', type=int, default=16)
    o = p.parse_args()
    rows = []
    for rel in SOURCES[o.function]:
        path = o.evidence / rel
        doc = json.loads(path.read_text() if path.exists() else gzip.open(str(path) + '.gz').read())
        assert doc['function'].upper() == o.function, rel
        for w in doc['witnesses']:
            if o.function == 'WEEKNUM' and not outside_return_types(w):
                continue
            if o.function in ('FORECAST.ETS.STAT', 'FORECAST.ETS.SEASONALITY') and w['expected_bits'] != 'error:Spill':
                continue
            rows.append((rel, w))
    if o.limit:
        rows = rows[:o.limit]
    client = Client(transport='socket', timeout=1800)
    socket = client._socket()

    def ask(item):
        rel, w = item
        call = [typed(a) for a in w['args']]
        pending = socket.submit_rows(o.function, [call], priority='bulk', echo=True, order='serial')
        reply = pending.result(timeout=client.timeout)
        return rel, w, call, reply, sorted(pending.env_ids)

    accepted, rejected, envs, stamps, settings = [], {}, set(), [], None
    with concurrent.futures.ThreadPoolExecutor(o.workers) as pool:
        for n, (rel, w, call, reply, env_ids) in enumerate(pool.map(ask, rows)):
            if n % 500 == 0:
                print(f'{n}/{len(rows)}', flush=True)
            if reply.get('order') != 'serial' or len(env_ids) != 1:
                why = 'not one serial environment'
            else:
                observed, readback = reply['rows'][0], reply['inputs'][0]
                result = drift.answer(observed)
                if drift.without_mode(call) != drift.without_mode(readback):
                    why = 'changed input'
                elif result is None:
                    why = 'unread or unsupported answer: ' + str(observed.get('type'))
                else:
                    envs.update(env_ids)
                    stamps.append(reply.get('timestamp'))
                    settings = settings or reply.get('calculation_settings')
                    accepted.append({'id': w['id'], 'original_source': rel,
                                     'args': [bank_arg(a) for a in readback], 'expected_bits': result,
                                     'bulk_expected_bits': w['expected_bits']})
                    continue
            rejected.setdefault(why, []).append(w['id'])
    if len(envs) > 1:
        raise SystemExit('answers came from several environments: %s' % sorted(envs))
    doc = {
        'format': 'dnacalc-witness-recapture/1',
        'function': o.function,
        'witnesses': accepted,
        'capture_provenance': {
            'service': 'touchstone/1', 'env_id': next(iter(envs), None),
            'environment': {'excel_build': '20527', 'platform': 'Windows', 'excel_input_plumbing': 'cell_value2_bulk'},
            'calculation_settings': settings, 'order': 'serial', 'calls_per_request': 1,
            'cell_layout': LAYOUT,
            # The answer is S!A1's value as calculated, with no INDEX around the call; a bank row's
            # result_index does not apply to it.
            'result_selection': 'anchor', 'first': min(stamps, default=None), 'last': max(stamps, default=None),
            'script': 'DnaCalc tools/xtask-harness/scripts/position-recapture.py',
        },
        'capture_summary': {'requested': len(rows), 'accepted': len(accepted),
                            'rejected': {k: len(v) for k, v in rejected.items()},
                            'rejected_ids': rejected},
    }
    o.out.write_text(json.dumps(doc, ensure_ascii=True, separators=(',', ':')) + '\n')
    print(f'{o.function}: accepted {len(accepted)}/{len(rows)} rejected {doc["capture_summary"]["rejected"]} env {sorted(envs)}')


if __name__ == '__main__':
    main()
