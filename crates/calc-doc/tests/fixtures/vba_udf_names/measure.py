"""Excel COM measurements for dc-snz.7; run with Windows Python + pywin32.

Own Excel instance only; no security/registry changes and no installed add-ins.
AutomationSecurity=3 is used only for the explicitly disabled-macro reopen.
The formulas and VBA in these fixtures were written for this investigation (MIT).
"""
from pathlib import Path
import argparse
import gc
import hashlib
import json
import platform
import shutil
import time
import zipfile

import pythoncom
import pywintypes
import win32com.client
import win32process

HERE = Path(__file__).resolve().parent
OUT = HERE
EVENTS = []


def retry(fn):
    for attempt in range(60):
        try:
            return fn()
        except pywintypes.com_error as e:
            if e.hresult not in (-2147418111, -2147417846, -2146777998):
                raise
            if attempt == 59:
                raise
            pythoncom.PumpWaitingMessages()
            time.sleep(0.5)


def event(kind, **kw):
    row = dict(kind=kind, **kw)
    EVENTS.append(row)
    (OUT / 'excel-observations.json').write_text(
        json.dumps(EVENTS, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(row, ensure_ascii=True), flush=True)


def module(book, name, text):
    m = retry(lambda: book.VBProject.VBComponents.Add(1))
    retry(lambda: setattr(m, 'Name', name))
    retry(lambda: m.CodeModule.AddFromString(text))
    return m


def function(name, offset, visibility='Public'):
    return (f'{visibility} Function {name}(Optional ByVal x As Double = 1) As Double\n'
            f'{name} = x + {offset}\nEnd Function\n')


def book_new(x):
    b = retry(lambda: x.Workbooks.Add(-4167))
    retry(lambda: setattr(b.Worksheets(1), 'Name', 'Cases'))
    return b


def enter(b, formulas):
    s = b.Worksheets(1)
    for row, (label, formula) in enumerate(formulas, 1):
        retry(lambda: setattr(s.Cells(row, 1), 'Value2', label))
        try:
            retry(lambda: setattr(s.Cells(row, 2), 'Formula2', formula))
            event('entry', book=b.Name, cell=f'B{row}', label=label,
                  typed=formula, formula=s.Cells(row, 2).Formula2,
                  value=s.Cells(row, 2).Value2, text=s.Cells(row, 2).Text)
        except pywintypes.com_error as e:
            event('entry-rejected', book=b.Name, cell=f'B{row}', label=label,
                  typed=formula, error=str(e))


def observe(x, b, label, calculate=True):
    if calculate:
        retry(lambda: x.CalculateFullRebuild())
    s = b.Worksheets(1)
    rows = []
    for row in range(1, s.UsedRange.Rows.Count + 1):
        c = s.Cells(row, 2)
        rows.append(dict(cell=f'B{row}', label=s.Cells(row, 1).Value2,
                         formula=c.Formula2, value=c.Value2, text=c.Text))
    try:
        links = b.LinkSources(1)
    except pywintypes.com_error as e:
        links = str(e)
    event('observe', scenario=label, book=b.Name, calculated=calculate,
          links=links, rows=rows)


def save(b, name, fmt):
    target=OUT/name
    if Path(b.FullName)==target and b.FileFormat==fmt:
        retry(lambda: b.Save())
    else:
        # These are this script's generated files, in its explicit output directory.
        # Avoid Excel's overwrite path, which can block a hidden COM instance.
        if target.exists(): target.unlink()
        retry(lambda: b.SaveAs(str(target), fmt))
    event('saved', file=name, sha256=hashlib.sha256((OUT/name).read_bytes()).hexdigest())


def open_book(x, name, update=0):
    return retry(lambda: x.Workbooks.Open(str(OUT/name), UpdateLinks=update))


def close(b):
    retry(lambda: b.Close(False))


def local_cases(x):
    b = book_new(x)
    code = 'Option Explicit\n' + ''.join(function(n, v) for n, v in [
        ('MyFunc',100), ('SUM',200), ('ACOT',300), ('Foo_2',400),
        ('Café',500), ('Ångström',600), ('A1',700), ('XFD1',800),
        ('R1C1',900), ('CaseFunc',1000), ('SameName',1100)])
    code += function('SecretFunc',1200,'Private')
    m = module(b, 'Module1', code)
    module(b, 'PrivateModule', 'Option Private Module\n'+function('HiddenFunc',1300))
    save(b, 'local.xlsm', 52)
    b.Names.Add(Name='SameName', RefersTo='=42')
    b.Names.Add(Name='NamedLambda', RefersTo='=LAMBDA(x,x+1400)')
    formulas = [
        ('public','=MyFunc(2)'), ('module','=Module1.MyFunc(2)'),
        ('workbook',"='local.xlsm'!MyFunc(2)"),
        ('workbook-module',"='local.xlsm'!Module1.MyFunc(2)"),
        ('missing','=NotThere(2)'), ('explicit-xludf-missing','=_xludf.NotThere(2)'),
        ('explicit-xlfn-missing','=_xlfn.NotThere(2)'),
        ('explicit-xll-missing','=_xll.NotThere(2)'),
        ('builtin','=SUM(2)'), ('forced-udf-builtin','=_xludf.SUM(2)'),
        ('module-builtin','=Module1.SUM(2)'),
        ('workbook-builtin',"='local.xlsm'!SUM(2)"),
        ('new-builtin','=ACOT(2)'), ('forced-udf-new','=_xludf.ACOT(2)'),
        ('module-new','=Module1.ACOT(2)'),
        ('workbook-new',"='local.xlsm'!ACOT(2)"),
        ('underscore-digit','=foo_2(2)'), ('leading-underscore','=_Leading(2)'),
        ('nonascii-latin','=café(2)'), ('nonascii-scandinavian','=ångström(2)'),
        ('cell-A1','=A1(2)'), ('module-A1','=Module1.A1(2)'),
        ('cell-XFD1','=XFD1(2)'), ('module-XFD1','=Module1.XFD1(2)'),
        ('R1C1','=R1C1(2)'), ('module-R1C1','=Module1.R1C1(2)'),
        ('dotted-missing','=Foo.Bar(2)'), ('digit-first','=2Foo(2)'),
        ('case-typed','=cAsEfUnC(2)'), ('private','=SecretFunc(2)'),
        ('module-private','=Module1.SecretFunc(2)'), ('option-private','=HiddenFunc(2)'),
        ('name-value','=SameName'), ('name-and-udf','=SameName(2)'),
        ('module-name-and-udf','=Module1.SameName(2)'),
        ('lambda','=NamedLambda(2)')]
    enter(b, formulas)
    observe(x,b,'local-before-metadata')
    retry(lambda: x.MacroOptions(Macro="'local.xlsm'!MyFunc", Description='Measured MyFunc description',
                                Category='DNA UDF probes', ArgumentDescriptions=['Input x description']))
    for component in (m,b.VBProject.VBComponents('PrivateModule')):
        path=OUT/(component.Name+'.bas')
        if path.exists(): path.unlink()
        component.Export(str(path))
    save(b,'local.xlsm',52)
    save(b,'local.xlsb',50)
    # SaveAs changes the workbook qualification to the new name.
    save(b,'stripped.xlsx',51)
    observe(x,b,'xlsx-same-session-after-VBA-removal')
    close(b)
    for name in ('local.xlsm','local.xlsb','stripped.xlsx'):
        b=open_book(x,name)
        observe(x,b,'fresh-reopen-'+name)
        close(b)
    # Per-open policy; do not change Trust Center, macro security or registry settings.
    old=x.AutomationSecurity
    try:
        x.AutomationSecurity=3
        b=open_book(x,'local.xlsm')
        observe(x,b,'macros-disabled-before-calculation',False)
        observe(x,b,'macros-disabled-recalculated')
        save(b,'disabled-resaved.xlsm',52)
        close(b)
    finally:
        x.AutomationSecurity=old
    b=open_book(x,'local.xlsm')
    cm=b.VBProject.VBComponents('Module1').CodeModule
    text=cm.Lines(1,cm.CountOfLines).replace('CaseFunc','RenamedFunc')
    cm.DeleteLines(1,cm.CountOfLines); cm.AddFromString(text)
    observe(x,b,'procedure-renamed')
    enter_sheet=b.Worksheets(1)
    enter_sheet.Range('B37').Formula2='=renamedfunc(2)'
    enter_sheet.Range('A37').Value2='renamed-new-call'
    save(b,'renamed.xlsm',52)
    close(b)


def external_cases(x):
    provider=book_new(x)
    module(provider,'Module1',function('OtherFunc',2000)+function('HiddenOther',2100,'Private'))
    module(provider,'PrivateModule','Option Private Module\n'+function('PrivateOther',2200))
    save(provider,'provider.xlsm',52)
    addin=book_new(x)
    module(addin,'Module1',function('AddFunc',3000))
    save(addin,'probe.xlam',55)
    close(addin)
    # Opening .xlam loads the add-in in this isolated Excel instance, without installing it.
    addin=open_book(x,'probe.xlam')
    event('addin-open',name=addin.Name,is_addin=addin.IsAddin)
    consumer=book_new(x)
    enter(consumer,[('other-book',"='provider.xlsm'!OtherFunc(2)"),
                    ('other-module',"='provider.xlsm'!Module1.OtherFunc(2)"),
                    ('other-unqualified','=OtherFunc(2)'),
                    ('other-private',"='provider.xlsm'!HiddenOther(2)"),
                    ('other-option-private',"='provider.xlsm'!PrivateOther(2)"),
                    ('addin-unqualified','=AddFunc(2)'),
                    ('addin-qualified',"='probe.xlam'!AddFunc(2)"),
                    ('addin-module',"='probe.xlam'!Module1.AddFunc(2)")])
    observe(x,consumer,'providers-open')
    save(consumer,'consumer.xlsx',51)
    save(consumer,'consumer.xlsb',50)
    close(consumer); close(provider); close(addin)
    for label,provider_on,addin_on,update in [
        ('all-absent-no-update',False,False,0),
        ('provider-only-no-update',True,False,0),
        ('addin-only-no-update',False,True,0),
        ('both-open-no-update',True,True,0),
        ('update-existing-paths',False,False,3)]:
        loaded=[]
        if provider_on: loaded.append(open_book(x,'provider.xlsm'))
        if addin_on: loaded.append(open_book(x,'probe.xlam'))
        b=open_book(x,'consumer.xlsx',update)
        observe(x,b,label+'-before-recalc',False)
        observe(x,b,label)
        save(b,label+'.xlsx',51)
        close(b)
        for z in loaded: close(z)
        # Updating links may auto-open the add-in; close only this instance's workbooks.
        for i in range(x.Workbooks.Count,0,-1): close(x.Workbooks(i))


def main():
    global OUT
    parser=argparse.ArgumentParser()
    parser.add_argument('--out',type=Path,default=HERE)
    parser.add_argument('--section',choices=['local','external','all'],default='all')
    args=parser.parse_args(); OUT=args.out.resolve(); OUT.mkdir(parents=True,exist_ok=True)
    x=win32com.client.DispatchEx('Excel.Application')
    pid=win32process.GetWindowThreadProcessId(x.Hwnd)[1]
    if pid==3144: raise RuntimeError('Refuse to use owner Excel')
    event('environment',utc=time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),pid=pid,
          excel=x.Version,build=x.Build,excel_os=x.OperatingSystem,platform=platform.platform(),
          automation_security=x.AutomationSecurity)
    try:
        x.Visible=False; x.DisplayAlerts=False; x.AskToUpdateLinks=False
        if args.section in ('local','all'): local_cases(x)
        if args.section in ('external','all'): external_cases(x)
    finally:
        try:
            x.DisplayAlerts=False
            for i in range(x.Workbooks.Count,0,-1): close(x.Workbooks(i))
            x.Quit()
        finally:
            x=None; gc.collect()


if __name__=='__main__': main()
