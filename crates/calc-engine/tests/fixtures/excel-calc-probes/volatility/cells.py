import sys,json
import os
sys.path.insert(0,os.path.dirname(os.path.abspath(__file__)))
import readout,re
# cells.py RAW_ROOT PROBE TRIAL SAVE_TAG [COLUMNS]: tok/ca of each formula cell of one saved .xlsb
root,probe,trial,tag=sys.argv[1],sys.argv[2],sys.argv[3],sys.argv[4]
cols=sys.argv[5] if len(sys.argv)>5 else 'AF'
r=readout.read(f'{root}/{probe}/{probe}_t{trial}/{tag}.xlsb')['sheet1']
f=json.load(open(f'{root}/{probe}/volatility-{probe}.json',encoding='utf-8-sig'))
forms={}
for row in f['rows']:
    if row['trial']==int(trial):
        for x in row['formulas']: a,_,t=x.partition(' '); forms[a]=t
def key(a): return (re.sub(r'\d','',a), int(re.sub(r'\D','',a)))
for a in sorted(r,key=key):
    if a[0] in cols:
        c=r[a]; tok='Y' if (c.get('token') or c.get('arr',{}).get('token')) else 'n'
        print(a,forms.get(a,''),'tok',tok,'ca',c.get('ca'))
