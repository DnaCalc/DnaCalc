"""Refresh the data used by docs/fun/function-bank.html.

Reads crates/calc-func/witness-status.tsv (full bank) and its git history (last commit of each day),
the Excel Functions Handbook build's per-function JSON (families, descriptions, verdicts, notes) and
open beads in .beads/issues.jsonl whose title names a function, then rewrites the three `const`
lines (BANK, HIST, BANK_AT) in the data asset. Run from the repository root:

    py -3 docs/fun/tools/refresh_function_bank.py --handbook ../handbook-preview/site/build/api/functions
"""
import argparse, collections, csv, datetime, io, json, os, re, subprocess, sys

STATUS = 'crates/calc-func/witness-status.tsv'
FUZZ_STATUS = 'crates/calc-func/fuzz-status.tsv'
DATA = 'docs/fun/function-bank-data.js'
LANDSCAPES = 'docs/fun/function-bank-landscapes.js'
CALL_CLASSES = ('exact', 'last_bit', 'numeric', 'gross', 'structural', 'refused', 'excluded')


def git(*args):
    return subprocess.run(['git', *args], capture_output=True, text=True, encoding='utf-8', check=True).stdout


def status_at(rev):
    txt = git('show', f'{rev}:{STATUS}')
    return [r for r in csv.DictReader(io.StringIO(txt), delimiter='\t') if r.get('bank') == 'full']


def first_sentence(md):
    t = re.sub(r'```.*?```', '', md, flags=re.S)
    t = re.sub(r'^\s*#+.*$', '', t, flags=re.M)
    t = re.sub(r'\*\*|`|\*', '', t)
    t = re.sub(r'\[([^\]]+)\]\([^)]+\)', r'\1', t)
    paras = [p.strip().replace('\n', ' ') for p in t.split('\n\n') if p.strip() and not p.strip().startswith(('|', '-', '    '))]
    s = re.sub(r'\s+', ' ', paras[0]) if paras else ''
    m = re.match(r'(.{40,260}?[.:])(\s|$)', s)
    return (m.group(1) if m else s[:240]).strip()


def load_fuzz_status(path):
    if not path or not os.path.exists(path):
        return {}
    fuzz = {}
    with open(path, encoding='utf-8') as fh:
        for r in csv.DictReader(fh, delimiter='\t'):
            fn = (r.get('function') or '').strip().upper()
            if not fn:
                continue
            fuzz[fn] = [
                int(r['cases_swept']),
                int(r['matched']),
                int(r['failed']),
                int(r['regions_covered']),
                int(r['regions_total']),
                int(r['frontiers_mapped']),
                int(r['open_clusters']),
                r['last_sweep_date'].strip(),
                int(r['sweeps']),
                r['last_sweep_commit'].strip(),
                int(r.get('harness_faults') or 0),
            ]
    return fuzz


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--handbook', help='folder with FUNC.<NAME>.json files from the handbook build; omit to retain existing handbook metadata')
    ap.add_argument('--working-status', action='store_true', help='use the current witness-status.tsv for the latest snapshot')
    ap.add_argument('--fuzz-status', default=FUZZ_STATUS, help='path to crates/calc-func/fuzz-status.tsv')
    ap.add_argument('--out', default=DATA, help='destination path for function-bank-data.js')
    ap.add_argument('--land', help='landscape.json from function_bank_landscape.py (optional; keeps the embedded one otherwise)')
    ap.add_argument('--land-note', default='', help='where the landscape rows came from, shown in the footer')
    a = ap.parse_args()
    if a.handbook and not os.path.isdir(a.handbook):
        ap.error('--handbook must name an existing directory')
    if a.land and not a.handbook:
        ap.error('--land needs --handbook for function signatures')
    page = open(DATA, encoding='utf-8').read()
    previous_rows = json.loads(page.splitlines()[0].removeprefix('const BANK=').removesuffix(';'))
    previous = {r[0]: r for r in previous_rows}
    head = git('log', '-1', '--format=%h', '--', STATUS).strip()
    cur = ([r for r in csv.DictReader(open(STATUS, encoding='utf-8'), delimiter='\t') if r.get('bank') == 'full']
           if a.working_status else status_at('HEAD'))
    byday = collections.OrderedDict()
    for line in reversed([l for l in git('log', '--format=%h %ad', '--date=short', '--', STATUS).splitlines() if l.strip()]):
        h, d = line.split()
        byday[d] = h
    if a.working_status:
        byday[datetime.date.today().isoformat()] = None
    dates = list(byday)
    snaps = [{r['function']: (int(r['rows']), int(r['matched'])) for r in (cur if byday[d] is None else status_at(byday[d])) if r['rows'].isdigit()} for d in dates]
    names = [r['function'] for r in cur]
    hist = {'dates': dates,
            'totals': [[sum(v[0] for v in s.values()), sum(v[1] for v in s.values()), len(s)] for s in snaps],
            'fn': {n: [list(s[n]) if n in s else None for s in snaps] for n in names}}
    issues = [json.loads(l) for l in open('.beads/issues.jsonl', encoding='utf-8')]
    open_beads = [i for i in issues if i.get('status') not in ('closed', 'tombstone')]
    fuzz = load_fuzz_status(a.fuzz_status)
    out = []
    for r in cur:
        n = r['function']
        w = None if r['worst_ulp'] in ('-', '') else float(r['worst_ulp'])
        p = os.path.join(a.handbook, f'FUNC.{n}.json') if a.handbook else None
        cat, state, desc, notes = previous[n][5:9] if n in previous else (None, '', '', [])
        if p and os.path.exists(p):
            d = json.load(open(p, encoding='utf-8'))
            cat = (d.get('category') or '').replace(' functions', '').replace('and', '&') or None
            state = (d.get('implementation_state') or {}).get('state_label', '')
            desc = first_sentence((d.get('sections') or {}).get('what-it-does', ''))
            notes = [[x.get('note_id'), x.get('title')] for x in (d.get('notes') or [])][:4]
        pat = re.compile(r'(?<![A-Z0-9_.])' + re.escape(n) + r'(?![A-Z0-9_.(]?[A-Z0-9_])')
        beads = [[i['id'], i['title'][:90]] for i in open_beads if pat.search(i.get('title', ''))][:4]
        values = [r.get(c) or '' for c in CALL_CLASSES]
        counts = [int(v) for v in values] if all(v.isdigit() for v in values) else None
        out.append([n, int(r['rows']), int(r['matched']), w, r['class'], cat, state, desc, notes, beads, counts])
    lines = ('const BANK=' + json.dumps(out, ensure_ascii=False, separators=(',', ':')) + ';\n'
             + 'const HIST=' + json.dumps(hist, separators=(',', ':')) + ';\n'
             + 'const BANK_AT=' + json.dumps(head + ', ' + dates[-1]) + ';\n'
             + 'const FUZZ=' + json.dumps(fuzz, ensure_ascii=False, separators=(',', ':'), sort_keys=True) + ';\n')
    new, k = re.subn(r'const BANK=.*?\nconst HIST=.*?\nconst BANK_AT=.*?;\n(?:const FUZZ=.*?;\n)?', lambda m: lines, page, count=1, flags=re.S)
    if k != 1:
        sys.exit('data block not found in ' + DATA)
    if a.land:
        sig = {}
        for fn in os.listdir(a.handbook):
            if not fn.endswith('.json'):
                continue
            d = json.load(open(os.path.join(a.handbook, fn), encoding='utf-8'))
            n = d.get('name') or ''
            t = (d.get('sections') or {}).get('what-it-does', '') + ' ' + (d.get('sections') or {}).get('try-it', '')
            m = re.search(r'`?' + re.escape(n) + r'\(([^)`]{0,160})\)', t)
            if m:
                sig[n] = [x.strip().strip('[]').strip() for x in m.group(1).split(',')]
        land = json.load(open(a.land))
        block = ('const LAND=' + json.dumps(land, separators=(',', ':')) + ';\n'
                 + 'const SIG=' + json.dumps(sig, ensure_ascii=False, separators=(',', ':')) + ';\n'
                 + 'const LAND_AT=' + json.dumps(a.land_note or ('full bank rows at ' + head)) + ';\n')
        landscapes = open(LANDSCAPES, encoding='utf-8').read()
        landscapes, k = re.subn(r'const LAND=.*?\nconst SIG=.*?\nconst LAND_AT=.*?;\n', lambda m: block, landscapes, count=1, flags=re.S)
        if k != 1:
            sys.exit('landscape block not found in ' + LANDSCAPES)
        open(LANDSCAPES, 'w', encoding='utf-8', newline='\n').write(landscapes)
        print(f'landscapes for {len(land)} functions, {len(sig)} signatures')
    open(a.out, 'w', encoding='utf-8', newline='\n').write(new)
    print(f'{sum(r[2] for r in out):,} of {sum(r[1] for r in out):,} Excel answers matched')
    print(f'{len(out)} functions, {len(dates)} days ({dates[0]} to {dates[-1]}), {len(fuzz)} fuzzed, at {head}')


if __name__ == '__main__':
    main()
