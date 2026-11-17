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
A = '{http://schemas.openxmlformats.org/drawingml/2006/main}'
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
            'text': (element.text or '') if element.tag == M + 't' else (element.text or '').strip(),
            'children': [tree(x) for x in element]}


def string_item(element):
    """Cell text excludes phonetic runs, which are compared separately."""
    if element is None:
        raise ValueError('inline string content is missing')
    def text_without_phonetics(node):
        if node.tag == M + 'rPh':
            return ''
        if node.tag == M + 't':
            return node.text or ''
        return ''.join(text_without_phonetics(child) for child in node)
    return {'text': text_without_phonetics(element),
            'runs': [tree(run) for run in element.findall(M + 'rPh')],
            'properties': [tree(prop) for prop in element.findall(M + 'phoneticPr')]}


def cell_styles(root):
    """Resolve indexed cell style components before comparing their XML definitions.

    This is a structural comparison: different representations of the same displayed style
    can still be reported as differences. It does not resolve named-style inheritance.
    """
    collections = {
        'fontId': root.findall(f'{M}fonts/{M}font'),
        'fillId': root.findall(f'{M}fills/{M}fill'),
        'borderId': root.findall(f'{M}borders/{M}border'),
    }
    result = []
    for xf in root.findall(f'{M}cellXfs/{M}xf'):
        definition = tree(xf)
        for name, items in collections.items():
            index = int(xf.get(name, '0'))
            if index >= len(items):
                raise ValueError(f'invalid {name} index {index}')
            definition['attributes'].pop(name, None)
            definition[name] = tree(items[index])
        # Number formats are compared by meaning in the adjacent number_format field.
        definition['attributes'].pop('numFmtId', None)
        result.append(definition)
    return result


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
            images = []
            for blip in anchor.iter(A + 'blip'):
                for attribute in ('embed', 'link'):
                    ident = blip.get(R + attribute)
                    if ident is None:
                        continue
                    image_rel = rels.get(ident)
                    if image_rel is None:
                        raise ValueError('missing image relationship')
                    if image_rel.get('TargetMode') == 'External':
                        images.append({'external': image_rel.get('Target')})
                    else:
                        data = z.read(target(path, image_rel))
                        images.append({'sha256': hashlib.sha256(data).hexdigest(), 'bytes': len(data)})
            obj['images'] = images
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
                obj['chart_geometry'] = [
                    {'type': group.tag.rsplit('}', 1)[-1],
                     'settings': {name: child.get('val') for name in
                                  ('varyColors', 'gapWidth', 'overlap', 'firstSliceAng', 'holeSize')
                                  if (child := group.find(C + name)) is not None}}
                    for group in chart_root.iter()
                    if group.tag in {C + 'barChart', C + 'pieChart', C + 'doughnutChart'}]
                obj['chart_settings'] = {
                    name: node.get('val') for name in ('plotVisOnly', 'dispBlanksAs')
                    if (node := chart_root.find('.//' + C + name)) is not None}
                obj['value_axes'] = [
                    {name: node.get('val') for name in ('orientation', 'min', 'max', 'majorUnit', 'minorUnit')
                     if (node := axis.find('.//' + C + name)) is not None}
                    for axis in chart_root.iter(C + 'valAx')]
            result.append(obj)
    return result


def external_caches(z, workbook, workbook_path, workbook_rels):
    result = []
    for entry in workbook.findall(f'{M}externalReferences/{M}externalReference'):
        rel = workbook_rels.get(entry.get(R + 'id'))
        if rel is None:
            result.append({'error': 'missing external-link relationship'})
            continue
        root = ET.fromstring(z.read(target(workbook_path, rel)))
        book = root.find(M + 'externalBook')
        if book is None:
            result.append({'error': 'external link is not an external workbook'})
            continue
        result.append({'sheet_names': [e.get('val') for e in book.findall(f'{M}sheetNames/{M}sheetName')],
                       'sheet_data': [tree(e) for e in book.findall(f'{M}sheetDataSet/{M}sheetData')]})
    return result


def pivot_caches(z, workbook, workbook_path, workbook_rels):
    result = []
    for cache in workbook.findall(f'{M}pivotCaches/{M}pivotCache'):
        rel = workbook_rels.get(cache.get(R + 'id'))
        if rel is None:
            raise ValueError('missing pivot cache relationship')
        path = target(workbook_path, rel)
        definition = ET.fromstring(z.read(path))
        records = []
        for item in relationships(z, path).values():
            if item.get('Type', '').endswith('/pivotCacheRecords'):
                records.append(tree(ET.fromstring(z.read(target(path, item)))))
        result.append({'cache_id': cache.get('cacheId'), 'definition': tree(definition), 'records': records})
    return result


def pivot_tables(z, sheet, sheet_path, sheet_rels):
    result = []
    for ref in sheet.findall(f'{M}pivotParts/{M}pivotPart'):
        rel = sheet_rels.get(ref.get(R + 'id'))
        if rel is None:
            raise ValueError('missing pivot table relationship')
        result.append(tree(ET.fromstring(z.read(target(sheet_path, rel)))))
    return result


def printer_settings(z, sheet, sheet_path, sheet_rels):
    setup = sheet.find(M + 'pageSetup')
    ident = None if setup is None else setup.get(R + 'id')
    if ident is None:
        return None
    rel = sheet_rels.get(ident)
    if rel is None or not rel.get('Type', '').endswith('/printerSettings'):
        raise ValueError('missing printer settings relationship')
    data = z.read(target(sheet_path, rel))
    return {'sha256': hashlib.sha256(data).hexdigest(), 'bytes': len(data)}


def header_footer_pictures(z, sheet, sheet_path, sheet_rels):
    result = []
    for ref in sheet.findall(M + 'legacyDrawingHF'):
        rel = sheet_rels.get(ref.get(R + 'id'))
        if rel is None:
            result.append({'error': 'missing header/footer drawing relationship'})
            continue
        path = target(sheet_path, rel)
        drawing = ET.fromstring(z.read(path))
        images = []
        for item in relationships(z, path).values():
            if item.get('Type', '').endswith('/image'):
                data = z.read(target(path, item))
                images.append({'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()})
        result.append({'drawing': tree(drawing), 'images': sorted(images, key=lambda e: e['sha256'])})
    return result


def package(data):
    with zipfile.ZipFile(io.BytesIO(data)) as z:
        wbpath = 'xl/workbook.xml'
        wb = ET.fromstring(z.read(wbpath))
        rels = relationships(z, wbpath)
        strings = []
        if 'xl/sharedStrings.xml' in z.namelist():
            root = ET.fromstring(z.read('xl/sharedStrings.xml'))
            strings = [string_item(s) for s in root.findall(M + 'si')]
        formats = []
        styles = []
        indexed_colors = None
        if 'xl/styles.xml' in z.namelist():
            root = ET.fromstring(z.read('xl/styles.xml'))
            styles = cell_styles(root)
            palette = root.find(f'{M}colors/{M}indexedColors')
            if palette is not None:
                indexed_colors = [item.get('rgb') for item in palette.findall(M + 'rgbColor')]
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
                phonetic = None
                if typ == 's' and val is not None:
                    item = strings[int(val)]; val = item['text']; phonetic = {'runs': item['runs'], 'properties': item['properties']} if item['runs'] or item['properties'] else None; typ = 'str'
                elif typ == 'inlineStr':
                    item = string_item(c.find(M + 'is')); val = item['text']; phonetic = {'runs': item['runs'], 'properties': item['properties']} if item['runs'] or item['properties'] else None; typ = 'str'
                elif typ in ('str', 's'):
                    typ = 'str'
                elif typ == 'n' and val not in (None, ''):
                    val = float(val).hex()
                f = c.find(M + 'f')
                style = int(c.get('s', '0'))
                cells[c.get('r')] = {'type': typ, 'value': val, 'formula': None if f is None else (f.text or ''),
                                     'formula_attributes': {} if f is None else dict(f.attrib),
                                     'number_format': formats[style] if style < len(formats) else f'unknown-style:{style}',
                                     'style_definition': styles[style] if style < len(styles) else f'unknown-style:{style}',
                                     'phonetic_visible': c.get('ph', '0') in ('1', 'true'), 'phonetic': phonetic}
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
                           'default_column_width': (sh.find(M + 'sheetFormatPr').get('defaultColWidth')
                                                    if sh.find(M + 'sheetFormatPr') is not None else None),
                           'columns': [(e.get('min'), e.get('max'), e.get('width'))
                                       for e in sh.findall(f'{M}cols/{M}col')],
                           'views': [{k: e.get(k) for k in ('showOutlineSymbols', 'defaultGridColor', 'colorId', 'view',
                                                             'zoomScale', 'zoomScaleNormal', 'zoomScaleSheetLayoutView')
                                      if e.get(k) is not None} for e in sh.findall(f'{M}sheetViews/{M}sheetView')],
                           'merges': merges, 'links': sorted(links, key=repr), 'comments': sorted(comments),
                           'conditional_formats': [tree(e) for e in sh.findall(M + 'conditionalFormatting')],
                           'validations': [tree(e) for e in sh.findall(f'{M}dataValidations/{M}dataValidation')],
                           'ignored_errors': [tree(e) for e in sh.findall(f'{M}ignoredErrors/{M}ignoredError')],
                           'sort_states': [tree(e) for e in sh.findall(M + 'sortState')] +
                                          [tree(e) for e in sh.findall(f'{M}autoFilter/{M}sortState')],
                           'drawings': drawing_objects(z, sh, path, shrels),
                           'printer_settings': printer_settings(z, sh, path, shrels),
                           'header_footer': None if sh.find(M + 'headerFooter') is None else tree(sh.find(M + 'headerFooter')),
                           'header_footer_pictures': header_footer_pictures(z, sh, path, shrels),
                           'phonetic_columns': [(e.get('min'), e.get('max')) for e in sh.findall(f'{M}cols/{M}col')
                                                if e.get('phonetic', '0') in ('1', 'true')],
                           'row_display_flags': [(e.get('r'), e.get('thickTop', '0') in ('1', 'true'),
                                                  e.get('thickBot', '0') in ('1', 'true'),
                                                  e.get('ph', '0') in ('1', 'true'))
                                                 for e in sh.findall(f'{M}sheetData/{M}row')
                                                 if any(e.get(k, '0') in ('1', 'true') for k in ('thickTop', 'thickBot', 'ph'))],
                           'phonetic_properties': [tree(e) for e in sh.findall(M + 'phoneticPr')],
                           'pivots': pivot_tables(z, sh, path, shrels)})
        props = wb.find(M + 'workbookPr')
        calc = wb.find(M + 'calcPr')
        return {'date1904': props is not None and props.get('date1904', '0') in ('1', 'true'),
                'full_precision': calc is None or calc.get('fullPrecision', '1') in ('1', 'true'), 'names': names,
                'indexed_colors': indexed_colors, 'external_caches': external_caches(z, wb, wbpath, rels),
                'pivot_caches': pivot_caches(z, wb, wbpath, rels), 'sheets': sheets}


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
    parser.add_argument('--extract-excel-inputs', type=Path, help='extract Excel-created legacy inputs for product conversion')
    parser.add_argument('--excel-inputs', type=Path, help='compare Excel-created inputs after the product conversion test')
    args = parser.parse_args()
    reading = json.loads(args.excel_result.read_text(encoding='utf-8-sig'))
    if reading.get('fatal'):
        raise SystemExit('Excel job failed: ' + str(reading['fatal']))
    generated = list(reading.get('generated', []))
    pivot = reading.get('pivot')
    if pivot is not None:
        pivot_source = pivot.get('xls_base64')
        generated.append({'file': 'excel-pivot.xls', 'source_base64': pivot_source,
                          'sha256': hashlib.sha256(base64.b64decode(pivot_source, validate=True)).hexdigest() if pivot_source else '',
                          'package_base64': pivot.get('xlsx_base64'), 'error': pivot.get('error')})
    generated_names = [item['file'] for item in generated if item.get('source_base64')]
    if len(set(generated_names)) != len(generated_names) or any(Path(name).name != name or name in ('.', '..', 'cases.json', 'outcomes.json') for name in generated_names):
        raise SystemExit('invalid or duplicate Excel-generated fixture name')
    if args.extract_excel_inputs:
        directory = args.extract_excel_inputs
        directory.mkdir(parents=True, exist_ok=True)
        destinations = [directory / name for name in generated_names] + [directory / 'cases.json']
        if any(path.exists() for path in destinations):
            raise SystemExit('refusing to overwrite Excel-generated input')
        for item in generated:
            if not item.get('source_base64'):
                continue
            data = base64.b64decode(item['source_base64'], validate=True)
            if hashlib.sha256(data).hexdigest() != item.get('sha256', '').lower():
                raise SystemExit('Excel-generated input hash mismatch: ' + item['file'])
            (directory / item['file']).write_bytes(data)
        (directory / 'cases.json').write_text(json.dumps(generated_names, indent=2) + '\n', encoding='utf-8')
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
    cases = json.loads((args.inputs / 'cases.json').read_text(encoding='utf-8'))
    expected_files = cases['files']
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
    generated_results = []
    outcomes = None
    if args.excel_inputs:
        outcomes = json.loads((args.excel_inputs / 'outcomes.json').read_text(encoding='utf-8'))
    for item in generated:
        name = item['file']
        row = {'file': name}
        if item.get('error') or not item.get('source_base64') or not item.get('package_base64'):
            row['error'] = item.get('error') or 'Excel-generated source or package missing'
        elif outcomes is None:
            row['not_compared'] = 'extract the generated inputs and run the product conversion test'
        elif name not in outcomes:
            row['error'] = 'product conversion outcome missing'
        elif outcomes[name] is not None:
            row['error'] = 'product conversion refused: ' + outcomes[name]
        else:
            try:
                source = (args.excel_inputs / name).read_bytes()
                if hashlib.sha256(source).hexdigest() != item['sha256'].lower():
                    raise ValueError('Excel-generated input hash mismatch')
                excel = package(base64.b64decode(item['package_base64'], validate=True))
                ours = package((args.excel_inputs / (name + '.dnacalc.xlsx')).read_bytes())
                row['differences'] = compare(excel, ours)
            except (ValueError, KeyError, ET.ParseError, zipfile.BadZipFile, OSError) as exc:
                row['error'] = str(exc)
        generated_results.append(row)
    repairs = reading.get('repairs', [])
    if sorted(cases.get('repair_cases', [])) != sorted(item['file'] for item in repairs):
        raise SystemExit('Excel result does not contain exactly the requested repair fixtures')
    repair_results = []
    for item in repairs:
        name = item['file']
        if Path(name).name != name or hashlib.sha256((args.inputs / name).read_bytes()).hexdigest() != item['sha256'].lower():
            raise SystemExit('repair fixture name or hash mismatch: ' + name)
        repair_results.append({key: item.get(key) for key in ('file', 'opened', 'saved', 'error')})
    output = {'files': results, 'excel_generated': generated_results, 'repair_observations': repair_results,
              'not_compared': ['calculated provenance', 'drawing formatting',
              'chart formatting', 'conditional formatting style definitions', 'pivot refresh and external cache sources',
              'VBA project contents', 'named-style inheritance and rendered style equivalence', 'other metadata']}
    args.output.write_text(json.dumps(output, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    all_results = results + generated_results
    print(f"Compared {sum('differences' in r for r in all_results)}/{len(all_results)} files; {sum(len(r.get('differences', [])) for r in all_results)} differences; {sum('error' in r for r in all_results)} comparison errors. "
          f"Repair observations: {len(repair_results)}, including {sum(bool(r.get('error')) for r in repair_results)} errors. Not compared: {', '.join(output['not_compared'])}.")


if __name__ == '__main__':
    main()
