"""Follow-up measurements: unqualified MacroOptions, non-ASCII case and invalid declarations."""
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
        (m.OUT/'identifier-observations.json').write_text(json.dumps(m.EVENTS,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
    m.event=record
    x=win32com.client.DispatchEx('Excel.Application')
    pid=win32process.GetWindowThreadProcessId(x.Hwnd)[1]
    if pid==3144: raise RuntimeError('Owner Excel')
    m.event('environment',pid=pid,hwnd=x.Hwnd,excel=x.Version,build=x.Build,excel_os=x.OperatingSystem)
    try:
        x.Visible=False; x.DisplayAlerts=False; x.AskToUpdateLinks=False
        b=m.open_book(x,'metadata-vba.xlsm')
        x.MacroOptions(Macro='MetaFunc',Description='Unqualified COM description',Category='DNA UDF custom',
                       ArgumentDescriptions=['Unqualified COM argument'])
        path=m.OUT/'Metadata-unqualified.bas'
        if path.exists(): path.unlink()
        b.VBProject.VBComponents('Module1').Export(str(path))
        m.save(b,'metadata-unqualified.xlsm',52)
        m.save(b,'metadata-unqualified.xlsb',50); m.close(b)
        b=m.open_book(x,'metadata-vba.xlsm')
        mod=b.VBProject.VBComponents('Module1')
        mod.CodeModule.AddFromString('''Public Sub SetCustomViaVba()
Application.MacroOptions Macro:="MetaFunc", Description:="Custom category description", Category:="DNA UDF custom", ArgumentDescriptions:=Array("Custom category argument")
End Sub
''')
        x.Run("'metadata-vba.xlsm'!SetCustomViaVba")
        path=m.OUT/'Metadata-custom-vba.bas'
        if path.exists(): path.unlink()
        mod.Export(str(path))
        m.save(b,'metadata-custom-vba.xlsm',52)
        m.save(b,'metadata-custom-vba.xlsb',50); m.close(b)
        b=m.open_book(x,'local.xlsm')
        s=b.Worksheets(1)
        for row,label,text in [(38,'uppercase-accent','=CAFÉ(2)'),(39,'uppercase-scandinavian','=ÅNGSTRÖM(2)')]:
            s.Cells(row,1).Value2=label; s.Cells(row,2).Formula2=text
        m.observe(x,b,'unicode-case')
        m.save(b,'unicode-case.xlsm',52); m.close(b)
        # AddFromString accepts source text that need not compile. The COM run attempts
        # below compile it; Excel returns an error without a UI dialog in this instance.
        for name in ['ValidName','Foo.Bar','_Leading','2Foo']:
            b=m.book_new(x)
            source=m.function(name,1)+'Public Sub CompileProbe()\nEnd Sub\n'
            mod=m.module(b,'Module1',source)
            m.event('invalid-declaration-source',name=name,source=mod.CodeModule.Lines(1,mod.CodeModule.CountOfLines))
            try:
                x.Run('CompileProbe')
                m.event('invalid-declaration-run',name=name,result='returned')
            except Exception as e:
                m.event('invalid-declaration-run',name=name,error=str(e))
            m.close(b)
    finally:
        try:
            x.DisplayAlerts=False
            for i in range(x.Workbooks.Count,0,-1): m.close(x.Workbooks(i))
            x.Quit()
        finally:
            x=None; gc.collect(); (m.OUT/'excel-observations.json').write_bytes(existing)


if __name__=='__main__': main()
