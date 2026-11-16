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


# Tiling (dc-wi2wu): a relocatable case, one whose formulas hold no absolute or whole-row/column
# reference, no defined name and no function that reads a position, gives the same answer at any
# offset. Copies of its layout shifted across one sheet, each with its own probe, share one
# calculation. Hidden rows stay shared, so such layouts tile across columns only (and hidden
# columns across rows only); a layout with both is not tiled.
import re
CELL = re.compile(r"(?<![A-Za-z0-9_.])([A-Z]{1,3})([0-9]+)(?![A-Za-z0-9_(.])")
POSITIONAL = re.compile(r"\b(ROW|ROWS|COLUMN|COLUMNS|CELL|OFFSET|INDIRECT|ADDRESS|INFO|SHEET|SHEETS|FORMULATEXT|ISFORMULA|"
                        r"TODAY|NOW|RAND|RANDBETWEEN|RANDARRAY|GETPIVOTDATA|LET|LAMBDA)\s*\(", re.I)
TILE_ROOM = 66          # spill room right of and below each tile's probe (the worker reads up to 64 x 64)
MAX_PROBES = 256        # the admission's bound per sheet case
MAX_CELLS = 10000


def column_number(letters):
    n = 0
    for ch in letters:
        n = n * 26 + ord(ch) - 64
    return n


def column_letters(n):
    out = ''
    while n:
        n, r = divmod(n - 1, 26)
        out = chr(65 + r) + out
    return out


def split_strings(formula):
    """(text, inside_quotes) pieces of a formula."""
    parts, quoted, start = [], False, 0
    for i, ch in enumerate(formula):
        if ch == '"':
            parts.append((formula[start:i], quoted))
            start, quoted = i, not quoted
    parts.append((formula[start:], quoted))
    return parts


def relocatable(formula):
    if not formula.startswith('='):
        return True
    code = ''.join(text for text, quoted in split_strings(formula) if not quoted)
    if '$' in code or '#' in code.replace('#N/A', '').replace('#DIV/0!', '').replace('#VALUE!', '').replace('#REF!', '') \
            .replace('#NAME?', '').replace('#NUM!', '').replace('#NULL!', '') or POSITIONAL.search(code):
        return False
    if re.search(r"(?<![A-Za-z0-9_.])[A-Z]{1,3}:[A-Z]{1,3}(?![A-Za-z0-9_(])|(?<![A-Za-z0-9_.:])[0-9]+:[0-9]+(?![0-9])", code):
        return False
    return True


def shift(formula, dr, dc):
    if not formula.startswith('='):
        return formula
    out = []
    for text, quoted in split_strings(formula):
        if quoted:
            out.append(text)
            continue
        out.append(CELL.sub(lambda m: column_letters(column_number(m.group(1)) + dc) + str(int(m.group(2)) + dr), text))
    return ''.join(out)


def tileable(set_, case):
    # Spill-placement cases are about the room around the formula, the sheet's edges included.
    if set_.get('family') == 'spill-placement':
        return False
    layout = set_['layouts'][case['layout']] if 'layout' in case else case
    if layout.get('names'):
        return False
    hidden_rows = any(s.get('hidden_rows') for s in layout['sheets'])
    hidden_cols = any(s.get('hidden_columns') for s in layout['sheets'])
    if hidden_rows and hidden_cols:
        return False
    texts = [case['formula']] + [v for s in layout['sheets'] for v in (s.get('cells') or {}).values() if v.startswith('=')]
    return all(relocatable(t) for t in texts)


def bounds(set_, case):
    layout = set_['layouts'][case['layout']] if 'layout' in case else case
    cells = [a for s in layout['sheets'] for a in (s.get('cells') or {})] + [case['at'].split('!')[-1]]
    rows = [int(re.match(r'[A-Z]+([0-9]+)', a.replace('$', '')).group(1)) for a in cells]
    cols = [column_number(re.match(r'([A-Z]+)', a.replace('$', '')).group(1)) for a in cells]
    return max(rows), max(cols)


def tiled_case(set_, cases):
    """One sheet case holding every case of `cases` (one layout) in its own tile."""
    first = cases[0]
    layout = set_['layouts'][first['layout']] if 'layout' in first else first
    height, width = bounds(set_, first)
    across = not any(s.get('hidden_columns') for s in layout['sheets'])
    step = (width + TILE_ROOM) if across else (height + TILE_ROOM)
    sheets = [{'name': s['name'], 'cells': {}, 'hidden_rows': s.get('hidden_rows') or [],
               'hidden_columns': s.get('hidden_columns') or []} for s in layout['sheets']]
    probes = []
    for t, case in enumerate(cases):
        dr, dc = (0, t * step) if across else (t * step, 0)
        for s, target in zip(layout['sheets'], sheets):
            for a, v in (s.get('cells') or {}).items():
                m = re.match(r'([A-Z]+)([0-9]+)', a)
                target['cells'][column_letters(column_number(m.group(1)) + dc) + str(int(m.group(2)) + dr)] = typed_cell(shift(v, dr, dc))
        sheet, at = case['at'].rsplit('!', 1)
        m = re.match(r'([A-Z]+)([0-9]+)', at)
        probes.append({'at': sheet + '!' + column_letters(column_number(m.group(1)) + dc) + str(int(m.group(2)) + dr),
                       'formula': shift(case['formula'], dr, dc)})
    return sheet_case(sheets, probes, {})


def packs(set_, order):
    """Groups of case indices sent as one tiled sheet case (or alone)."""
    out, current, key = [], [], None
    for i in order:
        case = set_['cases'][i]
        if not tileable(set_, case):
            out.append([i])
            continue
        height, width = bounds(set_, case)
        layout = set_['layouts'][case['layout']] if 'layout' in case else case
        across = not any(s.get('hidden_columns') for s in layout['sheets'])
        room = (16384 // (width + TILE_ROOM)) if across else (1048576 // (height + TILE_ROOM))
        cells = max(1, sum(len(s.get('cells') or {}) for s in layout['sheets']))
        limit = max(1, min(MAX_PROBES, room, MAX_CELLS // cells))
        k = case.get('layout')
        if current and (k != key or len(current) >= limit):
            out.append(current)
            current = []
        current.append(i)
        key = k
    if current:
        out.append(current)
    return out


def answer_of(row):
    if row['type'] == 'unread':
        return {'value': 'unread:input-changed', 'spill': None}, None
    if row['type'] != 'sheet_case':
        return {'value': 'unread:' + row['type'], 'spill': None}, None
    probe = row['probes'][0]
    if probe.get('type') == 'refused':
        return {'value': 'entry:refused', 'spill': None}, probe['detail']
    spill = probe['spill']
    if spill is not None and spill.get('type') == 'unread':
        return {'value': 'unread:' + spill['reason'], 'spill': None}, None
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


def ask_family(client, path, out, per_request, tile=False):
    set_ = json.load(open(path))
    order = sorted(range(len(set_['cases'])), key=lambda i: (set_['cases'][i].get('layout', ''), i))
    groups = packs(set_, order) if tile else [[i] for i in order]
    chunks = [groups[i:i + per_request] for i in range(0, len(groups), per_request)]
    env_before, clock_before = clock(client)
    started = time.monotonic()
    sock = client._socket()

    def sent(group):
        return tiled_case(set_, [set_['cases'][i] for i in group]) if len(group) > 1 else to_case(set_, set_['cases'][group[0]])

    futures = [(chunk, sock.submit_rows('_CASE', [[sent(g)] for g in chunk], priority='bulk')) for chunk in chunks]
    answers, envs = {}, set()
    for chunk, pending in futures:
        reply = pending.result(timeout=client.timeout)
        envs.update(pending.env_ids)
        for group, row in zip(chunk, reply['rows']):
            if row.get('type') == 'sheet_case':
                for k, i in enumerate(group):
                    answers[i] = answer_of({'type': 'sheet_case', 'probes': [row['probes'][k]]})
            else:
                for i in group:
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
                                  'tiled': tile,
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
    p.add_argument('--tile', action='store_true', help='pack relocatable cases of one layout into tiles of one sheet case')
    o = p.parse_args()
    o.out.mkdir(parents=True, exist_ok=True)
    client = Client(transport='socket', timeout=900)
    for path in sorted(glob.glob(str(REPO / 'tools' / 'xtask-harness' / 'cell-context' / '*.json'))):
        if o.family and pathlib.Path(path).stem != o.family:
            continue
        print(ask_family(client, path, o.out, o.per_request, o.tile), flush=True)


if __name__ == '__main__':
    main()
