"""Excel reopens actual calc-doc outputs; cached values are observed before and after recalc."""
from pathlib import Path
import gc
import json
import win32com.client
import win32process
import measure as m


def main():
    original=m.OUT; existing=(original/'excel-observations.json').read_bytes(); m.EVENTS=[]
    event=m.event
    def record(kind,**kw):
        event(kind,**kw)
        (original/'product-reopen.json').write_text(json.dumps(m.EVENTS,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
    m.event=record
    product=original.parents[4]/'target'/'udfnames'/'product'
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
    finally:
        try:
            x.DisplayAlerts=False
            for i in range(x.Workbooks.Count,0,-1): m.close(x.Workbooks(i))
            x.Quit()
        finally:
            x=None; gc.collect(); (original/'excel-observations.json').write_bytes(existing)


if __name__=='__main__': main()
