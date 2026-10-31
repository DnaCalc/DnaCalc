"""Move only this probe's add-in, measure reopen, then restore it.

Run after measure.py. Workbooks.Open loads .xlam without changing installed add-ins.
The --prompt mode leaves the COM open pending so Computer Use can record the dialogs.
"""
from pathlib import Path
import argparse
import json
import gc
import win32com.client
import win32process
import measure as m


def run(x):
    root=m.OUT; moved=root/'moved'; moved.mkdir(exist_ok=True)
    old=root/'probe.xlam'; new=moved/'probe.xlam'
    if new.exists(): raise RuntimeError('Move destination already exists')
    old.rename(new)
    try:
        for label,loaded,update in [('missing-no-update',False,0),('missing-update',False,3),
                                     ('moved-loaded-no-update',True,0),('moved-loaded-update',True,3)]:
            for ext in ('xlsx','xlsb'):
                addin=m.retry(lambda:x.Workbooks.Open(str(new),UpdateLinks=0)) if loaded else None
                b=m.open_book(x,'consumer.'+ext,update)
                m.observe(x,b,label+'-'+ext+'-before-recalc',False)
                m.observe(x,b,label+'-'+ext)
                m.save(b,label+'-'+ext+'.xlsx',51)
                m.close(b)
                if addin is not None: m.close(addin)
        addin=m.retry(lambda:x.Workbooks.Open(str(new),UpdateLinks=0))
        b=m.open_book(x,'consumer.xlsx')
        links=b.LinkSources(1)
        target=next(s for s in links if s.lower().endswith('probe.xlam'))
        b.ChangeLink(Name=target,NewName=str(new),Type=1)
        m.observe(x,b,'explicit-ChangeLink-to-moved')
        m.save(b,'changed-link.xlsx',51)
        m.close(b); m.close(addin)
    finally:
        new.rename(old)


def main():
    p=argparse.ArgumentParser(); p.add_argument('--prompt',action='store_true')
    a=p.parse_args(); m.EVENTS=[]
    # Keep the original measurements; this run has its own observation file.
    original_event=m.event
    def event(kind,**kw):
        original_event(kind,**kw)
        (m.OUT/('prompt-observations.json' if a.prompt else 'relocation-observations.json')).write_text(
            json.dumps(m.EVENTS,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
    m.event=event
    existing=(m.OUT/'excel-observations.json').read_bytes()
    x=win32com.client.DispatchEx('Excel.Application')
    pid=win32process.GetWindowThreadProcessId(x.Hwnd)[1]
    if pid==3144: raise RuntimeError('Owner Excel')
    m.event('environment',pid=pid,hwnd=x.Hwnd,excel=x.Version,build=x.Build,excel_os=x.OperatingSystem)
    try:
        x.Visible=a.prompt; x.DisplayAlerts=a.prompt; x.AskToUpdateLinks=a.prompt
        if a.prompt:
            old=m.OUT/'probe.xlam'; moved=m.OUT/'moved'/'probe.xlam'
            moved.parent.mkdir(exist_ok=True); old.rename(moved)
            try:
                m.event('prompt-open-start',file='consumer.xlsx')
                b=m.retry(lambda:x.Workbooks.Open(str(m.OUT/'consumer.xlsx')))
                m.observe(x,b,'interactive-missing-link')
                m.close(b)
            finally: moved.rename(old)
        else: run(x)
    finally:
        try:
            x.DisplayAlerts=False
            for i in range(x.Workbooks.Count,0,-1): m.close(x.Workbooks(i))
            x.Quit()
        finally:
            x=None; gc.collect()
            (m.OUT/'excel-observations.json').write_bytes(existing)


if __name__=='__main__': main()
