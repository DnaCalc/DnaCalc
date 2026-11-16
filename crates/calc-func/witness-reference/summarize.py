"""Per-function summary of the reference tables: python3 summarize.py <dir>  ->  <dir>/summary.tsv"""
import csv, glob, os, sys, collections

d = sys.argv[1]
classes = ['ours-wrong', 'excel-quirk', 'both-inexact', 'large-deviation']
out = open(os.path.join(d, 'summary.tsv'), 'w')
out.write('function\tmismatching_rows\t' + '\t'.join(classes) +
          '\tother_classes\tworst_excel_ulp\tworst_ours_ulp\n')
rows = []
for p in sorted(glob.glob(os.path.join(d, '*.tsv'))):
    f = os.path.basename(p)[:-4]
    if f == 'summary': continue
    c = collections.Counter(); we = wo = 0.0; n = 0
    for r in csv.DictReader(open(p), delimiter='\t', quoting=csv.QUOTE_NONE):
        n += 1; c[r['class']] += 1
        try: we = max(we, float(r['excel_ulp'])); wo = max(wo, float(r['ours_ulp']))
        except ValueError: pass
    other = ';'.join('%s=%d' % (k, v) for k, v in sorted(c.items()) if k not in classes)
    rows.append((n, f, c, other, we, wo))
for n, f, c, other, we, wo in sorted(rows, key=lambda r: -r[0]):
    out.write('%s\t%d\t%s\t%s\t%s\t%s\n' % (f, n, '\t'.join(str(c[k]) for k in classes), other,
                                          '%.3f' % we, '%.3f' % wo))
out.close()
print(sum(r[0] for r in rows), 'rows in', len(rows), 'functions')
