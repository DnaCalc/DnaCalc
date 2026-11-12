"""Refresh the data embedded in docs/fun/function-bank.html.

Reads crates/calc-func/witness-status.tsv (full bank) and its git history (last commit of each day),
the Excel Functions Handbook build's per-function JSON (families, descriptions, verdicts, notes) and
open beads in .beads/issues.jsonl whose title names a function, then rewrites the three `const`
lines (BANK, HIST, BANK_AT) in the page. Run from the repository root:

    py -3 docs/fun/tools/refresh_function_bank.py --handbook ../handbook-preview/site/build/api/functions
"""
import argparse, collections, csv, io, json, os, re, subprocess, sys

STATUS = 'crates/calc-func/witness-status.tsv'
PAGE = 'docs/fun/function-bank.html'


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


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--handbook', required=True, help='folder with FUNC.<NAME>.json files from the handbook build')
    a = ap.parse_args()
    head = git('rev-parse', '--short', 'HEAD').strip()
    cur = status_at('HEAD')
    byday = collections.OrderedDict()
    for line in reversed([l for l in git('log', '--format=%h %ad', '--date=short', '--', STATUS).splitlines() if l.strip()]):
        h, d = line.split()
        byday[d] = h
    dates = list(byday)
    snaps = [{r['function']: (int(r['rows']), int(r['matched'])) for r in status_at(byday[d]) if r['rows'].isdigit()} for d in dates]
    names = [r['function'] for r in cur]
    hist = {'dates': dates,
            'totals': [[sum(v[0] for v in s.values()), sum(v[1] for v in s.values()), len(s)] for s in snaps],
            'fn': {n: [list(s[n]) if n in s else None for s in snaps] for n in names}}
    issues = [json.loads(l) for l in open('.beads/issues.jsonl', encoding='utf-8')]
    open_beads = [i for i in issues if i.get('status') not in ('closed', 'tombstone')]
    out = []
    for r in cur:
        n = r['function']
        w = None if r['worst_ulp'] in ('-', '') else float(r['worst_ulp'])
        p = os.path.join(a.handbook, f'FUNC.{n}.json')
        cat, state, desc, notes = None, '', '', []
        if os.path.exists(p):
            d = json.load(open(p, encoding='utf-8'))
            cat = (d.get('category') or '').replace(' functions', '').replace('and', '&') or None
            state = (d.get('implementation_state') or {}).get('state_label', '')
            desc = first_sentence((d.get('sections') or {}).get('what-it-does', ''))
            notes = [[x.get('note_id'), x.get('title')] for x in (d.get('notes') or [])][:4]
        pat = re.compile(r'(?<![A-Z0-9_.])' + re.escape(n) + r'(?![A-Z0-9_.(]?[A-Z0-9_])')
        beads = [[i['id'], i['title'][:90]] for i in open_beads if pat.search(i.get('title', ''))][:4]
        out.append([n, int(r['rows']), int(r['matched']), w, r['class'], cat, state, desc, notes, beads])
    lines = ('const BANK=' + json.dumps(out, ensure_ascii=False, separators=(',', ':')) + ';\n'
             + 'const HIST=' + json.dumps(hist, separators=(',', ':')) + ';\n'
             + 'const BANK_AT=' + json.dumps(head + ', ' + dates[-1]) + ';\n')
    page = open(PAGE, encoding='utf-8').read()
    new, k = re.subn(r'const BANK=.*?\nconst HIST=.*?\nconst BANK_AT=.*?;\n', lambda m: lines, page, count=1, flags=re.S)
    if k != 1:
        sys.exit('data block not found in ' + PAGE)
    open(PAGE, 'w', encoding='utf-8', newline='\n').write(new)
    print(f'{len(out)} functions, {len(dates)} days ({dates[0]} to {dates[-1]}), at {head}')


if __name__ == '__main__':
    main()
