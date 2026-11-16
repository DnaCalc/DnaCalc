#!/usr/bin/env python3
"""EUROCONVERT with the Euro Currency Tools add-in open (dc-ymjf.2.65), asked through Touchstone (dc-wi2wu).

  python3 tools/xtask-harness/scripts/euroconvert-ask.py --evidence <CalcFunc-Evidence> --out <file>

Asks every row of readings/excel-20527/admit2-answers-_eurotool.euroconvert-*.json again as the bare call
=EUROCONVERT(...) in the Touchstone environment variant whose Excel has EUROTOOL.XLAM open (the broker's
--addin-worker EUROTOOL.XLAM; requests name it with addins=["EUROTOOL.XLAM"]), arguments in the same typed
form (cells), inputs echoed. Writes the WitnessSet the judge reads as
readings/excel-20527/addin-answers-euroconvert-eurotool.json (`cargo xtask witness judge --func EUROCONVERT`):
ids "addin-<bank id>", the arguments Excel read back, and capture_provenance with the environment id and the
add-ins that environment reports open. Rows whose inputs read back differently, or that Excel could not
answer, are counted as rejected.
"""
import argparse, importlib.util, json, pathlib, sys, time

HERE = pathlib.Path(__file__).resolve()
sys.path.insert(0, str(HERE.parents[3] / 'tools' / 'excel-service'))
from client import Client  # noqa: E402

spec = importlib.util.spec_from_file_location('recapture', HERE.with_name('position-recapture.py'))
recapture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(recapture)
drift = recapture.drift
ADDINS = ['EUROTOOL.XLAM']
SOURCES = ['readings/excel-20527/admit2-answers-_eurotool.euroconvert-%s.json' % s for s in ('enrich', 'more', 'scalar', 'special')]


CODES = ['EUR', 'BEF', 'LUF', 'DEM', 'ESP', 'FRF', 'IEP', 'ITL', 'NLG', 'ATS', 'PTE', 'FIM', 'GRD', 'SIT', 'CYP', 'MTL',
         'SKK', 'EEK', 'LVL', 'LTL', 'USD', 'dem', ' DEM', '']
AMOUNTS = [0, 1, -1, 100, 1234.5678, 0.005, 1e9, -0.125, 2.5e-7]


def generated():
    """Calls with real currency codes (the bank's random arguments only reach the errors): every pair of a
    subset of codes, amounts, full_precision omitted/TRUE/FALSE and triangulation_precision omitted/2/3/6."""
    import struct
    hexf = lambda x: '0x%016x' % struct.unpack('>Q', struct.pack('>d', float(x)))[0]
    rows = []
    for a in AMOUNTS:
        for src in CODES:
            for dst in ['EUR', 'DEM', 'FRF', 'ITL', 'GRD', 'LTL', 'USD', 'eur']:
                for extra in ([], [True], [False], [True, hexf(3)], [False, hexf(3)], [False, hexf(2)], [True, hexf(6)]):
                    rows.append({'id': 'gen-%05d' % len(rows), 'args': [hexf(a), src, dst, *extra]})
    return rows


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--evidence', type=pathlib.Path, required=True)
    p.add_argument('--out', type=pathlib.Path, required=True)
    p.add_argument('--per-request', type=int, default=500)
    p.add_argument('--generated', action='store_true', help='ask generated calls with real currency codes instead of the bank rows')
    o = p.parse_args()
    rows = generated() if o.generated else [w for rel in SOURCES for w in json.loads((o.evidence / rel).read_text())['witnesses']]
    client = Client(transport='socket', timeout=900)
    sock = client._socket()
    started = time.monotonic()
    pending = []
    for start in range(0, len(rows), o.per_request):
        part = rows[start:start + o.per_request]
        calls = [[recapture.typed(a) for a in w['args']] for w in part]
        pending.append((part, calls, sock.submit_rows('EUROCONVERT', calls, priority='bulk', echo=True, addins=ADDINS)))
    accepted, rejected, envs, settings, stamp = [], {}, set(), None, None
    for part, calls, p_ in pending:
        reply = p_.result(timeout=client.timeout)
        envs.update(p_.env_ids)
        settings = settings or reply.get('calculation_settings')
        stamp = stamp or reply.get('timestamp')
        for w, call, observed, readback in zip(part, calls, reply['rows'], reply['inputs']):
            result = drift.answer(observed)
            if drift.without_mode(call) != drift.without_mode(readback):
                why = 'changed input'
            elif result is None:
                why = 'unread or unsupported answer: ' + str(observed.get('type'))
            else:
                accepted.append({'id': ('eurotool-' if o.generated else 'addin-') + w['id'], 'args': [recapture.bank_arg(a) for a in readback],
                                 'expected_bits': result})
                continue
            rejected[why] = rejected.get(why, 0) + 1
    seconds = time.monotonic() - started
    if len(envs) != 1:
        raise SystemExit('answers came from several environments: %s' % sorted(envs))
    env_id = envs.pop()
    record = sock.envs.get(env_id, {}).get('env') or {}
    opened = record.get('addins_open') or []
    if opened != ADDINS:
        raise SystemExit('environment %s reports add-ins open %s, not %s' % (env_id, opened, ADDINS))
    doc = {'function': 'EUROCONVERT', 'witnesses': accepted,
           'capture_provenance': {'environment': {'excel_build': record.get('build'), 'platform': 'Windows',
                                                  'excel_input_plumbing': 'cell_value2_bulk'},
                                  'calculation_settings': settings, 'env_id': env_id, 'service': 'touchstone/1',
                                  'addins_open': opened, 'timestamp': stamp,
                                  'script': 'DnaCalc tools/xtask-harness/scripts/euroconvert-ask.py'},
           'capture_summary': {'requested': len(rows), 'accepted': len(accepted), 'rejected': rejected,
                               'calls_per_second': round(len(rows) / seconds, 1)}}
    o.out.write_text(json.dumps(doc, ensure_ascii=True, separators=(',', ':')) + '\n')
    print('EUROCONVERT: accepted %d/%d rejected %s env %s in %.1f s' % (len(accepted), len(rows), rejected, env_id, seconds))


if __name__ == '__main__':
    main()
