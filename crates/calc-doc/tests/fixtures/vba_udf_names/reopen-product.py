"""Excel reopens actual calc-doc outputs; cached values are observed before and after recalc."""
from pathlib import Path
import argparse
import gc
import hashlib
import json
import re
import shutil
import zipfile
import win32com.client
import win32process
import measure as m


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--product', type=Path)
    parser.add_argument('--out', type=Path)
    parser.add_argument('--paths', action='store_true')
    parser.add_argument('--path-variants', action='store_true')
    args=parser.parse_args()
    original=m.OUT; m.EVENTS=[]
    output=args.out or original/'product-reopen.json'
    def record(kind,**kw):
        m.EVENTS.append(dict(kind=kind,**kw))
        output.write_text(json.dumps(m.EVENTS,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
        print(json.dumps(m.EVENTS[-1],ensure_ascii=True),flush=True)
    m.event=record
    product=args.product or original.parents[4]/'target'/'udfnames'/'product'
    x=win32com.client.DispatchEx('Excel.Application')
    pid=win32process.GetWindowThreadProcessId(x.Hwnd)[1]
    if pid==3144: raise RuntimeError('Owner Excel')
    m.event('environment',pid=pid,excel=x.Version,build=x.Build,excel_os=x.OperatingSystem)
    try:
        x.Visible=False; x.DisplayAlerts=False; x.AskToUpdateLinks=False
        b=m.open_book(x,'loaded-xml.xlsb')
        m.observe(x,b,'fresh-reopen-loaded-xml.xlsb'); m.close(b)
        for name in ['local-xlsm-saved.xlsm','local-xlsb-saved.xlsm','local-xlsb-edited.xlsm',
                     'consumer-xlsx-saved.xlsx','consumer-xlsb-saved.xlsx',
                     'collision-xlsm-reentered.xlsm','collision-xlsb-reentered.xlsm',
                     'metadata-unqualified-xlsb-saved.xlsm','metadata-custom-vba-xlsb-saved.xlsm']:
            providers=[]
            if name.startswith('consumer'):
                for p in ['provider.xlsm','probe.xlam']:
                    providers.append(m.open_book(x,p))
            b=m.retry(lambda:x.Workbooks.Open(str(product/name),UpdateLinks=0))
            m.event('package', file=name, sha256=hashlib.sha256((product/name).read_bytes()).hexdigest())
            m.observe(x,b,name+'-before-recalc',False)
            m.observe(x,b,name)
            s=b.Worksheets(1)
            if name.startswith('collision'):
                m.event('collision-reentry',file=name,B1=s.Range('B1').Formula2,
                        B1_text=s.Range('B1').Text,D1=s.Range('D1').Formula2,D1_text=s.Range('D1').Text,
                        B2=s.Range('B2').Formula2,B2_text=s.Range('B2').Text,
                        D2=s.Range('D2').Formula2,D2_text=s.Range('D2').Text)
            m.close(b)
            for p in providers: m.close(p)
        if args.paths:
            # Same-folder control and a relocated output. Never overwrite the old witnesses.
            local=original/'cdoc-fixed-consumer.xlsx'
            try:
                shutil.copyfile(product/'consumer-xlsb-saved.xlsx',local)
                for directory in [original, product]:
                    providers=[m.open_book(x,p) for p in ['provider.xlsm','probe.xlam']]
                    path=local if directory==original else product/'consumer-xlsb-saved.xlsx'
                    b=m.retry(lambda:x.Workbooks.Open(str(path),UpdateLinks=0))
                    m.observe(x,b,'fixed-path-'+str(directory)); m.close(b)
                    for p in providers: m.close(p)
                if args.path_variants:
                    # Diagnose the role of the original workbook location independently
                    # of its external-link alternate URLs, before changing the converter.
                    with zipfile.ZipFile(original/'consumer.xlsx') as z:
                        source=z.read('xl/workbook.xml')
                    location=re.search(rb'<mc:AlternateContent.*?</mc:AlternateContent>',source).group()
                    for variant in ['location-and-alternate','location-only','absolute-primary']:
                        path=product/(variant+'.xlsx')
                        with zipfile.ZipFile(product/'consumer-xlsb-saved.xlsx') as src, zipfile.ZipFile(path,'w',zipfile.ZIP_DEFLATED) as dst:
                            for item in src.infolist():
                                data=src.read(item.filename)
                                if item.filename=='xl/workbook.xml' and variant!='absolute-primary':
                                    data=data.replace(b'<workbook ',b'<workbook xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:x15="http://schemas.microsoft.com/office/spreadsheetml/2010/11/main" mc:Ignorable="x15" ')
                                    data=data.replace(b'<bookViews>',location+b'<bookViews>')
                                if item.filename.startswith('xl/externalLinks/') and item.filename.endswith('.xml'):
                                    if variant=='location-only':
                                        data=re.sub(rb'<xxl21:alternateUrls.*?</xxl21:alternateUrls>',b'',data)
                                    if variant=='absolute-primary':
                                        data=data.replace(b'r:id="rId1"',b'r:id="rId2"')
                                dst.writestr(item,data)
                        providers=[m.open_book(x,p) for p in ['provider.xlsm','probe.xlam']]
                        b=m.retry(lambda:x.Workbooks.Open(str(path),UpdateLinks=0))
                        m.observe(x,b,'path-diagnostic-'+variant); m.close(b)
                        for p in providers: m.close(p)
                # A loaded add-in with the same basename at a different location is not the provider.
                moved=product/'moved'/'probe.xlam'
                moved.parent.mkdir(exist_ok=True)
                shutil.copyfile(original/'probe.xlam',moved)
                addin=m.retry(lambda:x.Workbooks.Open(str(moved),UpdateLinks=0))
                b=m.retry(lambda:x.Workbooks.Open(str(product/'consumer-xlsb-saved.xlsx'),UpdateLinks=0))
                m.observe(x,b,'fixed-path-wrong-addin'); m.close(b); m.close(addin)
                expected=['2002','2002','#NAME?','2102','2202','3002','3002','3002']
                for row in m.EVENTS:
                    if row['kind']=='collision-reentry':
                        assert (row['B1_text'],row['D1_text'])==('5005','5005'),row
                    if row['kind']=='observe':
                        if row['scenario'].startswith('fixed-path-') and row['scenario']!='fixed-path-wrong-addin':
                            assert [r['text'] for r in row['rows']]==expected,row
                        if row['scenario']=='fixed-path-wrong-addin':
                            assert all(r['text']=='#NAME?' for r in row['rows']),row
                m.event('fix-assertions', collision_formats=2, provider_folders=2, wrong_addin_rejected=True)
            finally:
                local.unlink(missing_ok=True)
    finally:
        try:
            x.DisplayAlerts=False
            for i in range(x.Workbooks.Count,0,-1): m.close(x.Workbooks(i))
            x.Quit()
        finally:
            x=None; gc.collect()


if __name__=='__main__': main()
