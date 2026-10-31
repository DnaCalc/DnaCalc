"""Excel reopens actual calc-doc outputs; cached values are observed before and after recalc."""
from pathlib import Path
import argparse
import gc
import hashlib
import importlib.util
import json
import re
import shutil
import zipfile
import win32com.client
import win32process
import measure as m


def metadata_records(path):
    spec=importlib.util.spec_from_file_location('vba_udf_wire',m.HERE/'inspect.py')
    wire=importlib.util.module_from_spec(spec); spec.loader.exec_module(wire)
    with zipfile.ZipFile(path) as z:
        return [dict(type=kind,payload_hex=body.hex(' '))
                for _,kind,body,_ in wire.records(z.read('xl/workbook.bin'))
                if kind in (664,665,1036)]


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--product', type=Path)
    parser.add_argument('--out', type=Path)
    parser.add_argument('--paths', action='store_true')
    parser.add_argument('--path-variants', action='store_true')
    parser.add_argument('--binding-controls', action='store_true')
    parser.add_argument('--help-ui', type=Path)
    parser.add_argument('--metadata-controls', action='store_true')
    parser.add_argument('--binary-controls', action='store_true')
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
        if args.help_ui:
            stop=output.with_suffix('.stop')
            stop.unlink(missing_ok=True)
            b=m.retry(lambda:x.Workbooks.Open(str(args.help_ui.resolve()),UpdateLinks=0))
            x.Visible=True
            m.event('help-ui-ready',file=str(args.help_ui.resolve()),hwnd=x.Hwnd,stop=str(stop.resolve()))
            # UI actions use Computer Use, confined to the recorded owned window.
            while not stop.exists():
                m.pythoncom.PumpWaitingMessages(); m.time.sleep(0.2)
            return
        if not args.metadata_controls and not args.binary_controls:
            b=m.open_book(x,'loaded-xml.xlsb')
            m.observe(x,b,'fresh-reopen-loaded-xml.xlsb'); m.close(b)
        outputs=['local-xlsm-saved.xlsm','local-xlsb-saved.xlsm','local-xlsb-edited.xlsm',
                     'consumer-xlsx-saved.xlsx','consumer-xlsb-saved.xlsx',
                     'collision-xlsm-reentered.xlsm','collision-xlsb-reentered.xlsm',
                     'metadata-unqualified-xlsb-saved.xlsm','metadata-custom-vba-xlsb-saved.xlsm']
        if args.metadata_controls:
            outputs=[name for name in outputs if name.startswith('metadata-')]
        if args.binary_controls:
            outputs=['local-xlsb-preserved.xlsb','collision-xlsb-preserved.xlsb',
                     'consumer-xlsb-preserved.xlsb','metadata-custom-vba-xlsb-preserved.xlsb']
        for name in outputs:
            providers=[]
            if name.startswith('consumer'):
                for p in ['provider.xlsm','probe.xlam']:
                    providers.append(m.open_book(x,p))
            if args.binary_controls:
                source_name=name.replace('-xlsb-preserved.xlsb','.xlsb')
                source=m.open_book(x,source_name)
                m.observe(x,source,'binary-original-'+source_name)
                expected_rows=m.EVENTS[-1]['rows']; m.close(source)
                assert expected_rows[0]['value']>0,'the original macro must execute after full recalculation'
            b=m.retry(lambda:x.Workbooks.Open(str(product/name),UpdateLinks=0))
            m.event('package', file=name, sha256=hashlib.sha256((product/name).read_bytes()).hexdigest())
            m.observe(x,b,name+'-before-recalc',False)
            m.observe(x,b,name)
            if args.binary_controls:
                actual_rows=m.EVENTS[-1]['rows']
                expected=[(row['cell'],row['value'],row['text']) for row in expected_rows]
                actual=[(row['cell'],row['value'],row['text']) for row in actual_rows]
                m.event('binary-preservation',source=source_name,file=name,matches=expected==actual)
                assert expected==actual,(expected,actual)
            s=b.Worksheets(1)
            if name.startswith('collision') and '-reentered.' in name:
                m.event('collision-reentry',file=name,B1=s.Range('B1').Formula2,
                        B1_text=s.Range('B1').Text,D1=s.Range('D1').Formula2,D1_text=s.Range('D1').Text,
                        B2=s.Range('B2').Formula2,B2_text=s.Range('B2').Text,
                        D2=s.Range('D2').Formula2,D2_text=s.Range('D2').Text)
            if args.metadata_controls:
                stem=name.split('-xlsb-')[0]
                exported=product/(stem+'-excel.bas')
                exported.unlink(missing_ok=True)
                m.retry(lambda:b.VBProject.VBComponents('Module1').Export(str(exported)))
                attributes=lambda data:[line for line in data.splitlines() if line.startswith(b'Attribute MetaFunc.VB_')]
                expected_attributes=attributes((original/(stem.replace('metadata','Metadata',1)+'.bas')).read_bytes())
                actual_attributes=attributes(exported.read_bytes())
                resaved=product/(stem+'-excel.xlsb')
                resaved.unlink(missing_ok=True)
                m.retry(lambda:b.SaveAs(str(resaved),50))
                expected=metadata_records(original/(stem+'.xlsb'))
                actual=metadata_records(resaved)
                m.event('metadata-resave',file=name,source_binary=stem+'.xlsb',
                        resaved_sha256=hashlib.sha256(resaved.read_bytes()).hexdigest(),
                        expected_records=expected,actual_records=actual,
                        exported_attributes=[line.decode('cp1252') for line in actual_attributes],
                        matches=(expected==actual and expected_attributes==actual_attributes),
                        scope='Excel reopens XML and writes binary help metadata; UI display is not read by this script')
                assert expected==actual,(expected,actual)
                assert expected_attributes==actual_attributes,(expected_attributes,actual_attributes)
            m.close(b)
            for p in providers: m.close(p)
        if args.binding_controls:
            provider=m.open_book(x,'provider.xlsm')
            b=m.book_new(x)
            m.retry(lambda:setattr(b.Worksheets(1).Range('C1'),'Value2',2))
            m.enter(b,[('qualified-reference-argument',"='provider.xlsm'!OtherFunc(C1)"),
                       ('module-reference-argument',"='provider.xlsm'!Module1.OtherFunc(C1)")])
            m.observe(x,b,'qualified-reference-arguments')
            m.save(b,'binding-arguments.xlsx',51)
            m.close(b); m.close(provider)
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
