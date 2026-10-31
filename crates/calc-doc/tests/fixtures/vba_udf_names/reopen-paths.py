"""Compare relocated XLSB conversion with same-folder saves and Excel's own SaveAs."""
import gc
import json
import shutil
import win32com.client
import win32process
import measure as m


def main():
    original=m.OUT; existing=(original/'excel-observations.json').read_bytes(); m.EVENTS=[]
    event=m.event
    def record(kind,**kw):
        event(kind,**kw)
        (original/'path-observations.json').write_text(json.dumps(m.EVENTS,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
    m.event=record
    product=original.parents[4]/'target'/'udfnames'/'product'
    x=win32com.client.DispatchEx('Excel.Application')
    pid=win32process.GetWindowThreadProcessId(x.Hwnd)[1]
    if pid==3144: raise RuntimeError('Owner Excel')
    m.event('environment',pid=pid,excel=x.Version,build=x.Build,excel_os=x.OperatingSystem)
    try:
        x.Visible=False; x.DisplayAlerts=False; x.AskToUpdateLinks=False
        provider=m.open_book(x,'provider.xlsm'); addin=m.open_book(x,'probe.xlam')
        for name in ['consumer-xlsx-saved.xlsx','consumer-xlsb-saved.xlsx']:
            target=original/('dna-'+name)
            shutil.copyfile(product/name,target)
            b=m.open_book(x,target.name)
            m.observe(x,b,'same-folder-'+name); m.close(b)
        b=m.open_book(x,'consumer.xlsb')
        # Excel saves to another folder, keeping the calls attached to their original files.
        target=product/'excel-consumer-from-binary.xlsx'
        if target.exists(): target.unlink()
        b.SaveAs(str(target),51); m.close(b)
        b=m.retry(lambda:x.Workbooks.Open(str(target),UpdateLinks=0))
        m.observe(x,b,'excel-relocated-binary-save'); m.close(b)
        m.close(addin); m.close(provider)
    finally:
        try:
            x.DisplayAlerts=False
            for i in range(x.Workbooks.Count,0,-1): m.close(x.Workbooks(i))
            x.Quit()
        finally:
            x=None; gc.collect(); (original/'excel-observations.json').write_bytes(existing)


if __name__=='__main__': main()
