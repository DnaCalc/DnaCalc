"""Change only calculation hints in copies of a genuine 20527 stale-cache save.

These are synthetic flag experiments, not files authored by an older Excel build.
"""
from pathlib import Path
import argparse
import copy
import hashlib
import json
import re
import zipfile

root=Path(__file__).parent
parser=argparse.ArgumentParser()
parser.add_argument('--first-open',action='store_true',help='Only create the explicitly synthetic Automatic first-workbook input')
args=parser.parse_args()
source=root/'manual-false-save-log-01-tested.xlsm'
def attribute(tag,key,value):
    pattern=r'\s'+re.escape(key)+r'="[^"]*"'
    tag=re.sub(pattern,'',tag)
    if value is None:return tag
    end='/>' if tag.endswith('/>') else '>'
    return tag[:-len(end)]+' '+key+'="'+value+'"'+end
variants={
    'control':{},
    'old-calc-id':{'calcId':'124519'},
    'future-calc-id':{'calcId':'999999'},
    'full-on-load':{'fullCalcOnLoad':'1'},
    'completed-one':{'calcCompleted':'1'},
    'remove-ca':{'ca':None},
    'ca-zero':{'ca':'0'},
    'sheet-full-on-load':{'sheetFull':'1'},
}
if args.first_open:
    variants={'first-open-auto':{'calcMode':'auto'}}
manifest=[]
with zipfile.ZipFile(source) as z:
    original={name:z.read(name) for name in z.namelist()}
for label,changes in variants.items():
    parts=copy.copy(original)
    wb=parts['xl/workbook.xml'].decode('utf-8')
    for key,value in changes.items():
        if key not in ('ca','sheetFull'):
            wb=re.sub(r'<calcPr\b[^>]*/>',lambda m:attribute(m.group(0),key,value),wb)
    parts['xl/workbook.xml']=wb.encode('utf-8')
    if 'ca' in changes or 'sheetFull' in changes:
        sheet=parts['xl/worksheets/sheet1.xml'].decode('utf-8')
        if 'ca' in changes:
            sheet=re.sub(r'<f\b[^>]*>',lambda m:attribute(m.group(0),'ca',changes['ca']),sheet)
        if 'sheetFull' in changes:
            if '<sheetCalcPr' in sheet:
                sheet=re.sub(r'<sheetCalcPr\b[^>]*/>',lambda m:attribute(m.group(0),'fullCalcOnLoad',changes['sheetFull']),sheet)
            else:sheet=sheet.replace('</sheetData>','</sheetData><sheetCalcPr fullCalcOnLoad="'+changes['sheetFull']+'"/>',1)
        parts['xl/worksheets/sheet1.xml']=sheet.encode('utf-8')
    file=root/('synthetic-'+label+'.xlsm')
    with zipfile.ZipFile(file,'w',zipfile.ZIP_DEFLATED) as z:
        for name,data in parts.items():z.writestr(name,data)
    manifest.append(dict(file=file.name,sha256=hashlib.sha256(file.read_bytes()).hexdigest(),
                         source=source.name,source_sha256=hashlib.sha256(source.read_bytes()).hexdigest(),
                         changes=changes,label='synthetic flags; current-version cache'))
(root/('first-open-synthetic-manifest.json' if args.first_open else 'synthetic-manifest.json')).write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf-8')
print(f'Created {len(manifest)} explicitly synthetic variants; no formula or cache value changed.')
