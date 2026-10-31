"""Extract exact XML and XLSB records from the measured fixtures; standard library only.

The offset is in the uncompressed part; record_hex includes the type/size header.
This is an evidence dumper, not a second workbook reader or formula evaluator.
"""
from pathlib import Path
import argparse
import hashlib
import json
import struct
import xml.etree.ElementTree as ET
import zipfile

NS={'m':'http://schemas.openxmlformats.org/spreadsheetml/2006/main'}
KINDS={0:'BrtRowHdr',8:'BrtFmlaString',9:'BrtFmlaNum',10:'BrtFmlaBool',11:'BrtFmlaError',
       39:'BrtName',355:'BrtSupBookSrc',357:'BrtSupSelf',358:'BrtSupSame',
       361:'BrtPlaceholderName',362:'BrtExternSheet',426:'BrtArrFmla',427:'BrtShrFmla',
       667:'BrtSupAddin',359:'BrtSupTabs',360:'BrtBeginSupBook',
       577:'BrtSupNameStart',585:'BrtSupNameFmla',586:'BrtSupNameBits',587:'BrtSupNameEnd',
       1036:'BrtNameExt',664:'BrtBeginFnGroup',665:'BrtFnGroup',666:'BrtEndFnGroup',
       5108:'BrtExternalLinksAlternateUrls'}


def records(data):
    at=0
    while at<len(data):
        start=at; kind=0; size=0
        for k in range(2):
            b=data[at]; at+=1; kind|=(b&127)<<(7*k)
            if b<128: break
        for k in range(4):
            b=data[at]; at+=1; size|=(b&127)<<(7*k)
            if b<128: break
        end=at+size
        if end>len(data): raise ValueError('Truncated record')
        yield start,kind,data[at:end],data[start:end]
        at=end


def wide(b,at):
    n=struct.unpack_from('<I',b,at)[0]; at+=4
    if n==0xffffffff: return None,at
    return b[at:at+2*n].decode('utf-16le'),at+2*n


def binary(part,data):
    out=[]; row=0; counts={}
    for offset,kind,b,raw in records(data):
        counts[str(kind)]=counts.get(str(kind),0)+1
        if kind==0: row=struct.unpack_from('<I',b)[0]; continue
        if part.startswith('xl/externalLinks/') or kind in KINDS:
            e=dict(offset=offset,type=kind,kind=KINDS.get(kind,'record-'+str(kind)),
                   record_hex=raw.hex(' '),payload_hex=b.hex(' '))
            if kind==39:
                flags,key,scope=struct.unpack_from('<IBI',b)
                name,at=wide(b,9); cce=struct.unpack_from('<I',b,at)[0]; at+=4
                e.update(name=name,flags=f'0x{flags:08x}',scope=scope,
                         rgce_hex=b[at:at+cce].hex(' '))
            elif kind in (8,9,10,11):
                col=struct.unpack_from('<I',b)[0]
                at={9:18,10:11,11:11}.get(kind)
                if kind==8: _,at=wide(b,8); at+=2
                cce=struct.unpack_from('<I',b,at)[0]; at+=4
                e.update(row=row+1,column=col+1,rgce_hex=b[at:at+cce].hex(' '))
            elif kind==426:
                r1,r2,c1,c2=struct.unpack_from('<IIII',b)
                cce=struct.unpack_from('<I',b,17)[0]
                e.update(range=[r1+1,r2+1,c1+1,c2+1],rgce_hex=b[21:21+cce].hex(' '))
            out.append(e)
    return dict(record_counts=counts,records=out)


def inspect(path):
    result=dict(file=path.name,size=path.stat().st_size,
                sha256=hashlib.sha256(path.read_bytes()).hexdigest(),parts={})
    with zipfile.ZipFile(path) as z:
        result['vbaProject']='xl/vbaProject.bin' in z.namelist()
        for name in z.namelist():
            if name in ('xl/workbook.xml','xl/_rels/workbook.xml.rels','xl/_rels/workbook.bin.rels') or name.startswith('xl/externalLinks/'):
                data=z.read(name)
                result['parts'][name]=data.decode('utf-8') if name.endswith(('.xml','.rels')) else binary(name,data)
            elif name in ('xl/workbook.bin','xl/worksheets/sheet1.bin'):
                result['parts'][name]=binary(name,z.read(name))
            elif name=='xl/worksheets/sheet1.xml':
                # Keep exact lexical <c> fragments, including <f> attributes and file caches.
                import re
                xml=z.read(name).decode('utf-8')
                result['parts'][name]=[m.group(0) for m in re.finditer(r'<c\b[^>]*>.*?</c>',xml) if '<f' in m.group(0)]
    return result


def main():
    p=argparse.ArgumentParser(); p.add_argument('directory',type=Path,nargs='?',default=Path(__file__).parent)
    args=p.parse_args()
    data=[inspect(f) for f in sorted(args.directory.iterdir()) if f.suffix in ('.xlsx','.xlsm','.xlsb','.xlam') and not f.name.startswith('~$')]
    out=args.directory/'file-forms.json'
    out.write_text(json.dumps(data,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
    print(f'{len(data)} files -> {out}')


if __name__=='__main__': main()
