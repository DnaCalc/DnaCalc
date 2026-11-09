import json, re, sys
r = json.load(open(sys.argv[1], encoding='utf-8-sig'))
print({k: v for k, v in r.items() if k != 'rows'})
seen = {}
for row in r['rows']:
    key = (row['case'], row['op'])
    if 'error' in row:
        seen.setdefault(key, {}).setdefault(('ERR', row['error'], row.get('step')), 0); continue
    def norm(s):
        return re.sub(r'=[-0-9.E+]+(?=@)', '=v', s) if row['case'] in ('rand', 'now') else s
    sig = tuple(norm(row[s]['seq']) + ' => ' + ','.join(f'{k}={v}' for k, v in row[s]['values'].items() if k != 'C1') for s in ('s1', 's2', 's3'))
    seen.setdefault(key, {}).setdefault(sig, 0); seen[key][sig] += 1
for k, v in seen.items():
    print(k)
    for sig, c in v.items():
        print('  x%d' % c)
        for s, x in zip(('s1', 's2', 's3'), sig): print('    ', s, '|', x)
