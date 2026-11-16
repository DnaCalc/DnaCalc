#!/usr/bin/env python3
"""Excel's answers to cell-context case sets (dc-483v), asked as Touchstone sheet cases (dc-wi2wu).

  python3 tools/xtask-harness/scripts/cell-context-ask.py --out <dir> [--family F] [--per-request 200]

Each case of tools/xtask-harness/cell-context/<family>.json becomes one sheet case (function "_CASE",
token L; tools/excel-service/touchstone/PROTOCOL.md): its layout's sheets, cells and names, and one probe,
the case's formula at its cell. The worker builds each distinct layout once per batch and asks the cases
on it one after another, so the cases of a layout are sent together. The answered set is written as
<out>/<family>.json in the form `cargo xtask cells judge` reads (readings/cell-context/ of the evidence
repository): each case with "answer" {"value", "spill"} in the witness form, as
tools/excel-session/cell-context-job.ps1 wrote it; an entry Excel refused is "entry:refused" (its message
in "entry_error" beside the answer) and a layout whose constants did not read back is
"unread:input-changed". capture_provenance records the environment id, Excel's NOW() before and after
(same environment), and the user culture and separators only when an attestation file
(migration/evidence/*touchstone*-en-za.json) names this environment.
"""
import argparse, concurrent.futures, glob, json, pathlib, struct, sys, time

HERE = pathlib.Path(__file__).resolve()
REPO = HERE.parents[3]
sys.path.insert(0, str(REPO / 'tools' / 'excel-service'))
from client import Client, sheet_case, formula  # noqa: E402

CODES = {'Null': '#NULL!', 'Div0': '#DIV/0!', 'Value': '#VALUE!', 'Ref': '#REF!', 'Name': '#NAME?', 'Num': '#NUM!', 'NA': '#N/A'}
NAMES = {'#NULL!': 'Null', '#DIV/0!': 'Div0', '#VALUE!': 'Value', '#REF!': 'Ref', '#NAME?': 'Name', '#NUM!': 'Num',
         '#N/A': 'NA', '#GETTING_DATA': 'GettingData', '#SPILL!': 'Spill', '#CONNECT!': 'Connect', '#BLOCKED!': 'Blocked',
         '#UNKNOWN!': 'Unknown', '#FIELD!': 'Field', '#CALC!': 'Calc', '#BUSY!': 'Busy'}


def typed_cell(text):
    if text.startswith('='):
        return formula(text)
    if text.startswith('0x') and len(text) == 18:
        return {'type': 'number', 'bits': text[2:].lower()}
    if text.startswith('text:'):
        return {'type': 'text', 'value': text[5:]}
    if text in ('logical:true', 'logical:True'):
        return {'type': 'boolean', 'value': True}
    if text in ('logical:false', 'logical:False'):
        return {'type': 'boolean', 'value': False}
    if text.startswith('error:') and text[6:] in CODES:
        return {'type': 'error', 'code': CODES[text[6:]]}
    raise ValueError('not a witness-form cell: %r' % text)


def form(value):
    kind = value['type']
    if kind == 'number':
        return '0x' + value['bits'].lower()
    if kind == 'text':
        return 'text:' + value['value']
    if kind == 'boolean':
        return 'logical:' + ('True' if value['value'] else 'False')
    if kind == 'error':
        return 'error:' + NAMES.get(value['code'], 'code' + value['code'])
    if kind == 'blank':
        return 'blank'
    return 'unread:' + kind


def to_case(set_, case):
    layout = set_['layouts'][case['layout']] if 'layout' in case else case
    sheets = [{'name': s['name'], 'cells': {a: typed_cell(v) for a, v in (s.get('cells') or {}).items()},
               'hidden_rows': s.get('hidden_rows') or [], 'hidden_columns': s.get('hidden_columns') or []}
              for s in layout['sheets']]
    return sheet_case(sheets, [{'at': case['at'], 'formula': case['formula']}], layout.get('names') or {})


def answer_of(row):
    if row['type'] == 'unread':
        return {'value': 'unread:input-changed', 'spill': None}, None
    if row['type'] != 'sheet_case':
        return {'value': 'unread:' + row['type'], 'spill': None}, None
    probe = row['probes'][0]
    if probe.get('type') == 'refused':
        return {'value': 'entry:refused', 'spill': None}, probe['detail']
    spill = probe['spill']
    rows = None
    if spill is not None and spill['rows'] * spill['cols'] > 1:
        rows = [[form(v) for v in r] for r in spill['values']]
    return {'value': form(probe['value']), 'spill': rows}, None


def attested(env_id):
    for path in sorted(glob.glob(str(REPO / 'migration' / 'evidence' / '*touchstone*en-za.json'))):
        doc = json.load(open(path))
        if doc.get('env_id') == env_id and doc.get('service') == 'touchstone/1':
            return path[len(str(REPO)) + 1:], doc['profile']
    return None, None


def clock(client):
    reply = client.ask('NOW', [[]])
    row = reply['rows'][0]
    return reply.get('env_id'), row['bits'] if row.get('type') == 'number' else None


def ask_family(client, path, out, per_request):
    set_ = json.load(open(path))
    order = sorted(range(len(set_['cases'])), key=lambda i: (set_['cases'][i].get('layout', ''), i))
    chunks = [order[i:i + per_request] for i in range(0, len(order), per_request)]
    env_before, clock_before = clock(client)
    started = time.monotonic()
    sock = client._socket()
    futures = [(chunk, sock.submit_rows('_CASE', [[to_case(set_, set_['cases'][i])] for i in chunk], priority='bulk'))
               for chunk in chunks]
    answers, envs = {}, set()
    for chunk, pending in futures:
        reply = pending.result(timeout=client.timeout)
        envs.update(pending.env_ids)
        for i, row in zip(chunk, reply['rows']):
            answers[i] = answer_of(row)
    seconds = time.monotonic() - started
    env_after, clock_after = clock(client)
    if len(envs) != 1:
        raise SystemExit('%s: answered by several environments %s' % (path, sorted(envs)))
    env_id = envs.pop()
    cases = []
    for i, case in enumerate(set_['cases']):
        answer, error = answers[i]
        copy = dict(case)
        copy['answer'] = answer
        if error is not None:
            copy['entry_error'] = error
        cases.append(copy)
    settings = {}
    evidence, profile = attested(env_id)
    if profile:
        settings.update({'user_culture': profile['locale'], 'international_decimal_separator': profile['decimal_separator'],
                         'international_thousands_separator': profile['thousands_separator'], 'culture_evidence': evidence})
    if env_before == env_id == env_after and clock_before and clock_after:
        settings.update({'calculation_clock': clock_before, 'calculation_clock_after': clock_after})
    doc = {'format': set_['format'], 'family': set_['family'], 'layouts': set_.get('layouts', {}), 'cases': cases,
           'capture_provenance': {'service': 'touchstone/1', 'env_id': env_id, 'request': 'sheet cases (_CASE, token L)',
                                  'calculation_settings': settings, 'cases_per_second': round(len(cases) / seconds, 1),
                                  'script': 'DnaCalc tools/xtask-harness/scripts/cell-context-ask.py',
                                  'timestamp': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())}}
    target = out / (set_['family'] + '.json')
    target.write_text(json.dumps(doc, ensure_ascii=True, indent=1) + '\n')
    refused = sum(1 for c in cases if c['answer']['value'] == 'entry:refused')
    unread = sum(1 for c in cases if c['answer']['value'].startswith('unread:'))
    return '%s: %d cases in %.1f s (%.0f cases/s), env %s, %d entries refused, %d unread' % (
        set_['family'], len(cases), seconds, len(cases) / seconds, env_id, refused, unread)


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--out', type=pathlib.Path, required=True)
    p.add_argument('--family')
    p.add_argument('--per-request', type=int, default=200)
    o = p.parse_args()
    o.out.mkdir(parents=True, exist_ok=True)
    client = Client(transport='socket', timeout=900)
    for path in sorted(glob.glob(str(REPO / 'tools' / 'xtask-harness' / 'cell-context' / '*.json'))):
        if o.family and pathlib.Path(path).stem != o.family:
            continue
        print(ask_family(client, path, o.out, o.per_request), flush=True)


if __name__ == '__main__':
    main()
