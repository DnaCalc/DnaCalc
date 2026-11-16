#!/usr/bin/env python3
"""Compare controlled dc-4j0e.12 Excel conversions with the product's saved packages.

The input directory is produced by the calc-doc write_legacy_excel_session_inputs test. The
Excel JSON is produced by xls-conversion-job.ps1. This compares workbook sheet identity, defined
names, cell formulas and stored values, number formats, merges, and hyperlinks. Other package
features are explicitly listed as not compared; a match is not a calculation verdict.
"""
import argparse
import base64
import hashlib
import io
import json
import posixpath
import xml.etree.ElementTree as ET
import zipfile
from pathlib import Path

M = '{http://schemas.openxmlformats.org/spreadsheetml/2006/main}'
R = '{http://schemas.openxmlformats.org/officeDocument/2006/relationships}'
P = '{http://schemas.openxmlformats.org/package/2006/relationships}'
XDR = '{http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing}'
C = '{http://schemas.openxmlformats.org/drawingml/2006/chart}'


def relationships(z, path):
    folder, name = posixpath.split(path)
    relpath = posixpath.join(folder, '_rels', name + '.rels')
    if relpath not in z.namelist():
        return {}
    root = ET.fromstring(z.read(relpath))
    return {r.get('Id'): r for r in root.findall(P + 'Relationship')}


def target(base, rel):
    if rel.get('TargetMode') == 'External':
        return rel.get('Target')
    return posixpath.normpath(posixpath.join(posixpath.dirname(base), rel.get('Target').lstrip('/'))) if not rel.get('Target').startswith('/') else rel.get('Target').lstrip('/')


def tree(element):
    """Retain XML content and attributes; namespace prefixes and attribute order are irrelevant."""
    return {'tag': element.tag, 'attributes': dict(sorted(element.attrib.items())),
            'text': (element.text or '').strip(), 'children': [tree(x) for x in element]}


def drawing_objects(z, sheet, sheet_path, sheet_rels):
    result = []
    for ref in sheet.findall(M + 'drawing'):
        rel = sheet_rels.get(ref.get(R + 'id'))
        if rel is None:
            raise ValueError('missing drawing relationship')
        path = target(sheet_path, rel)
        root = ET.fromstring(z.read(path)); rels = relationships(z, path)
        for anchor in root:
            position = {}
            for end in ('from', 'to'):
                marker = anchor.find(XDR + end)
                if marker is not None:
                    position[end] = {x.tag.rsplit('}', 1)[-1]: x.text for x in marker}
            obj = {'anchor': anchor.tag.rsplit('}', 1)[-1], 'position': position,
                   'types': sorted(x.tag.rsplit('}', 1)[-1] for x in anchor if x.tag in
                                   (XDR + 'sp', XDR + 'pic', XDR + 'graphicFrame', XDR + 'grpSp'))}
            chart = next(anchor.iter(C + 'chart'), None)
            if chart is not None:
                chart_rel = rels.get(chart.get(R + 'id'))
                if chart_rel is None:
                    raise ValueError('missing chart relationship')
                chart_root = ET.fromstring(z.read(target(path, chart_rel)))
                obj['chart_types'] = sorted({x.tag.rsplit('}', 1)[-1] for x in chart_root.iter()
                                             if x.tag.startswith(C) and x.tag.endswith('Chart')})
                obj['series_formulas'] = [x.text or '' for x in chart_root.iter(C + 'f')]
                obj['chart_text'] = [x.text or '' for x in chart_root.iter()
                                     if x.tag.endswith('}t') and x.text]
            result.append(obj)
    return result


def package(data):
    with zipfile.ZipFile(io.BytesIO(data)) as z:
        wbpath = 'xl/workbook.xml'
        wb = ET.fromstring(z.read(wbpath))
        rels = relationships(z, wbpath)
        strings = []
        if 'xl/sharedStrings.xml' in z.namelist():
            root = ET.fromstring(z.read('xl/sharedStrings.xml'))
            strings = [''.join(x.text or '' for x in s.iter(M + 't')) for s in root.findall(M + 'si')]
        formats = []
        if 'xl/styles.xml' in z.namelist():
            root = ET.fromstring(z.read('xl/styles.xml'))
            custom = {int(e.get('numFmtId')): e.get('formatCode') for e in root.findall(f'{M}numFmts/{M}numFmt')}
            for xf in root.findall(f'{M}cellXfs/{M}xf'):
                ident = int(xf.get('numFmtId', '0'))
                formats.append(custom.get(ident, f'builtin:{ident}'))
        names = sorted(((e.get('name'), e.get('localSheetId'), e.text or '') for e in wb.findall(f'{M}definedNames/{M}definedName')), key=repr)
        sheets = []
        for s in wb.findall(f'{M}sheets/{M}sheet'):
            rel = rels.get(s.get(R + 'id'))
            if rel is None:
                raise ValueError('missing sheet relationship: ' + str(s.get('name')))
            path = target(wbpath, rel)
            sh = ET.fromstring(z.read(path))
            shrels = relationships(z, path)
            cells = {}
            for c in sh.iter(M + 'c'):
                typ = c.get('t', 'n')
                v = c.find(M + 'v')
                val = None if v is None else v.text or ''
                if typ == 's' and val is not None:
                    val = strings[int(val)]; typ = 'str'
                elif typ == 'inlineStr':
                    val = ''.join(x.text or '' for x in c.iter(M + 't')); typ = 'str'
                elif typ in ('str', 's'):
                    typ = 'str'
                elif typ == 'n' and val not in (None, ''):
                    val = float(val).hex()
                f = c.find(M + 'f')
                style = int(c.get('s', '0'))
                cells[c.get('r')] = {'type': typ, 'value': val, 'formula': None if f is None else (f.text or ''),
                                     'formula_attributes': {} if f is None else dict(f.attrib),
                                     'number_format': formats[style] if style < len(formats) else f'unknown-style:{style}'}
            merges = sorted(e.get('ref') for e in sh.findall(f'{M}mergeCells/{M}mergeCell'))
            links = []
            for link in sh.findall(f'{M}hyperlinks/{M}hyperlink'):
                rel = shrels.get(link.get(R + 'id'))
                links.append((link.get('ref'), None if rel is None else rel.get('Target'), link.get('location'), link.get('display')))
            comments = []
            for rel in shrels.values():
                if rel.get('Type', '').endswith('/comments'):
                    comment_root = ET.fromstring(z.read(target(path, rel)))
                    comments.extend((e.get('ref'), ''.join(x.text or '' for x in e.iter(M + 't')))
                                    for e in comment_root.iter(M + 'comment'))
            sheets.append({'name': s.get('name'), 'state': s.get('state', 'visible'), 'cells': cells,
                           'merges': merges, 'links': sorted(links, key=repr), 'comments': sorted(comments),
                           'conditional_formats': [tree(e) for e in sh.findall(M + 'conditionalFormatting')],
                           'validations': [tree(e) for e in sh.findall(f'{M}dataValidations/{M}dataValidation')],
                           'drawings': drawing_objects(z, sh, path, shrels)})
        props = wb.find(M + 'workbookPr')
        return {'date1904': props is not None and props.get('date1904', '0') in ('1', 'true'), 'names': names, 'sheets': sheets}


def compare(expected, actual):
    diffs = []
    def walk(path, a, b):
        if isinstance(a, dict) and isinstance(b, dict):
            for k in sorted(a.keys() | b.keys()):
                if k not in a or k not in b:
                    diffs.append({'path': path + '/' + k, 'excel': a.get(k), 'dnacalc': b.get(k)})
                else:
                    walk(path + '/' + k, a[k], b[k])
        elif isinstance(a, list) and isinstance(b, list):
            if len(a) != len(b):
                diffs.append({'path': path + '/length', 'excel': len(a), 'dnacalc': len(b)})
            for i, (x, y) in enumerate(zip(a, b)):
                walk(path + '/' + str(i), x, y)
        elif a != b:
            diffs.append({'path': path, 'excel': a, 'dnacalc': b})
    walk('', expected, actual)
    return diffs


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--inputs', required=True, type=Path)
    parser.add_argument('--excel-result', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--extract-pivot', type=Path, help='write the job-generated pivot twins here for dc-4j0e.13')
    args = parser.parse_args()
    reading = json.loads(args.excel_result.read_text(encoding='utf-8-sig'))
    if reading.get('fatal'):
        raise SystemExit('Excel job failed: ' + str(reading['fatal']))
    if args.extract_pivot:
        pivot = reading.get('pivot') or {}
        if pivot.get('error') or not pivot.get('xls_base64') or not pivot.get('xlsx_base64'):
            raise SystemExit('Excel pivot twins missing: ' + str(pivot.get('error')))
        args.extract_pivot.mkdir(parents=True, exist_ok=True)
        for extension in ('xls', 'xlsx'):
            dest = args.extract_pivot / ('pivot.' + extension)
            if dest.exists():
                raise SystemExit('refusing to overwrite ' + str(dest))
            dest.write_bytes(base64.b64decode(pivot[extension + '_base64'], validate=True))
    expected_files = json.loads((args.inputs / 'cases.json').read_text(encoding='utf-8'))['files']
    actual_files = [item['file'] for item in reading['files']]
    if sorted(expected_files) != sorted(actual_files):
        raise SystemExit('Excel result does not contain exactly the requested fixtures')
    results = []
    for item in reading['files']:
        name = item['file']
        if Path(name).name != name:
            raise SystemExit('invalid fixture name')
        source = args.inputs / name
        if hashlib.sha256(source.read_bytes()).hexdigest() != item['sha256'].lower():
            raise SystemExit('input hash mismatch: ' + name)
        row = {'file': name}
        if item.get('error') or not item.get('package_base64'):
            row['error'] = item.get('error') or 'Excel package missing'
        else:
            try:
                excel = package(base64.b64decode(item['package_base64'], validate=True))
                ours = package((args.inputs / (source.stem + '.dnacalc.xlsx')).read_bytes())
                row['differences'] = compare(excel, ours)
            except (ValueError, KeyError, ET.ParseError, zipfile.BadZipFile, OSError) as exc:
                row['error'] = str(exc)
        results.append(row)
    output = {'files': results, 'not_compared': ['calculated provenance', 'drawing formatting and image contents',
              'chart formatting', 'conditional formatting style definitions', 'pivot tables and caches',
              'VBA project contents', 'style properties other than number formats', 'other metadata']}
    args.output.write_text(json.dumps(output, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    print(f"Compared {sum('differences' in r for r in results)}/{len(results)} files; {sum(len(r.get('differences', [])) for r in results)} differences; {sum('error' in r for r in results)} errors. Not compared: {', '.join(output['not_compared'])}.")


if __name__ == '__main__':
    main()
