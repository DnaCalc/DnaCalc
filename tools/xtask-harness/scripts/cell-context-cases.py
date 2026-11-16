#!/usr/bin/env python3
"""Cell-context witness case generators (dc-483v). Writes one case set per family into
tools/xtask-harness/cell-context/<family>.json (format dnacalc-cell-context/1, described in
tools/xtask-harness/src/cell_context.rs).

  python3 -I tools/xtask-harness/scripts/cell-context-cases.py [--family reference-info]

A family names its layouts once and lists (layout, address, formula) cases; adding a
reference-shaped case is one fam.case(...) line. Every case records layout_sha, the SHA-256 of its layout and
formula, so an Excel answer is only ever judged against the layout Excel built.
"""
import argparse, hashlib, json, pathlib, re, struct

ROOT = pathlib.Path(__file__).resolve().parents[3]
OUT = ROOT / 'tools/xtask-harness/cell-context'


def n(x):
    return '0x%016x' % struct.unpack('>Q', struct.pack('>d', float(x)))[0]


def t(s):
    return 'text:' + s


class Family:
    """A family's layouts (named once) and its cases, each naming its layout."""

    def __init__(self):
        self.layouts, self.cases = {}, []

    def layout(self, key, sheets, names=None):
        self.layouts[key] = {'sheets': sheets, 'names': names or {}}
        return key

    def case(self, cid, function, layout, at, formula):
        resolved = dict(self.layouts[layout], at=at, formula=formula)
        sha = hashlib.sha256(json.dumps(resolved, sort_keys=True, separators=(',', ':'),
                                        ensure_ascii=True).encode()).hexdigest()
        self.cases.append({'id': cid, 'function': function, 'layout': layout, 'at': at, 'formula': formula,
                           'layout_sha': sha})


# The shared layout of the reference family: a formula sheet with every kind of cell, a data
# sheet and a sheet whose name needs quotes. The formula cell's default place leaves spill room.
def base_sheets(hidden_rows=(), hidden_columns=()):
    sheet1 = {
        'A1': n(1), 'A2': t('abc'), 'A3': 'logical:true', 'A4': 'error:NA', 'A5': '=A1+1',
        'A6': '="x"&A2', 'B1': n(10), 'B2': n(20), 'B3': n(30), 'B4': n(40), 'B5': n(50),
        'C3': '=SUM(B1:B5)', 'D1': '=SEQUENCE(3)', 'E1': t("quoted'"), 'E2': t('^caret'),
        'E3': n(0.5), 'E4': n(-1234.5), 'E5': n(45000.25),
    }
    return [
        {'name': 'Sheet1', 'cells': sheet1, 'hidden_rows': list(hidden_rows), 'hidden_columns': list(hidden_columns)},
        {'name': 'Data', 'cells': {'A1': n(7), 'B2': '=Sheet1!A1*2', 'C3': t('data')}},
        {'name': 'Third Sheet', 'cells': {'E9': n(3)}},
    ]


NAMES = {'Rate': '=Sheet1!$B$2', 'Block': '=Data!$A$1:$C$3'}
REFS = ['A1', 'A2', 'A3', 'A4', 'A5', 'A6', 'A7', 'B3', '$C$7', 'A1:A3', 'B2:D4', 'D1', 'D2', 'D1#',
        'Data!C5', 'Data!B2', "'Third Sheet'!E9", 'XFD1048576', '3:5', 'C:D', 'Rate', 'Block',
        'INDIRECT("B4")', 'OFFSET(A1,2,3)', 'A1:B2 B2:C3', '(A1,B3)']
PLACES = ['Sheet1!H10', 'Sheet1!A20', 'Data!Z100', "'Third Sheet'!B2", 'Sheet1!XFD1048576']


def reference_info():
    fam = Family()
    s = fam.layout('base', base_sheets(), NAMES)
    for f in ('ROW', 'COLUMN'):
        for at in PLACES:
            fam.case(f'{f.lower()}-noarg-{at}', f, s, at, f'={f}()')
        for i, r in enumerate(REFS):
            fam.case(f'{f.lower()}-{i:02}', f, s, 'Sheet1!H10', f'={f}({r})')
        for i, r in enumerate(['1', '"A1"', 'TRUE', '#N/A', '{1,2}']):
            fam.case(f'{f.lower()}-value-{i}', f, s, 'Sheet1!H10', f'={f}({r})')
    for at in PLACES:
        fam.case(f'sheet-noarg-{at}', 'SHEET', s, at, '=SHEET()')
    for i, r in enumerate(REFS + ['"Data"', '"third sheet"', '"Sheet1"', '"nope"', '""', '1', 'TRUE', '#N/A', '{1,2}']):
        fam.case(f'sheet-{i:02}', 'SHEET', s, 'Data!F6', f'=SHEET({r})')
    for f in ('ISFORMULA', 'FORMULATEXT'):
        for i, r in enumerate(REFS + ['1', '"A5"', 'TRUE', '#N/A']):
            fam.case(f'{f.lower()}-{i:02}', f, s, 'Sheet1!H10', f'={f}({r})')
        fam.case(f'{f.lower()}-self', f, s, 'Sheet1!H10', f'={f}(H10)')
    shifts = [(0, 0), (1, 0), (0, 1), (2, 3), (-1, 0), (0, -1), (1048575, 0), (0, 16383)]
    sizes = [None, (1, 1), (2, 1), (1, 2), (3, 2), (0, 1), (1, 0), (-1, 1), (2, -1)]
    i = 0
    for base in ('A1', 'B2', 'B2:C3', 'Data!A1', 'D1#'):
        for (r, c) in shifts:
            for size in sizes:
                args = f'{base},{r},{c}' + ('' if size is None else f',{size[0]},{size[1]}')
                fam.case(f'offset-{i:03}', 'OFFSET', s, 'Sheet1!H10', f'=OFFSET({args})')
                i += 1
    for j, formula in enumerate(['=SUM(OFFSET(B1,0,0,5))', '=OFFSET(A1,,)', '=OFFSET(A1,1,,2)', '=OFFSET(Rate,1,0)',
                                 '=OFFSET(A1,"1",0)', '=OFFSET(A1,TRUE,0)', '=OFFSET(A1,#N/A,0)', '=OFFSET(1,0,0)',
                                 '=ROWS(OFFSET(A1,0,0,4,2))', '=OFFSET(A1:B2,0,0,,)']):
        fam.case(f'offset-extra-{j}', 'OFFSET', s, 'Sheet1!H10', formula)
    # Left out: "filename", and "address" of another sheet's cell; both name the workbook
    # ([Book3]Data!$C$3), which depends on how many workbooks the Excel session has made.
    infos = ['address', 'col', 'row', 'contents', 'type', 'width', 'format', 'prefix', 'protect',
             'color', 'parentheses', 'ADDRESS', 'nope', '']
    for k, info in enumerate(infos):
        for i, r in enumerate(['A1', 'A2', 'A3', 'A4', 'A5', 'A7', 'B1:B5', 'D2', 'E1', 'E2', 'E3', 'Data!C3',
                               "'Third Sheet'!E9", 'Rate']):
            if info.lower() == 'address' and '!' in r:
                continue
            fam.case(f'cell-{k:02}-{i:02}', 'CELL', s, 'Sheet1!H10', f'=CELL("{info}",{r})')
    return fam


def subtotal_hidden():
    """SUBTOTAL and AGGREGATE over rows some of which are hidden and some of which hold nested
    SUBTOTAL or AGGREGATE formulas, with an error and text among the values."""
    fam = Family()
    values = {f'B{r}': n(v) for r, v in zip(range(1, 11), [3, 1, 4, 1.5, 9, 2.25, 6, 5, 3.5, 8])}
    layouts = {
        'plain': ({}, []),
        'hidden': ({}, [3, 6]),
        'nested': ({'B4': '=SUBTOTAL(9,B1:B3)', 'B8': '=AGGREGATE(9,0,B5:B7)'}, []),
        'nested-hidden': ({'B4': '=SUBTOTAL(9,B1:B3)', 'B8': '=AGGREGATE(9,0,B5:B7)'}, [2, 8]),
        'errors': ({'B5': 'error:Div0', 'B7': t('x'), 'B9': 'logical:true'}, [6]),
        'all-hidden': ({}, list(range(1, 11))),
    }
    for lname, (extra, hidden) in layouts.items():
        cells = dict(values)
        cells.update(extra)
        sheets = fam.layout(lname, [{'name': 'Sheet1', 'cells': cells, 'hidden_rows': hidden}])
        for f in list(range(1, 12)) + list(range(101, 112)):
            fam.case(f'subtotal-{lname}-{f}', 'SUBTOTAL', sheets, 'Sheet1!D12', f'=SUBTOTAL({f},B1:B10)')
        fam.case(f'subtotal-{lname}-2d', 'SUBTOTAL', sheets, 'Sheet1!D12', '=SUBTOTAL(109,A1:C10)')
        fam.case(f'subtotal-{lname}-two', 'SUBTOTAL', sheets, 'Sheet1!D12', '=SUBTOTAL(9,B1:B5,B6:B10)')
        for f in range(1, 20):
            for opt in range(8):
                k = '' if f < 14 else (',2' if f in (14, 15) else ',0.5')
                fam.case(f'aggregate-{lname}-{f}-{opt}', 'AGGREGATE', sheets, 'Sheet1!D12',
                         f'=AGGREGATE({f},{opt},B1:B10{k})')
    return fam


def spill_placement():
    """Array results placed with and without spill room: a blocking constant, the sheet edge,
    a blocking formula, and the free case."""
    fam = Family()
    data = {f'B{r}': n(v) for r, v in zip(range(1, 6), [10, 20, 30, 40, 50])}
    data.update({f'C{r}': n(v) for r, v in zip(range(1, 6), [1, 2, 3, 4, 5])})
    formulas = {
        'TRANSPOSE': ['=TRANSPOSE(B1:B5)', '=TRANSPOSE(B1:C5)', '=TRANSPOSE(B1)'],
        'SEQUENCE': ['=SEQUENCE(3)', '=SEQUENCE(2,3)', '=SEQUENCE(1,1)', '=SEQUENCE(1048576)'],
        'TREND': ['=TREND(B1:B5)', '=TREND(B1:B5,C1:C5,{6;7})'],
        'FREQUENCY': ['=FREQUENCY(B1:B5,{15,35})', '=FREQUENCY(B1:B5,C1:C2)'],
        'WRAPCOLS': ['=WRAPCOLS(B1:B5,2)', '=WRAPCOLS(B1:B5,2,0)'],
    }
    blocks = {'free': {}, 'below': {'H11': n(1)}, 'right': {'I10': t('x')}, 'diagonal': {'I11': '=1+1'},
              'far': {'K14': n(0)}}
    for function, fs in formulas.items():
        for j, formula in enumerate(fs):
            for bname, block in blocks.items():
                cells = dict(data)
                cells.update(block)
                sheets = fam.layout(bname, [{'name': 'Sheet1', 'cells': cells}])
                fam.case(f'{function.lower()}-{j}-{bname}', function, sheets, 'Sheet1!H10', formula)
            for at in ('Sheet1!XFD1048576', 'Sheet1!XFD1', 'Sheet1!A1048576'):
                sheets = fam.layout('free', [{'name': 'Sheet1', 'cells': dict(data)}])
                fam.case(f'{function.lower()}-{j}-edge-{at[7:]}', function, sheets, at, formula)
    # A spill over cells its own result reads (Excel's P05, dc-hsa5): the formula in A1, its
    # source below or beside it, the arguments of the serial bank layout in B1:E1.
    own = fam.layout('own', [{'name': 'Sheet1', 'cells': {'B1': n(0.9), 'C1': n(1), 'D1': t('-1'), 'E1': n(365),
                                                           'G6': n(5), 'H6': n(6)}}])
    for j, formula in enumerate(['=OFFSET(B1,C1,D1,E1)', '=OFFSET(B1,1,-1,3)', '=OFFSET(A1,1,0,3)', '=A2:A4',
                                 '=A2:A4*1', '=A2:A4&""', '=SEQUENCE(3)+A3', '=SUM(A2:A3)+SEQUENCE(3)',
                                 '=A2:A6', '=OFFSET(A1,1,0,6)', '=TRANSPOSE(B1:B3)', '=IF(TRUE,A2:A4)',
                                 '=INDEX(A2:A10,0)', '=ROWS(A2:A4)+SEQUENCE(3)', '=ISBLANK(A2:A4)']):
        fam.case(f'own-{j:02}', 'SPILL', own, 'Sheet1!A1', formula)
    for j, formula in enumerate(['=G2:G7', '=G2:H7', '=OFFSET(G2,0,0,3,2)']):
        fam.case(f'own-g-{j:02}', 'SPILL', own, 'Sheet1!G1', formula)
    return fam


def ifs_family():
    """COUNTIFS, SUMIFS, AVERAGEIFS, MAXIFS and MINIFS over ranges of mixed cells."""
    fam = Family()
    a = ['0x4024000000000000', t('abc'), n(20), '', t('a*c'), 'logical:true', n(30), 'error:NA', '=""', n(45292),
         t('20'), n(-5), t('Abc'), n(20), t('*')]
    b = [n(v) for v in [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]]
    c = [t(x) for x in ['x', 'y', 'x', 'y', 'x', 'y', 'x', 'y', 'x', 'y', 'x', 'y', 'x', 'y', 'x']]
    cells = {}
    for i in range(15):
        if a[i]:
            cells[f'A{i + 1}'] = a[i]
        cells[f'B{i + 1}'] = b[i]
        cells[f'C{i + 1}'] = c[i]
    cells.update({'E1': n(20), 'E2': t('x'), 'E3': '', 'E4': t('>10')})
    cells = {k: v for k, v in cells.items() if v}
    sheets = fam.layout('mixed', [{'name': 'Sheet1', 'cells': cells, 'hidden_rows': [4]}])
    criteria = ['">10"', '"<=20"', '"<>30"', '"=abc"', '"a*"', '"?bc"', '"~*"', '""', '"="', '"<>"', '20', '"20"',
                'TRUE', '"TRUE"', 'E1', 'E3', 'E4', '"#N/A"', '">=2024/01/01"', '"*"', '">"&E1', '{">10","<5"}']
    for i, cr in enumerate(criteria):
        fam.case(f'countifs-{i:02}', 'COUNTIFS', sheets, 'Sheet1!H1', f'=COUNTIFS(A1:A15,{cr})')
        fam.case(f'countifs2-{i:02}', 'COUNTIFS', sheets, 'Sheet1!H1', f'=COUNTIFS(A1:A15,{cr},C1:C15,"x")')
        for f in ('SUMIFS', 'AVERAGEIFS', 'MAXIFS', 'MINIFS'):
            fam.case(f'{f.lower()}-{i:02}', f, sheets, 'Sheet1!H1', f'={f}(B1:B15,A1:A15,{cr})')
            fam.case(f'{f.lower()}2-{i:02}', f, sheets, 'Sheet1!H1', f'={f}(B1:B15,A1:A15,{cr},C1:C15,E2)')
    for f, extra in (('COUNTIFS', 'A1:A15,">0",B1:B14,">0"'), ('SUMIFS', 'B1:B15,A1:A14,">0"'),
                     ('AVERAGEIFS', 'A1:A15,B1:B15,">100"'), ('MAXIFS', 'A1:A15,B1:B15,">0"'),
                     ('MINIFS', 'B1:B15,A1:A15,"zzz"'), ('SUMIFS', 'B1:B15,A1:B15,">0"')):
        fam.case(f'{f.lower()}-shape-{len(fam.cases):04}', f, sheets, 'Sheet1!H1', f'={f}({extra})')
    return fam


def lambda_helpers():
    """MAP, REDUCE, SCAN, BYROW, BYCOL and MAKEARRAY over a small grid of mixed cells."""
    fam = Family()
    cells = {f'B{r}': n(v) for r, v in zip(range(1, 5), [1, 2, 3, 4])}
    cells.update({f'C{r}': n(v) for r, v in zip(range(1, 5), [10, 20, 30, 40])})
    cells.update({'D1': t('a'), 'D2': 'logical:true', 'D4': 'error:Div0'})
    grid = fam.layout('grid', [{'name': 'Sheet1', 'cells': cells}])
    formulas = {
        'MAP': ['=MAP(B1:B4,LAMBDA(x,x*2))', '=MAP(B1:B4,C1:C4,LAMBDA(x,y,x+y))', '=MAP(D1:D4,LAMBDA(x,ISTEXT(x)))',
                '=MAP(B1:C4,LAMBDA(x,x^2))', '=MAP(B1:B4,LAMBDA(x,y,x))', '=MAP(B1:B4,C1:C3,LAMBDA(x,y,x+y))',
                '=MAP(D1:D4,LAMBDA(x,x))', '=MAP(B1:B4,LAMBDA(x,SEQUENCE(2)))'],
        'REDUCE': ['=REDUCE(0,B1:C4,LAMBDA(a,v,a+v))', '=REDUCE(1,B1:B4,LAMBDA(a,v,a*v))', '=REDUCE(,B1:B4,LAMBDA(a,v,a+v))',
                   '=REDUCE("",D1:D3,LAMBDA(a,v,a&v))', '=REDUCE(0,D1:D4,LAMBDA(a,v,a+1))', '=REDUCE(0,B1:B4,LAMBDA(a,a))'],
        'SCAN': ['=SCAN(0,B1:B4,LAMBDA(a,v,a+v))', '=SCAN(1,B1:C4,LAMBDA(a,v,a*v))', '=SCAN("",D1:D3,LAMBDA(a,v,a&v))',
                 '=SCAN(0,D1:D4,LAMBDA(a,v,a+v))'],
        'BYROW': ['=BYROW(B1:C4,LAMBDA(r,SUM(r)))', '=BYROW(B1:D4,LAMBDA(r,COUNTA(r)))', '=BYROW(B1:C4,LAMBDA(r,r))',
                  '=BYROW(B1:C4,LAMBDA(r,MAX(r)-MIN(r)))'],
        'BYCOL': ['=BYCOL(B1:C4,LAMBDA(c,SUM(c)))', '=BYCOL(B1:D4,LAMBDA(c,ROWS(c)))', '=BYCOL(B1:C4,LAMBDA(c,c))'],
        'MAKEARRAY': ['=MAKEARRAY(3,2,LAMBDA(r,c,r*10+c))', '=MAKEARRAY(2,2,LAMBDA(r,c,INDEX(B1:C4,r,c)))',
                      '=MAKEARRAY(0,2,LAMBDA(r,c,1))', '=MAKEARRAY(2,2,LAMBDA(r,1))', '=MAKEARRAY(1.9,2.1,LAMBDA(r,c,r+c))'],
    }
    for function, fs in formulas.items():
        for j, formula in enumerate(fs):
            fam.case(f'{function.lower()}-{j}', function, grid, 'Sheet1!H10', formula)
    return fam


def lambda_groupby():
    """Bank-coverage gaps (docs/plan/bank-coverage-2026-11-12.md): MAP, REDUCE, SCAN, BYROW, BYCOL and
    MAKEARRAY over every lambda body below and several input shapes; GROUPBY and PIVOTBY over a small
    sales table with every aggregation, header, total, sort and filter form; and functions that read the
    calculation's clock, compared through its date only (TODAY, INT(NOW())) since the recorded clock is
    Excel's NOW() before and after the cases."""
    fam = Family()
    cells = {}
    for r, (a, b, c) in enumerate([(1, 10, 'a'), (2, 20, 'b'), (3, 30, None), (4, 40, 'd'), (-5, 0.5, 'e')], start=1):
        cells[f'B{r}'], cells[f'C{r}'] = n(a), n(b)
        if c is not None:
            cells[f'D{r}'] = t(c)
    cells.update({'E1': 'logical:true', 'E2': 'error:NA', 'E3': n(0), 'E4': t('7'), 'E5': 'logical:false'})
    grid = fam.layout('grid', [{'name': 'Sheet1', 'cells': cells}])
    unary = ['x*2', 'x+1', 'x^2', 'ISNUMBER(x)', 'IF(x>2,x,-x)', 'TEXT(x,"0.0")', 'x&"!"', 'SQRT(x)', 'IFERROR(1/x,"inf")',
             'N(x)', 'LEN(x)', 'ROUND(x/3,2)', 'SEQUENCE(2)', 'x', 'ISBLANK(x)', '{1,2}']
    ranges = ['B1:B5', 'C1:C5', 'D1:D5', 'E1:E5', 'B1:C5', 'B1:E1', 'B3', '{1,2;3,4}']
    for body in unary:
        for rng in ranges:
            fam.case(f'map-{len(fam.cases):04}', 'MAP', grid, 'Sheet1!H10', f'=MAP({rng},LAMBDA(x,{body}))')
    for rng in ['B1:B5', 'C1:C5', 'B1:C5']:
        fam.case(f'map-{len(fam.cases):04}', 'MAP', grid, 'Sheet1!H10', f'=MAP({rng},C1:C5,LAMBDA(x,y,x*y))')
        fam.case(f'map-{len(fam.cases):04}', 'MAP', grid, 'Sheet1!H10', f'=MAP({rng},D1:D5,LAMBDA(x,y,x&y))')
    binary = ['a+v', 'a*v', 'a&v', 'MAX(a,v)', 'IF(ISNUMBER(v),a+v,a)', 'a+1', 'v', 'a', 'IFERROR(a+v,a)', 'a-v/2']
    for init in ['0', '1', '""', '', 'B1', '{0,0}']:
        for body in binary:
            for rng in ['B1:B5', 'B1:C5', 'D1:D5', 'E1:E5']:
                for f in ('REDUCE', 'SCAN'):
                    fam.case(f'{f.lower()}-{len(fam.cases):04}', f, grid, 'Sheet1!H10', f'={f}({init},{rng},LAMBDA(a,v,{body}))')
    rowbody = ['SUM(r)', 'COUNTA(r)', 'MAX(r)-MIN(r)', 'AVERAGE(r)', 'TEXTJOIN("-",TRUE,r)', 'ROWS(r)', 'COLUMNS(r)', 'r',
               'INDEX(r,1)', 'SUMPRODUCT(r,r)', 'CONCAT(r)', 'COUNT(r)']
    for body in rowbody:
        for rng in ['B1:C5', 'B1:E5', 'B1:B5', 'B2:D2', '{1,2;3,4}']:
            for f in ('BYROW', 'BYCOL'):
                fam.case(f'{f.lower()}-{len(fam.cases):04}', f, grid, 'Sheet1!H10', f'={f}({rng},LAMBDA(r,{body}))')
    for rows in ['1', '2', '3.7', '0', '-1', '"2"', 'TRUE', 'B2']:
        for cols in ['1', '2', '2.5', '0', 'C5']:
            for body in ['r*10+c', 'r', 'INDEX(B1:C5,r,c)', 'r&"/"&c', 'IF(r=c,1,0)']:
                fam.case(f'makearray-{len(fam.cases):04}', 'MAKEARRAY', grid, 'Sheet1!H10', f'=MAKEARRAY({rows},{cols},LAMBDA(r,c,{body}))')

    table = {'A1': t('Region'), 'B1': t('Product'), 'C1': t('Qty'), 'D1': t('Price')}
    data = [('North', 'Apples', 10, 1.5), ('South', 'Pears', 4, 2.0), ('North', 'Pears', 7, 2.25), ('East', 'Apples', 3, 1.25),
            ('South', 'Apples', 12, 1.5), ('North', 'Apples', 5, 1.75), ('East', 'Plums', 9, 3.0), ('South', 'Plums', 1, 2.75)]
    for i, (reg, prod, q, pr) in enumerate(data, start=2):
        table.update({f'A{i}': t(reg), f'B{i}': t(prod), f'C{i}': n(q), f'D{i}': n(pr)})
    sales = fam.layout('sales', [{'name': 'Sheet1', 'cells': table}])
    aggs = ['SUM', 'COUNT', 'COUNTA', 'AVERAGE', 'MAX', 'MIN', 'PRODUCT', 'MEDIAN', 'STDEV.S', 'VAR.P', 'PERCENTOF',
            'ARRAYTOTEXT', 'LAMBDA(x,SUM(x)*2)', 'LAMBDA(x,ROWS(x))']
    for agg in aggs:
        fam.case(f'groupby-{len(fam.cases):04}', 'GROUPBY', sales, 'Sheet1!G2', f'=GROUPBY(A2:A9,C2:C9,{agg})')
        fam.case(f'pivotby-{len(fam.cases):04}', 'PIVOTBY', sales, 'Sheet1!G2', f'=PIVOTBY(A2:A9,B2:B9,C2:C9,{agg})')
    for headers in ['0', '1', '2', '3', '']:
        for total in ['0', '1', '2', '-1', '-2', '']:
            for sort in ['1', '-1', '2', '-2', '']:
                fam.case(f'groupby-{len(fam.cases):04}', 'GROUPBY', sales, 'Sheet1!G2',
                         f'=GROUPBY(A1:B9,C1:D9,SUM,{headers},{total},{sort})')
    for headers in ['0', '1', '3']:
        for rtotal in ['0', '1', '-1']:
            for ctotal in ['0', '1', '-1']:
                fam.case(f'pivotby-{len(fam.cases):04}', 'PIVOTBY', sales, 'Sheet1!G2',
                         f'=PIVOTBY(A1:A9,B1:B9,C1:C9,SUM,{headers},{rtotal},,{ctotal})')
    for flt in ['C2:C9>4', 'A2:A9<>"East"', 'ISNUMBER(C2:C9)']:
        fam.case(f'groupby-{len(fam.cases):04}', 'GROUPBY', sales, 'Sheet1!G2', f'=GROUPBY(A2:A9,C2:C9,SUM,,,,{flt})')
        fam.case(f'pivotby-{len(fam.cases):04}', 'PIVOTBY', sales, 'Sheet1!G2', f'=PIVOTBY(A2:A9,B2:B9,C2:C9,SUM,,,,,,{flt})')
    for rel in ['0', '1']:
        fam.case(f'groupby-{len(fam.cases):04}', 'GROUPBY', sales, 'Sheet1!G2', f'=GROUPBY(A2:B9,C2:C9,SUM,,2,,,{rel})')
    fam.case(f'groupby-{len(fam.cases):04}', 'GROUPBY', sales, 'Sheet1!G2', '=GROUPBY(A2:A9,HSTACK(C2:C9,D2:D9),HSTACK(SUM,MAX))')
    fam.case(f'groupby-{len(fam.cases):04}', 'GROUPBY', sales, 'Sheet1!B5', '=GROUPBY(A2:A9,C2:C9,SUM)')   # no spill room

    clock = fam.layout('clock', [{'name': 'Sheet1', 'cells': {'A1': n(45000), 'A2': t('2024/03/15')}}])
    for formula in ['=TODAY()', '=INT(NOW())', '=YEAR(TODAY())', '=TODAY()-A1', '=DATEDIF(A1,TODAY(),"d")',
                    '=TEXT(TODAY(),"yyyy-mm-dd")', '=WEEKDAY(TODAY())', '=TODAY()>A1', '=DAYS(TODAY(),A2)',
                    '=EOMONTH(TODAY(),0)', '=YEARFRAC(A1,TODAY())', '=ISOWEEKNUM(TODAY())', '=MONTH(NOW())',
                    '=DATEVALUE("15/3")', '=VALUE("15 Mar")', '=NETWORKDAYS(A1,TODAY())']:
        fam.case(f'clock-{len(fam.cases):04}', formula[1:formula.index('(')], clock, 'Sheet1!C1', formula)
    return fam


def lambda_references():
    """What the LAMBDA helpers hand their LAMBDA (fn2, 2026-11-16): references into a range argument,
    values from an array, an omitted REDUCE/SCAN initial value, MAP's alignment by row and column."""
    fam = Family()
    cells = {'B1': n(1), 'B2': n(2), 'B3': n(3), 'C1': n(10), 'C2': t('t'), 'C3': 'logical:true', 'D1': t('a'), 'D2': t('b')}
    grid = fam.layout('refs', [{'name': 'Sheet1', 'cells': cells}])
    for formula in [
            '=MAP(B1:B2,LAMBDA(x,ISREF(x)))', '=MAP({1,2},LAMBDA(x,ISREF(x)))', '=MAP(B1:B3,LAMBDA(x,ROW(x)))',
            '=MAP(B3,LAMBDA(x,ISREF(x)))', '=MAP(C1:C3,LAMBDA(x,SUM(x)))', '=MAP(C1:C3,LAMBDA(x,x))',
            '=MAP(B1:B3,LAMBDA(x,B1:B2))', '=MAP(B1:B3,LAMBDA(x,INDEX(B1:B3,1)))',
            '=MAP(B1:B3,C1:C2,LAMBDA(x,y,x&y))', '=MAP(B1:C3,B1:B3,LAMBDA(x,y,x+y))', '=MAP(B1:B2,{1,2,3},LAMBDA(x,y,x&y))',
            '=MAP({1,2,3},B1:B2,LAMBDA(x,y,x&y))', '=MAP(B1:B3,5,LAMBDA(x,y,x+y))', '=MAP(5,B1:B3,LAMBDA(x,y,x+y))',
            '=MAP({1;2},{1,2,3},LAMBDA(x,y,x&y))', '=MAP(B1:C1,B1:B3,LAMBDA(x,y,x&y))', '=MAP(B1:B3,C1:C2,LAMBDA(x,y,IFERROR(y,"pad")))',
            '=MAP(B1:B3,C1:C2,LAMBDA(x,y,ISNA(y)))', '=MAP(B1:C1,B1:B3,LAMBDA(x,y,"k"))',
            '=SCAN(0,B1:B3,LAMBDA(a,v,ROW(v)))', '=SCAN(,B1:B3,LAMBDA(a,v,ISREF(a)))', '=SCAN(,B1:B3,LAMBDA(a,v,ROW(a)))',
            '=SCAN(,B1:B3,LAMBDA(a,v,a))', '=SCAN(,{5,6},LAMBDA(a,v,a+v))', '=SCAN(,C1:C3,LAMBDA(a,v,TYPE(a)))',
            '=SCAN(,C1:C3,LAMBDA(a,v,ISREF(v)))', '=SCAN(,D1:D2,LAMBDA(a,v,ISTEXT(a)))', '=SCAN(,D1:D2,LAMBDA(a,v,MAX(a)))',
            '=SCAN(,D1:D2,LAMBDA(a,v,MAX(a,v)))', '=SCAN(,{"a","b"},LAMBDA(a,v,MAX(a,v)))', '=SCAN(0,B1:B3,LAMBDA(a,v,ISREF(a)))',
            '=SCAN(B1,B1:B3,LAMBDA(a,v,ISREF(a)))', '=SCAN(0,C1:C3,LAMBDA(a,v,v))', '=SCAN(,B1:B1,LAMBDA(a,v,a+v))',
            '=REDUCE(0,B1:B3,LAMBDA(a,v,ISREF(a)))', '=REDUCE(,B1:B3,LAMBDA(a,v,ROW(a)))', '=REDUCE(,B1:B2,LAMBDA(a,v,ISREF(a)))',
            '=REDUCE(,B1:B2,LAMBDA(a,v,ROW(a)))', '=REDUCE(,C1:C2,LAMBDA(a,v,ISREF(a)))', '=REDUCE(,D1:D2,LAMBDA(a,v,MAX(a,v)))',
            '=REDUCE(,D1:D2,LAMBDA(a,v,ISTEXT(a)))', '=REDUCE(,D1:D2,LAMBDA(a,v,MAX(a)))', '=REDUCE(,D1:D2,LAMBDA(a,v,MAX(v)))',
            '=REDUCE(,D1:D2,LAMBDA(a,v,TYPE(a)))', '=REDUCE(,D1:D2,LAMBDA(a,v,a))', '=REDUCE(,D1:D2,LAMBDA(a,v,ISBLANK(a)))',
            '=REDUCE(,{"a","b"},LAMBDA(a,v,MAX(a,v)))', '=REDUCE(,C1:C2,LAMBDA(a,v,MAX(a,v)))', '=REDUCE(0,C1:C3,LAMBDA(a,v,v))',
            '=REDUCE(,{5,6},LAMBDA(a,v,a))', '=REDUCE(,B1,LAMBDA(a,v,a+v))', '=REDUCE(0,C1:C3,LAMBDA(a,v,ISREF(a)))',
            '=REDUCE(0,B1:B3,LAMBDA(a,v,v))+0', '=ISREF(REDUCE(0,B1:B3,LAMBDA(a,v,v)))',
            '=REDUCE(0,B1:C2,LAMBDA(a,v,a+ROW(v)*10+COLUMN(v)))',
            '=BYROW(B1:C2,LAMBDA(r,ISREF(r)))', '=BYCOL(B1:C3,LAMBDA(c,ISREF(c)))', '=BYCOL(B1:C3,LAMBDA(c,ROW(c)))',
            '=BYROW(B1:C3,LAMBDA(r,ROWS(r)&COLUMNS(r)))', '=BYROW({1,2;3,4},LAMBDA(r,ISREF(r)))', '=BYROW(B1:C3,LAMBDA(r,ROW(r)))',
            '=BYCOL(B1:C3,LAMBDA(c,COLUMN(c)))', '=MAKEARRAY(2,2,LAMBDA(r,c,ISREF(r)))',
            '=MAP(B1:B3,"x",LAMBDA(x,y,x&y))', '=MAP({1,2},5,LAMBDA(x,y,x+y))', '=MAP(B1:B3,{5},LAMBDA(x,y,x+y))',
            '=MAP(B1:B3,B1,LAMBDA(x,y,x+y))', '=MAP(5,6,LAMBDA(x,y,x+y))', '=ISREF(REDUCE(0,B1:B3,LAMBDA(a,v,B2)))',
            '=ISREF(REDUCE(B1,B1:B3,LAMBDA(a,v,a)))', '=ROWS(REDUCE(0,B1:B3,LAMBDA(a,v,B1:B2)))']:
        f = formula[1:formula.index('(')]
        fam.case(f'{f.lower()}-{len(fam.cases):03}', f, grid, 'Sheet1!H10', formula)
    return fam


# The en-ZA number, currency, date and time text of locale-cases.py (dc-7ofm), read here by
# operators instead of functions (dc-3k5n, en-ZA part).
COERCION_TEXTS = [
    '1 234', '1 234.5', '1\u00a0234', '1,234', '1.234,5', 'R1 234.50', '-R12', '(R12)', 'R 12', '12%', '1.5E3',
    '(12)', '+12', '12-', '.5', '5.', '1 2', '1e', 'TRUE', ' 12 ', '2024/03/15', '2024-03-15', '15/03/2024',
    '03/15/2024', '24/03/15', '15 March 2024', '15-Mar-2024', 'Mar 15, 2024', 'March 2024', '15/3', '3/15', '15 Mar',
    '1/2', '10:30', '10:30 PM', '22:30:00', '2024/03/15 10:30', '25:00', '1 1/2', '0 3/4', '15.03.2024', '', 'abc',
]


def operator_coercion():
    """Text read as a number by the arithmetic operators, unary minus and comparison, both as a
    literal and through a cell holding the text."""
    fam = Family()
    cells = {f'A{i + 1}': t(x) for i, x in enumerate(COERCION_TEXTS) if x}
    sheet = fam.layout('texts', [{'name': 'Sheet1', 'cells': cells}])
    for i, x in enumerate(COERCION_TEXTS):
        literal = '"' + x.replace('"', '""') + '"'
        forms = [f'={literal}+0', f'={literal}*1', f'=-{literal}', f'={literal}^1', f'={literal}=1234', f'={literal}&1']
        if x:
            forms += [f'=A{i + 1}+0', f'=A{i + 1}-0', f'=A{i + 1}/1']
        for j, formula in enumerate(forms):
            fam.case(f'coerce-{i:02}-{j}', 'OPERATOR', sheet, 'Sheet1!D1', formula)
    return fam


# Arrays of references (dc-b1l): OFFSET and INDIRECT lifted over an array give one reference per
# element; each consumer below is asked how it takes them (whole, once per reference, or its first).
REFERENCE_ARRAYS = ['OFFSET(A1,{0;1;2},0,2)', 'OFFSET(A1,{0;1;2},0)', 'INDIRECT({"A1:A2";"A3:A4"})',
                    'OFFSET(A1,0,{0,2})', 'OFFSET(A1,{0;2},{0,2},2,1)', 'OFFSET(A1,{0;0;1},0)', 'OFFSET(B1,{0;1},0)']
REFERENCE_ARRAY_CONSUMERS = [
    'SUM(X)', 'AVERAGE(X)', 'MAX(X)', 'MIN(X)', 'COUNT(X)', 'COUNTA(X)', 'COUNTBLANK(X)', 'PRODUCT(X)', 'SUMSQ(X)',
    'AVERAGEA(X)', 'MAXA(X)', 'MINA(X)', 'MEDIAN(X)', 'MODE(X)', 'MODE.SNGL(X)', 'MODE.MULT(X)', 'STDEV(X)',
    'STDEV.S(X)', 'STDEV.P(X)', 'STDEVA(X)', 'VAR(X)', 'VAR.S(X)', 'VAR.P(X)', 'VARPA(X)', 'DEVSQ(X)', 'AVEDEV(X)',
    'GEOMEAN(X)', 'HARMEAN(X)', 'KURT(X)', 'SKEW(X)', 'LARGE(X,1)', 'SMALL(X,1)', 'PERCENTILE(X,0.5)',
    'PERCENTILE.INC(X,0.5)', 'QUARTILE(X,1)', 'RANK(1,X)', 'RANK.EQ(2,X)', 'RANK.AVG(2,X)', 'PERCENTRANK(X,2)',
    'TRIMMEAN(X,0)', 'AGGREGATE(9,0,X)', 'AGGREGATE(4,0,X)', 'AGGREGATE(2,4,X)', 'AGGREGATE(14,0,X,1)',
    'AGGREGATE(9,0,X,X)', 'SUMPRODUCT(X)', 'SUMPRODUCT(--X)', 'CONCAT(X)', 'TEXTJOIN(",",TRUE,X)', 'NPV(0.1,X)',
    'SUM(X,1)', 'SUMX2PY2(X,X)', 'CORREL(X,X)', 'COVAR(X,X)', 'SLOPE(X,X)', 'FREQUENCY(X,{2})',
    'INDEX(X,1)', 'INDEX(X,1,1,2)', 'MATCH(2,X,0)', 'VLOOKUP(2,X,1,0)', 'HLOOKUP(2,X,1,0)', 'LOOKUP(2,X)',
    'XLOOKUP(2,X,X)', 'XMATCH(2,X)', 'ROWS(X)', 'COLUMNS(X)', 'AREAS(X)', 'ROW(X)', 'COLUMN(X)', 'ISREF(X)',
    'ISFORMULA(X)', 'FORMULATEXT(X)', 'SHEET(X)', 'SHEETS(X)', 'CELL("address",X)', 'CELL("contents",X)', 'N(X)',
    'T(X)', 'TYPE(X)', 'ISBLANK(X)', 'ISNUMBER(X)', 'ISTEXT(X)', 'ISERROR(X)', 'LEN(X)', 'ABS(X)', 'ROUND(X,0)',
    'X+0', '-X', 'X&""', 'X=1', 'IF(TRUE,X)', 'IFERROR(X,0)', 'CHOOSE(1,X)', 'LET(r,X,r)', 'LET(r,X,SUM(r))',
    'SUMIF(X,">1")', 'COUNTIF(X,">1")', 'AVERAGEIF(X,">1")', 'SUMIFS(X,X,">1")', 'COUNTIFS(X,">1")',
    'AVERAGEIFS(X,X,">1")', 'MAXIFS(X,X,">1")', 'MINIFS(X,X,">1")', 'SUBTOTAL(9,X)', 'SUBTOTAL(3,X)',
    'SORT(X)', 'UNIQUE(X)', 'TAKE(X,1)', 'DROP(X,1)', 'VSTACK(X)', 'HSTACK(X)', 'TOCOL(X)', 'TOROW(X)',
    'TRANSPOSE(X)', 'CHOOSECOLS(X,1)', 'CHOOSEROWS(X,1)', 'EXPAND(X,3)', 'FILTER(X,TRUE)', 'SORTBY(X,X)',
    'MAP(X,LAMBDA(v,v))', 'BYROW(X,LAMBDA(r,SUM(r)))', 'REDUCE(0,X,LAMBDA(a,v,a+v))', 'MMULT(X,1)',
    'DSUM(X,1,X)', 'ISNA(MATCH(1,X,0))', 'COUNTA(X,X)', 'ARRAYTOTEXT(X)', 'AND(X)', 'OR(X)', 'XOR(X)',
    'INDEX(X,1,1)', 'INDEX(X,2,1)', 'INDEX(X,1,2)', 'INDEX(X,0,1)', 'INDEX(X,2)', 'INDEX(X,1,1,1)', 'INDEX(X,0)',
    'UNIQUE(X,TRUE)', 'UNIQUE(X,,TRUE)', 'SCAN(0,X,LAMBDA(a,v,a+v))', 'BYCOL(X,LAMBDA(c,SUM(c)))', 'ISERR(X)',
    'ERROR.TYPE(X)', 'IFNA(X,0)', 'ISNA(X)', 'NOT(X)', 'AND(X,TRUE)', 'OR(X,FALSE)', 'SUMX2MY2(X,X)',
    'PEARSON(X,X)', 'RSQ(X,X)', 'INTERCEPT(X,X)', 'SUMX2PY2(X,{1;2;3})', 'CORREL(X,{1;2;3})', 'ARRAYTOTEXT(X,1)',
    'MAKEARRAY(2,1,LAMBDA(r,c,INDEX(X,r)))', 'ISLOGICAL(X)', 'ISREF(INDEX(X,1,1))', 'TEXT(X,"0")', 'VALUE(X)',
]
# Plain arrays beside the reference arrays: how the same consumers read text, errors and logicals.
REFERENCE_ARRAY_CONTROLS = [
    'SUMX2PY2({"a";"b";"c"},{"a";"b";"c"})', 'CORREL({"a";"b";"c"},{"a";"b";"c"})', 'SLOPE({"a";"b"},{"a";"b"})',
    'SUMX2PY2({1;2},{1;2})', 'AND({"a"})', 'AND({"a";"b"})', 'OR({"a"})', 'AND(A1:A2&"")', 'AND({1;#VALUE!})',
    'ARRAYTOTEXT({#VALUE!;#VALUE!})', 'SUMX2PY2({#VALUE!;1},{1;1})', 'CORREL({#VALUE!;1;2},{1;1;2})',
    'UNIQUE({#VALUE!;#VALUE!})',
]


def reference_arrays():
    """Each consumer of an array of references (dc-b1l), over five arrays of references."""
    fam = Family()
    cells = {}
    for i in range(6):
        cells[f'A{i + 1}'] = n(i + 1)
        cells[f'C{i + 1}'] = n(10 * (i + 1))
    cells.update({'B1': t('x'), 'B2': 'logical:true', 'B4': n(7), 'B5': 'error:NA', 'B6': '=A6*2'})
    sheet = fam.layout('grid', [{'name': 'Sheet1', 'cells': cells}, {'name': 'Data', 'cells': {'A1': n(9)}}])
    for i, ref in enumerate(REFERENCE_ARRAYS):
        for j, consumer in enumerate(REFERENCE_ARRAY_CONSUMERS):
            call = re.match(r'([A-Z][A-Z0-9.]*)\(', consumer)
            fam.case(f'refarray-{i}-{j:03}', call.group(1) if call else 'OPERATOR', sheet, 'Sheet1!H1',
                     '=' + re.sub(r'\bX\b', lambda _: ref, consumer))
    for j, control in enumerate(REFERENCE_ARRAY_CONTROLS):
        fam.case(f'refarray-control-{j:02}', re.match(r'([A-Z][A-Z0-9.]*)\(', control).group(1), sheet, 'Sheet1!H1',
                 '=' + control)
    return fam


# 3-D references at the argument positions whose treatment the catalogue does not hold (dc-b1l): a
# 3-D reference spreads across its sheets (Take), makes the call #VALUE!, or is #REF! as that argument.
# A pair of cases per position where a branch or a match decides whether the argument is read.
THREE_D_CASES = [
    'IF(TRUE,1,X)', 'IF(FALSE,1,X)', 'ECMA.CEILING(X,1)', 'ECMA.CEILING(2.5,X)',
    'COUNTIF(A1:A3,X)', 'SUMIF(A1:A3,X)', 'SUMIF(A1:A3,X,B1:B3)', 'COUNTIFS(A1:A3,X)',
    'COUNTIFS(A1:A3,">0",X,">0")', 'COUNTIFS(A1:A3,">0",A1:A3,X)', 'COUNTIFS(A1:A3,">5",A1:A3,X)',
    'SUMIFS(B1:B3,A1:A3,X)', 'SUMIFS(B1:B3,A1:A3,">0",X,">0")', 'SUMIFS(B1:B3,A1:A3,">0",A1:A3,X)',
    'AVERAGEIFS(B1:B3,A1:A3,">0",X,">0")', 'AVERAGEIFS(B1:B3,A1:A3,">0",A1:A3,X)', 'MAXIFS(B1:B3,A1:A3,X)',
    'MAXIFS(B1:B3,A1:A3,">0",X,">0")', 'MINIFS(B1:B3,A1:A3,X)', 'MINIFS(B1:B3,A1:A3,">0",A1:A3,X)',
    'IFERROR(1,X)', 'IFERROR(NA(),X)', 'IFNA(1,X)', 'IFNA(NA(),X)', 'XMATCH(2,A1:A3,0,X)',
    'XLOOKUP(2,A1:A3,B1:B3,X)', 'XLOOKUP(9,A1:A3,B1:B3,X)', 'XLOOKUP(2,A1:A3,B1:B3,"",X)',
    'XLOOKUP(2,A1:A3,B1:B3,"",0,X)', 'CHOOSE(1,1,X)', 'CHOOSE(2,1,X)', 'TRIMRANGE(X)', 'RANDBETWEEN(X,X)',
    'IFS(TRUE,1,X,2)', 'IFS(FALSE,1,X,2)', 'IFS(FALSE,1,TRUE,X)', 'IFS(TRUE,1,TRUE,X)', 'SWITCH(1,1,2,X)',
    'SWITCH(1,2,3,X)', 'SWITCH(1,2,3,X,4)', 'SWITCH(1,1,2,2,X)', 'ENCODEURL(X)', 'FILTERXML(X,"//a")',
    'JIS(X)', 'WORKDAY.INTL(45000,1,1,X)', 'NETWORKDAYS.INTL(45000,45010,1,X)', 'CELL("address",X)',
    'CELL("contents",X)', 'CELL(X)', 'INFO(X)', 'DSUM(A5:B8,X,D5:D6)', 'DSUM(A5:B8,1,X)', 'DCOUNT(A5:B8,X,D5:D6)',
    'DAVERAGE(A5:B8,1,X)', 'DGET(A5:B8,X,D5:D6)', 'DMAX(A5:B8,1,X)', 'ACCRINTM(X,45100,0.1,1000)',
    'ACCRINTM(45000,X,0.1,1000)', 'ACCRINTM(45000,45100,X,1000)', 'ACCRINTM(45000,45100,0.1,1000,X)',
    'FILTER(A1:A3,A1:A3>5,X)', 'FILTER(A1:A3,A1:A3>0,X)', 'TOCOL(A1:B3,0,X)', 'TOROW(A1:B3,0,X)',
    'WRAPROWS(A1:A3,2,X)', 'WRAPCOLS(A1:A3,2,X)', 'PERCENTOF(X,1)', 'PERCENTOF(1,X)', 'LET(x,X,1)', 'LET(x,1,X)',
    'LAMBDA(x,x)(X)', 'X+1', '1+X', 'X-1', 'X*2', 'X/2', 'X^2', 'X&"a"', 'X=1', 'X<>1', 'X<1', 'X<=1', 'X>1',
    'X>=1', '-X', '+X', 'X%', 'GROUPBY(X,X,SUM)', 'HYPERLINK(X)',
    # Controls at positions the catalogue holds (dc-10y, measured through Range.Formula).
    'ABS(X)', 'ROUND(X,0)', 'ROUND(1.5,X)', 'NORM.DIST(X,0,1,TRUE)', 'SUM(X)', 'SUM(X,1)', 'COUNT(X)', 'INDEX(X,1)',
    'ROWS(X)', 'ISREF(X)', 'LEN(X)', 'DATE(X,1,1)', 'MAX(X)', 'AVERAGE(X)', 'CONCAT(X)', 'RANK(1,X)', 'ISNUMBER(X)',
    'N(X)', 'T(X)', 'TYPE(X)', 'SIN(X)', 'POWER(X,2)', 'MOD(X,2)', 'TEXT(X,"0")', 'VALUE(X)', 'IF(X,1,2)',
    'SUMPRODUCT(X)', 'MATCH(1,X,0)', 'VLOOKUP(1,X,1,0)',
    # Criteria: E1 holds #VALUE! and E2 #REF!, so a criterion read as either error matches one.
    'COUNTIF(E1:E2,X)', 'SUMIF(E1:E2,X,B1:B2)', 'COUNTIFS(E1:E2,X)', 'SUMIFS(B1:B2,E1:E2,X)',
    'COUNTIFS(A1:A2,">0",E1:E2,X)', 'MAXIFS(B1:B2,E1:E2,X)', 'MINIFS(B1:B2,A1:A2,">0",E1:E2,X)',
    'AVERAGEIFS(B1:B2,A1:A2,">0",E1:E2,X)', 'SUMIFS(B1:B2,A1:A2,">0",E1:E2,X)', 'COUNTIFS(E1:E2,X,E1:E2,X)',
    'XLOOKUP(9,A1:A3,B1:B3,X)&""', 'ISERROR(IF(FALSE,1,X))', 'ERROR.TYPE(IF(FALSE,1,X))',
    'ERROR.TYPE(IFERROR(NA(),X))', 'ERROR.TYPE(CHOOSE(2,1,X))', 'ERROR.TYPE(IFS(FALSE,1,TRUE,X))',
    'FILTERXML("<a>1</a>",X)', 'HYPERLINK("x",X)', 'GROUPBY(A1:A3,B1:B3,SUM,X)',
]


def three_d():
    """A 3-D reference (Ja:Ma!A1, the sheets holding 1, 2 and 3 there) at each position whose
    treatment is not measured (dc-b1l)."""
    fam = Family()
    calc = {'A1': n(1), 'A2': n(2), 'A3': n(3), 'B1': n(10), 'B2': n(20), 'B3': n(30),
            'A5': t('h'), 'B5': t('v'), 'A6': n(1), 'B6': n(5), 'A7': n(2), 'B7': n(6), 'A8': n(3), 'B8': n(7),
            'D5': t('h'), 'D6': t('>1'), 'E1': 'error:Value', 'E2': 'error:Ref'}
    # Sheet names that are no cell address (S1:S3!A1 would read as S1 to S3!A1).
    sheets = [{'name': 'Calc', 'cells': calc}] + [{'name': name, 'cells': {'A1': n(i), 'B1': n(10 * i)}}
                                                   for i, name in ((1, 'Ja'), (2, 'Fe'), (3, 'Ma'))]
    layout = fam.layout('sheets', sheets)
    for j, case in enumerate(THREE_D_CASES):
        call = re.match(r'([A-Z][A-Z0-9.]*)\(', case)
        fam.case(f'threed-{j:03}', call.group(1) if call else 'OPERATOR', layout, 'Calc!H1',
                 '=' + re.sub(r'\bX\b', lambda _: 'Ja:Ma!A1', case))
    return fam


# WEEKNUM with a return type outside 1-21 reads the week start the pass's last WEEKNUM or
# ISOWEEKNUM left (dc-amhpx): one formula alone with WEEKNUM in its workbook, the calls in it.
WEEKNUM_PASS = [
    'WEEKNUM(45000,30)', 'WEEKNUM(45000,0.5)', 'WEEKNUM(45000,1)&"|"&WEEKNUM(45000,30)',
    'WEEKNUM(45000,2)&"|"&WEEKNUM(45000,30)', 'WEEKNUM(45000,30)&"|"&WEEKNUM(45000,2)',
    'ISOWEEKNUM(45000)&"|"&WEEKNUM(45000,30)', 'WEEKNUM(45000,21)&"|"&WEEKNUM(45002,30)',
    'WEEKNUM(45000,13)&"|"&WEEKNUM(45000,30)', 'WEEKNUM(45000,2)&"|"&WEEKNUM(45000,3)&"|"&WEEKNUM(45000,30)',
    'WEEKNUM(45000,{2,30})', 'WEEKNUM(45000,{30,2})', 'WEEKNUM(45000,{2;30})', 'WEEKNUM({45000,45002},{17,30})',
    'IF(TRUE,WEEKNUM(45000,2),0)&"|"&WEEKNUM(45000,30)', 'WEEKNUM(45000,30)+WEEKNUM(45000,30)',
    'LET(a,WEEKNUM(45000,11),b,WEEKNUM(45002,99),a&"|"&b)', 'SUM(WEEKNUM(45000,2),WEEKNUM(45001,-1))',
    'WEEKNUM(45000,2)&"|"&ABS(1)&"|"&WEEKNUM(45000,30)', 'WEEKNUM(45000,17)&"|"&WEEKNUM(45001,22)',
    'WEEKNUM("x",2)&"|"&WEEKNUM(45000,30)', 'IFERROR(WEEKNUM(-1,2),0)&"|"&WEEKNUM(45000,30)',
]


def weeknum_pass():
    """Each case one formula, alone with WEEKNUM in its workbook (dc-amhpx)."""
    fam = Family()
    sheet = fam.layout('empty', [{'name': 'Sheet1', 'cells': {}}])
    for j, formula in enumerate(WEEKNUM_PASS):
        fam.case(f'weeknum-pass-{j:02}', 'WEEKNUM', sheet, 'Sheet1!B2', '=' + formula)
    return fam


FAMILIES = {'reference-info': reference_info, 'subtotal-hidden': subtotal_hidden,
            'spill-placement': spill_placement, 'ifs': ifs_family, 'lambda-helpers': lambda_helpers,
            'operator-coercion': operator_coercion, 'lambda-groupby': lambda_groupby,
            'lambda-references': lambda_references, 'reference-arrays': reference_arrays,
            'three-d': three_d, 'weeknum-pass': weeknum_pass}


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--family')
    o = p.parse_args()
    OUT.mkdir(parents=True, exist_ok=True)
    for name, make in FAMILIES.items():
        if o.family and name != o.family:
            continue
        fam = make()
        ids = [c['id'] for c in fam.cases]
        assert len(ids) == len(set(ids)), 'duplicate case id'
        # One case per line keeps the file small and its diffs readable.
        lines = [json.dumps(c, ensure_ascii=True, separators=(',', ':')) for c in fam.cases]
        head = {'format': 'dnacalc-cell-context/1', 'family': name, 'layouts': fam.layouts}
        text = json.dumps(head, indent=1, ensure_ascii=True)[:-2] + ',\n "cases": [\n' + ',\n'.join(lines) + '\n ]\n}\n'
        json.loads(text)
        path = OUT / f'{name}.json'
        path.write_text(text)
        print(name, len(fam.cases), 'cases ->', path.relative_to(ROOT))


if __name__ == '__main__':
    main()
