"""Reduce a full-bank judge trace (jsonl.gz with function, args, expected, actual per row) to the per-function
answer landscapes embedded in docs/fun/function-bank.html: up to 260 rows per function, misses first, each
row as [arg1, arg2?, code] with code 0 = exact, -1 = differs in kind, else log10 of the ulp distance.

    python3 function_bank_landscape.py <bank-trace.jsonl.gz> landscape.json

Then: py -3 docs/fun/tools/refresh_function_bank.py --handbook <...> --land landscape.json
"""
import gzip, json, struct, random, sys, collections, math
src = sys.argv[1]; out = sys.argv[2]; CAP = 260
def num(s):
    if isinstance(s, str) and s.startswith('0x') and len(s) == 18:
        try: return struct.unpack('>d', bytes.fromhex(s[2:]))[0]
        except Exception: return None
    return None
def ordered(b):
    u = struct.unpack('>q', struct.pack('>d', b))[0]
    return u if u >= 0 else -(u & 0x7fffffffffffffff)
rows = collections.defaultdict(list); first = None
with gzip.open(src, 'rt', encoding='utf-8') as f:
    for line in f:
        try: r = json.loads(line)
        except Exception: continue
        if first is None: first = r
        fn = r.get('function'); a = r.get('actual'); e = r.get('expected')
        if fn is None: continue
        xs = [num(x) if isinstance(x, str) else (float(x) if isinstance(x, (int, float)) and not isinstance(x, bool) else None) for x in (r.get('args') or [])]
        if a == e: code = 0
        else:
            na, ne = num(a), num(e)
            if na is not None and ne is not None and math.isfinite(na) and math.isfinite(ne):
                code = abs(ordered(na) - ordered(ne)) or 1
            else: code = -1
        rows[fn].append((xs, code))
random.seed(7)
res = {}
for fn, rs in rows.items():
    numeric_pos = [i for i in range(max((len(x) for x, _ in rs), default=0)) if sum(1 for x, _ in rs if i < len(x) and x[i] is not None) > len(rs) * 0.5]
    if not numeric_pos: continue
    miss = [r for r in rs if r[1] != 0]; hit = [r for r in rs if r[1] == 0]
    take = random.sample(miss, min(len(miss), CAP // 2 if hit else CAP))
    take += random.sample(hit, min(len(hit), CAP - len(take)))
    i1 = numeric_pos[0]; i2 = numeric_pos[1] if len(numeric_pos) > 1 else None
    def r4(v): return None if v is None or not math.isfinite(v) else float('%.4g' % v)
    pts = []
    for xs, code in take:
        x1 = r4(xs[i1]) if i1 < len(xs) else None
        x2 = r4(xs[i2]) if i2 is not None and i2 < len(xs) else None
        if x1 is None: continue
        c = code if code <= 0 else min(19, round(math.log10(code), 1))  # 0 match, -1 kind, else log10 ulp (0 = 1 ulp)
        if code == 1: c = 0.01
        pts.append([x1, x2, c] if i2 is not None else [x1, c])
    res[fn] = {'n': len(rs), 'miss': len(miss), 'a': [i1, i2], 'p': pts}
json.dump(res, open(out, 'w'), separators=(',', ':'))
print(len(res), 'functions;', sum(len(v['p']) for v in res.values()), 'points; first row keys', sorted(first.keys()) if first else None)
