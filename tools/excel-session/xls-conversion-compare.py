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
V = '{urn:schemas-microsoft-com:vml}'
VX = '{urn:schemas-microsoft-com:office:excel}'


def relationships(z, path):
    folder, name = posixpath.split(path)
    relpath = posixpath.join(folder, '_rels', name + '.rels')
    if relpath not in z.namelist():
        return {}
    root = ET.fromstring(z.read(relpath))
    return {r.get('Id'): r for r in root.findall(P + 'Relationship')}


def core_properties(z):
    links = [rel for rel in relationships(z, '').values()
             if rel.get('Type', '').endswith('/metadata/core-properties')]
    if not links:
        return {}
    if len(links) != 1:
        raise ValueError('multiple core-properties relationships')
    root = ET.fromstring(z.read(target('', links[0])))
    dc = '{http://purl.org/dc/elements/1.1/}'
    cp = '{http://schemas.openxmlformats.org/package/2006/metadata/core-properties}'
    dcterms = '{http://purl.org/dc/terms/}'
    result = {}
    for key, tag in [('title', dc + 'title'), ('subject', dc + 'subject'), ('author', dc + 'creator'),
                     ('keywords', cp + 'keywords'), ('description', dc + 'description'),
                     ('last_modified_by', cp + 'lastModifiedBy'), ('revision', cp + 'revision'), ('category', cp + 'category'),
                     ('created', dcterms + 'created'), ('modified', dcterms + 'modified')]:
        element = root.find(tag)
        if element is not None:
            result[key] = element.text or ''
    return result


def extended_properties(z):
    links = [rel for rel in relationships(z, '').values()
             if rel.get('Type', '').endswith('/extended-properties')]
    if not links:
        return {}
    if len(links) != 1:
        raise ValueError('multiple extended-properties relationships')
    root = ET.fromstring(z.read(target('', links[0])))
    ns = '{http://schemas.openxmlformats.org/officeDocument/2006/extended-properties}'
    result = {}
    for key, tag in [('application', 'Application'), ('manager', 'Manager'), ('company', 'Company')]:
        element = root.find(ns + tag)
        if element is not None:
            result[key] = element.text or ''
    return result


def custom_properties(z):
    links = [rel for rel in relationships(z, '').values()
             if rel.get('Type', '').endswith('/custom-properties')]
    if not links:
        return []
    if len(links) != 1:
        raise ValueError('multiple custom-properties relationships')
    root = ET.fromstring(z.read(target('', links[0])))
    result = []
    for prop in root:
        values = [{'type': child.tag.rsplit('}', 1)[-1], 'value': child.text or ''} for child in prop]
        result.append({'name': prop.get('name'), 'values': values})
    return sorted(result, key=lambda item: (item['name'] or '', repr(item['values'])))


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


def note_geometry(z, sheet, sheet_path, sheet_rels):
    result = []
    for ref in sheet.findall(M + 'legacyDrawing'):
        rel = sheet_rels.get(ref.get(R + 'id'))
        if rel is None:
            raise ValueError('missing legacy drawing relationship')
        root = ET.fromstring(z.read(target(sheet_path, rel)))
        for shape in root.iter(V + 'shape'):
            client = shape.find(VX + 'ClientData')
            if client is None or client.get('ObjectType') != 'Note':
                continue
            style = {}
            for item in shape.get('style', '').split(';'):
                name, sep, value = item.partition(':')
                if sep and name.strip().lower() in ('margin-left', 'margin-top', 'width', 'height', 'visibility'):
                    style[name.strip().lower()] = value.strip()
            anchor = client.find(VX + 'Anchor')
            result.append({'row': client.findtext(VX + 'Row'), 'column': client.findtext(VX + 'Column'),
                           'anchor': None if anchor is None else [part.strip() for part in (anchor.text or '').split(',')],
                           'style': style})
    return sorted(result, key=repr)


def form_controls(z, sheet, sheet_path, sheet_rels):
    result = []
    for ref in sheet.findall(M + 'legacyDrawing'):
        rel = sheet_rels.get(ref.get(R + 'id'))
        if rel is None:
            raise ValueError('missing legacy drawing relationship')
        root = ET.fromstring(z.read(target(sheet_path, rel)))
        for shape in root.iter(V + 'shape'):
            client = shape.find(VX + 'ClientData')
            if client is not None and client.get('ObjectType') != 'Note':
                result.append({'object_type': client.get('ObjectType'), 'client_data': tree(client)})
    controls = sheet.find(M + 'controls')
    return {'legacy': sorted(result, key=repr), 'controls': None if controls is None else tree(controls)}


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
            for end in ('pos', 'ext'):
                marker = anchor.find(XDR + end)
                if marker is not None:
                    position[end] = dict(marker.attrib)
            obj = {'anchor': anchor.tag.rsplit('}', 1)[-1], 'position': position,
                   'types': sorted(x.tag.rsplit('}', 1)[-1] for x in anchor if x.tag in
                                   (XDR + 'sp', XDR + 'pic', XDR + 'graphicFrame', XDR + 'grpSp', XDR + 'cxnSp'))}
            obj['group_structure'] = [tree(group) for group in anchor.findall(XDR + 'grpSp')]
            obj['shape_properties'] = [tree(properties) for element in anchor
                                       if element.tag in (XDR + 'sp', XDR + 'cxnSp')
                                       for properties in element.findall(XDR + 'spPr')]
            obj['shape_text'] = []
            for element in anchor:
                if element.tag != XDR + 'sp':
                    continue
                body = element.find(XDR + 'txBody')
                if body is not None:
                    text = ''.join(node.text or '' for node in body.iter(A + 't'))
                    obj['shape_text'].append({'utf16_units': len(text.encode('utf-16-le')) // 2,
                                              'sha256': hashlib.sha256(text.encode('utf-8')).hexdigest(),
                                              'body_properties': None if body.find(A + 'bodyPr') is None else tree(body.find(A + 'bodyPr')),
                                              'paragraph_properties': [tree(p) for p in body.iter(A + 'pPr')],
                                              'run_properties': [tree(run) for run in body.iter(A + 'rPr')]})
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
                title = chart_root.find(f'{C}chart/{C}title')
                obj['chart_title'] = None if title is None else {
                    'text': [e.text or '' for e in title.iter() if e.tag in (A + 't', C + 'v')],
                    'formulas': [e.text or '' for e in title.iter(C + 'f')],
                    'font_properties': [tree(e) for e in title.iter() if e.tag in (A + 'defRPr', A + 'rPr', A + 'endParaRPr')]}
                legend = chart_root.find(f'{C}chart/{C}legend')
                obj['chart_legend'] = None if legend is None else tree(legend)
                settings = chart_root.find(C + 'printSettings')
                obj['chart_print_settings'] = None if settings is None else tree(settings)
                obj['chart_types'] = sorted({x.tag.rsplit('}', 1)[-1] for x in chart_root.iter()
                                             if x.tag.startswith(C) and x.tag.endswith('Chart')})
                obj['group_labels'] = [None if group.find(C + 'dLbls') is None else tree(group.find(C + 'dLbls'))
                                       for group in chart_root.iter() if group.tag.startswith(C) and group.tag.endswith('Chart')]
                obj['series_formulas'] = [x.text or '' for x in chart_root.iter(C + 'f')]
                obj['series_solid_fills'] = []
                for series in chart_root.iter(C + 'ser'):
                    fill = series.find(f'{C}spPr/{A}solidFill')
                    color = None if fill is None else next(iter(fill), None)
                    obj['series_solid_fills'].append(None if color is None else {
                        'kind': color.tag.rsplit('}', 1)[-1], 'value': color.get('val')})
                obj['series_line_colors'] = []
                obj['series_labels'] = []
                for series in chart_root.iter(C + 'ser'):
                    fill = series.find(f'{C}spPr/{A}ln/{A}solidFill')
                    color = None if fill is None else next(iter(fill), None)
                    obj['series_line_colors'].append(None if color is None else {
                        'kind': color.tag.rsplit('}', 1)[-1], 'value': color.get('val')})
                    labels = series.find(C + 'dLbls')
                    obj['series_labels'].append(None if labels is None else tree(labels))
                obj['series_value_caches'] = []
                for series in chart_root.iter(C + 'ser'):
                    data = series.find(C + 'val')
                    if data is None:
                        data = series.find(C + 'yVal')
                    cache = data.find(f'{C}numRef/{C}numCache') if data is not None else None
                    count = cache.find(C + 'ptCount') if cache is not None else None
                    obj['series_value_caches'].append(None if cache is None else {
                        'count': count.get('val') if count is not None else None,
                        'points': [{'index': point.get('idx'), 'value': point.findtext(C + 'v')}
                                   for point in cache.findall(C + 'pt')]})
                obj['series_category_caches'] = []
                for series in chart_root.iter(C + 'ser'):
                    data = series.find(C + 'cat')
                    if data is None:
                        data = series.find(C + 'xVal')
                    cache = None; kind = None
                    if data is not None:
                        for ref, cache_name in (('numRef', 'numCache'), ('strRef', 'strCache')):
                            cache = data.find(f'{C}{ref}/{C}{cache_name}')
                            if cache is not None:
                                kind = cache_name
                                break
                    count = cache.find(C + 'ptCount') if cache is not None else None
                    obj['series_category_caches'].append(None if cache is None else {
                        'kind': kind, 'count': count.get('val') if count is not None else None,
                        'points': [{'index': point.get('idx'), 'value': point.findtext(C + 'v')}
                                   for point in cache.findall(C + 'pt')]})
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
                obj['axis_font_properties'] = [
                    {'type': axis.tag.rsplit('}', 1)[-1],
                     'properties': [tree(e) for tx in axis.findall(C + 'txPr') for e in tx.iter()
                                    if e.tag in (A + 'defRPr', A + 'rPr', A + 'endParaRPr')]}
                    for axis in chart_root.iter() if axis.tag in (C + 'catAx', C + 'valAx', C + 'dateAx', C + 'serAx')]
                obj['axis_titles'] = [
                    {'type': axis.tag.rsplit('}', 1)[-1],
                     'text': [e.text or '' for e in title.iter() if e.tag in (A + 't', C + 'v')],
                     'formulas': [e.text or '' for e in title.iter(C + 'f')],
                     'font_properties': [tree(e) for e in title.iter() if e.tag in (A + 'defRPr', A + 'rPr', A + 'endParaRPr')]}
                    for axis in chart_root.iter() if axis.tag in (C + 'catAx', C + 'valAx', C + 'dateAx', C + 'serAx')
                    if (title := axis.find(C + 'title')) is not None]
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


def sheet_protection(sheet):
    element = sheet.find(M + 'sheetProtection')
    if element is None:
        return None
    # Report protection switches without exposing a saved password verifier or salt.
    return {'attributes': {key: value for key, value in element.attrib.items()
                           if key not in ('password', 'hashValue', 'saltValue', 'algorithmName', 'spinCount')},
            'password_present': any(key in element.attrib for key in ('password', 'hashValue'))}


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
            comment_runs = []
            for rel in shrels.values():
                if rel.get('Type', '').endswith('/comments'):
                    comment_root = ET.fromstring(z.read(target(path, rel)))
                    comments.extend((e.get('ref'), ''.join(x.text or '' for x in e.iter(M + 't')))
                                    for e in comment_root.iter(M + 'comment'))
                    for note in comment_root.iter(M + 'comment'):
                        runs = []
                        for run in note.findall(f'{M}text/{M}r'):
                            props = run.find(M + 'rPr')
                            runs.append({'text': ''.join(x.text or '' for x in run.iter(M + 't')),
                                         'properties': None if props is None else tree(props)})
                        comment_runs.append({'ref': note.get('ref'), 'runs': runs})
            sheets.append({'name': s.get('name'), 'state': s.get('state', 'visible'), 'cells': cells,
                           'sheet_properties': None if sh.find(M + 'sheetPr') is None else tree(sh.find(M + 'sheetPr')),
                           'sheet_protection': sheet_protection(sh),
                           'filter_mode': sh.find(M + 'sheetPr') is not None and sh.find(M + 'sheetPr').get('filterMode', '0') in ('1', 'true'),
                           'auto_filter': None if sh.find(M + 'autoFilter') is None else tree(sh.find(M + 'autoFilter')),
                           'default_column_width': (sh.find(M + 'sheetFormatPr').get('defaultColWidth')
                                                    if sh.find(M + 'sheetFormatPr') is not None else None),
                           'columns': [(e.get('min'), e.get('max'), e.get('width'))
                                       for e in sh.findall(f'{M}cols/{M}col')],
                           'views': [{k: e.get(k) for k in ('tabSelected', 'showOutlineSymbols', 'defaultGridColor', 'colorId', 'view', 'zoomToFit',
                                                             'zoomScale', 'zoomScaleNormal', 'zoomScaleSheetLayoutView')
                                      if e.get(k) is not None} for e in sh.findall(f'{M}sheetViews/{M}sheetView')],
                           'merges': merges, 'links': sorted(links, key=repr), 'comments': sorted(comments),
                           'comment_runs': sorted(comment_runs, key=repr),
                           'note_geometry': note_geometry(z, sh, path, shrels),
                           'form_controls': form_controls(z, sh, path, shrels),
                           'scenarios': None if sh.find(M + 'scenarios') is None else tree(sh.find(M + 'scenarios')),
                           'conditional_formats': [tree(e) for e in sh.findall(M + 'conditionalFormatting')],
                           'validations': [tree(e) for e in sh.findall(f'{M}dataValidations/{M}dataValidation')],
                           'ignored_errors': [tree(e) for e in sh.findall(f'{M}ignoredErrors/{M}ignoredError')],
                           'sort_states': [tree(e) for e in sh.findall(M + 'sortState')] +
                                          [tree(e) for e in sh.findall(f'{M}autoFilter/{M}sortState')],
                           'drawings': drawing_objects(z, sh, path, shrels),
                           'printer_settings': printer_settings(z, sh, path, shrels),
                           'page_margins': None if sh.find(M + 'pageMargins') is None else tree(sh.find(M + 'pageMargins')),
                           'page_setup': None if sh.find(M + 'pageSetup') is None else tree(sh.find(M + 'pageSetup')),
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
        return {'core_properties': core_properties(z), 'extended_properties': extended_properties(z),
                'custom_properties': custom_properties(z),
                'date1904': props is not None and props.get('date1904', '0') in ('1', 'true'),
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
