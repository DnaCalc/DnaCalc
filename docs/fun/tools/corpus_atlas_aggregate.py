import json, sys, collections, random
src, out = sys.argv[1], sys.argv[2]
seen = set(); random.seed(5)
fn_files = collections.Counter(); fn_calls = collections.Counter(); shapes = collections.Counter()
feat = collections.Counter(); parts = collections.Counter(); prod = collections.Counter(); sets = collections.Counter()
pts = []; n = 0; with_formulas = 0; formula_cells = 0; total_cells = 0; nf_hist = collections.Counter(); fns_per_file = collections.Counter()
for line in open(src, encoding='utf-8'):
    try: r = json.loads(line)
    except Exception: continue
    s = r.get('set', '?')
    if s in ('repo', 'stage'): continue
    h = r.get('sha256')
    if not h or h in seen: continue
    seen.add(h)
    if not (r.get('package') or {}).get('opened'): continue
    n += 1; sets[s] += 1
    c = r.get('cells') or {}
    fc = c.get('formula_cells') or 0; cc = c.get('cells') or 0
    formula_cells += fc; total_cells += cc
    if fc: with_formulas += 1
    nf_hist[min(7, len(str(fc))) if fc else 0] += 1
    fs = r.get('functions') or {}
    fns_per_file[min(40, len(fs))] += 1
    for k, v in fs.items(): fn_files[k] += 1; fn_calls[k] += v
    for k, v in (r.get('function_shapes') or {}).items(): shapes[k] += v
    for k, v in (r.get('formula_features') or {}).items():
        if v: feat[k] += 1
    for k, v in ((r.get('package') or {}).get('part_categories') or {}).items():
        if v: parts[k] += 1
    p = r.get('producer') or {}
    prod[(p.get('application') or 'unknown') + ' ' + (p.get('app_version') or '')[:4]] += 1
    if random.random() < .12 and cc:
        pts.append([r.get('bytes') or 0, cc, fc, len(fs), s, (r.get('sheets') or {}).get('count') or 0])
json.dump({'files': n, 'sets': sets, 'with_formulas': with_formulas, 'formula_cells': formula_cells, 'cells': total_cells,
           'fn_files': fn_files, 'fn_calls': fn_calls, 'shapes': shapes.most_common(60), 'features': feat, 'parts': parts,
           'producers': prod.most_common(14), 'points': pts, 'fns_per_file': fns_per_file, 'nf_hist': nf_hist}, open(out, 'w'))
print(n, 'files', len(fn_files), 'functions', len(pts), 'points')
