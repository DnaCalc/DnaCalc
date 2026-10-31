"""Additional XML/binary conversions, procedure casing and MacroOptions persistence.

Run after measure.py. All projects are authored here; no external VBA source is used.
"""
import gc
import json
import win32com.client
import win32process
import measure as m


def main():
    existing=(m.OUT/'excel-observations.json').read_bytes(); m.EVENTS=[]
    event=m.event
    def record(kind,**kw):
        event(kind,**kw)
        (m.OUT/'extra-observations.json').write_text(json.dumps(m.EVENTS,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
    m.event=record
    x=win32com.client.DispatchEx('Excel.Application')
    pid=win32process.GetWindowThreadProcessId(x.Hwnd)[1]
    if pid==3144: raise RuntimeError('Owner Excel')
    m.event('environment',pid=pid,excel=x.Version,build=x.Build,excel_os=x.OperatingSystem)
    try:
        x.Visible=False; x.DisplayAlerts=False; x.AskToUpdateLinks=False
        b=m.open_book(x,'local.xlsm'); m.observe(x,b,'xml-to-binary')
        m.save(b,'loaded-xml.xlsb',50); m.close(b)
        b=m.open_book(x,'local.xlsb'); m.observe(x,b,'binary-to-xml')
        m.save(b,'loaded-binary.xlsm',52); m.close(b)
        b=m.open_book(x,'local.xlsm')
        cm=b.VBProject.VBComponents('Module1').CodeModule
        text=cm.Lines(1,cm.CountOfLines).replace('CaseFunc','CASEFUNC')
        cm.DeleteLines(1,cm.CountOfLines); cm.AddFromString(text)
        m.observe(x,b,'declaration-case-only-change')
        m.save(b,'case-only.xlsm',52); m.close(b)
        b=m.book_new(x); mod=m.module(b,'Module1',m.function('MetaFunc',4000)+'''
Public Sub SetMetadataViaVba()
Application.MacroOptions Macro:="MetaFunc", Description:="VBA-set description", Category:=3, ArgumentDescriptions:=Array("VBA-set argument")
End Sub
''')
        m.save(b,'metadata-before.xlsm',52)
        m.enter(b,[('metadata','=MetaFunc(2)')])
        x.MacroOptions(Macro="'metadata-before.xlsm'!MetaFunc",Description='Description survives',
                       Category=3,ArgumentDescriptions=['Argument description'])
        if (m.OUT/'Metadata.bas').exists(): (m.OUT/'Metadata.bas').unlink()
        mod.Export(str(m.OUT/'Metadata.bas'))
        m.save(b,'metadata-after.xlsm',52); m.save(b,'metadata-after.xlsb',50)
        m.close(b)
        b=m.open_book(x,'metadata-after.xlsm')
        m.observe(x,b,'metadata-fresh-reopen')
        if (m.OUT/'Metadata-reopened.bas').exists(): (m.OUT/'Metadata-reopened.bas').unlink()
        b.VBProject.VBComponents('Module1').Export(str(m.OUT/'Metadata-reopened.bas'))
        x.MacroOptions(Macro="'metadata-after.xlsm'!MetaFunc",Category='DNA UDF custom')
        if (m.OUT/'Metadata-custom.bas').exists(): (m.OUT/'Metadata-custom.bas').unlink()
        b.VBProject.VBComponents('Module1').Export(str(m.OUT/'Metadata-custom.bas'))
        m.save(b,'metadata-custom.xlsm',52); m.close(b)
        b=m.open_book(x,'metadata-after.xlsm')
        x.Run("'metadata-after.xlsm'!SetMetadataViaVba")
        if (m.OUT/'Metadata-vba.bas').exists(): (m.OUT/'Metadata-vba.bas').unlink()
        b.VBProject.VBComponents('Module1').Export(str(m.OUT/'Metadata-vba.bas'))
        m.save(b,'metadata-vba.xlsm',52); m.close(b)
        b=m.book_new(x)
        m.module(b,'Module1','''Public Function LAMBDA(ByVal x As Double, ByVal y As Double) As Double
LAMBDA = x + y + 5000
End Function
''')
        b.Names.Add(Name='Rate',RefersTo='=2')
        m.enter(b,[('udf-named-LAMBDA','=_xludf.LAMBDA(Rate,3)'),
                   ('module-LAMBDA','=Module1.LAMBDA(Rate,3)')])
        m.save(b,'collision.xlsm',52); m.close(b)
        b=m.open_book(x,'collision.xlsm'); m.observe(x,b,'collision-xml-reopened')
        m.save(b,'collision.xlsm',52); m.save(b,'collision.xlsb',50); m.close(b)
    finally:
        try:
            x.DisplayAlerts=False
            for i in range(x.Workbooks.Count,0,-1): m.close(x.Workbooks(i))
            x.Quit()
        finally:
            x=None; gc.collect(); (m.OUT/'excel-observations.json').write_bytes(existing)


if __name__=='__main__': main()
