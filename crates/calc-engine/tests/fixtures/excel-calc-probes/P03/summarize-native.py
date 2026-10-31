from pathlib import Path
import collections
import csv
import json

root = Path(__file__).parent
counts = collections.defaultdict(collections.Counter)
threads = collections.defaultdict(set)
for file in sorted(root.glob('*-native.tsv')):
    condition = file.name.rsplit('-', 2)[0]
    stage = 0
    for row in csv.DictReader(file.open(encoding='utf-8'), delimiter='\t'):
        if row['kind'] == '99':
            stage = int(row['site'])
        elif row['kind'] == '2':
            counts[(condition, stage)].update([(row['status'], row['type'], row['value'])])
            threads[condition].add(row['thread'])
out = {'stages': {1:'entry', 2:'first calculation', 3:'warm F9', 4:'Full', 5:'Rebuild',
                  6:'same-value rewrite', 7:'upstream change/equal output', 8:'dirty reversed areas'},
       'coercions': [{'condition':k[0], 'stage':k[1], 'observations':
                      [{'status':o[0], 'type':o[1], 'value':o[2], 'count':n} for o,n in v.items()]}
                     for k,v in counts.items()],
       'observed_thread_ids': {k:sorted(v) for k,v in threads.items()}}
(root/'native-summary.json').write_text(json.dumps(out,indent=2)+'\n',encoding='utf-8')
