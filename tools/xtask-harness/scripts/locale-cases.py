#!/usr/bin/env python3
"""en-ZA locale cases for the locale-sensitive functions (dc-7ofm), asked through the same
capture path as bank-coverage.py and written in its WitnessSet form.

  python3 tools/xtask-harness/scripts/locale-cases.py --out <empty dir> [--only TEXT]

Writes answers-<function>-locale.json per function: TEXT date, time, currency, fraction,
scientific and number codes; VALUE, DATEVALUE and TIMEVALUE number, currency, date and time
text; NUMBERVALUE separator combinations; DOLLAR and FIXED rounding-to-zero signs. Rows whose
input Excel read back differently are rejected, as in bank-coverage.py. The environment is
recorded as the service reports it (env_id); the culture is attested separately
(migration/evidence/dc-7ofm-touchstone-en-za.json).
"""
import argparse, importlib.util, json, pathlib, struct

spec = importlib.util.spec_from_file_location('bank_coverage', pathlib.Path(__file__).with_name('bank-coverage.py'))
bank = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bank)
number, text, Client = bank.number, bank.text, bank.Client

SEED = 'locale-en-za-20261115'

VALUES = [0, 0.5, 1, 1.5, 59, 60, 61, 366, 36526, 45000.75, 45292.123456789, 45366.999999,
          2958465.99999, -1, -0.25, 1234.5678, -1234.5678, 0.000123, 1e10, 12345678901.5,
          1 / 3, 1 / 7, 0.9999999]

CODES = [
    # dates
    'yyyy/mm/dd', 'dd/mm/yyyy', 'd mmm yyyy', 'dddd, d mmmm yyyy', 'mmm-yy', 'm/d/yy', 'yy', 'ddd',
    'mmmmm', 'yyyy-mm-dd', 'd/m', 'dd mmm', 'mmmm yyyy', '[$-1C09]d mmmm yyyy', '[$-409]mmmm d, yyyy',
    '[$-en-ZA]dddd', 'yyyy/mm/dd hh:mm', 'bbbb', 'bb/mm/dd', 'e', 'ggg e',
    # times
    'hh:mm', 'h:mm AM/PM', 'hh:mm:ss.000', '[h]:mm:ss', '[mm]:ss', 'h AM/PM', 'mm:ss.0', 'A/P h',
    'h:mm:ss am/pm', '[ss]', 'h:mm a/p', 'hh"h"mm',
    # currency
    'R # ##0.00', 'R #,##0.00', '$#,##0.00', '[$R-1C09] # ##0.00', '[$€-x-euro2] # ##0.00',
    '# ##0.00 [$USD]', '"R"0.00', 'R0.00;-R0.00', '\\R0', '[$-1C09]R # ##0.00;-R # ##0.00',
    'R # ##0.00;(R # ##0.00)', '_-R* # ##0.00_-;-R* # ##0.00_-;_-R* "-"??_-;_-@_-',
    # fractions
    '# ?/?', '# ??/??', '?/8', '# ?/16', '0 ?/?', '??/??', '# ???/???', '0/4', '# ?/100',
    # scientific
    '0.00E+00', '0.0E-0', '##0.0E+0', '0E+0', '0.000e+00', '#.##E+00', '0.00E+00;-0.00E+00', '0.0E+000',
    # numbers
    '# ##0.00', '#,##0.00', '0.0%', '0.00%%', '# ##0 ', '0.00_);(0.00)', '[>=1000]# ##0;0.00',
    '[Red]0.00;[Blue]-0.00', 'General', '@', '0.00" ZAR"', '000-000', '# ###', '0.###', '#.000',
]

TEXTS = [
    '1 234', '1 234.5', '1 234', '1 234.5', '1,234', '1,234.5', '1.234,5', '12 345 678', '12 34',
    '1  234', 'R1 234.50', 'R1234', '-R12', '(R12)', 'R 12', 'R-12', '12R', '$12', '12%', '-12%', '1.5E3',
    '(12)', '+12', '12-', '.5', '5.', '1 2', '1e', 'TRUE', ' 12 ',
    '2024/03/15', '2024-03-15', '2024/3/5', '15/03/2024', '03/15/2024', '24/03/15', '15 March 2024',
    '15-Mar-2024', 'Mar 15, 2024', 'March 2024', '2024/3', '15/3', '3/15', '15 Mar', '1 Jan 1900',
    '1900/02/29', '29/02/1900', '2024/02/30', 'Sunday, 15 March 2024', '15.03.2024', '2024.03.15',
    '10:30', '10:30 PM', '10:30:15.5', '22:30:00', '2024/03/15 10:30', '0:00', '25:00', '10:61',
    '12:00 AM', '12:00 a', '12a', '12 p', '10:30 nm', '10:30 vm', '1:2:3', '24:00:00', '9:5',
    '1/2', '1 1/2', '-1 1/2', '0 3/4', '2024/03/15 12:00:00 PM', '15 Mrt 2024', '15 Maart 2024',
]

NUMBERVALUE_TEXTS = ['1 234.5', '1.234,5', '1,234.5', '1 234,5', '12%', '12%%', ' 1 2 ', '1e3', '(12)',
                     '-12-', 'R12', '1.2.3', '1,,2', ',5', '5,', '1 2,3', '', '  ', '1.5E+2', '+1,5']
NUMBERVALUE_SEPARATORS = [None, (',', '.'), ('.', ','), ('.', ' '), (',', ' '), (',', ' '), ('.', "'")]

TINY = [-1e-300, -1e-200, -1e-100, -2.4e-58, -1e-50, -1e-20, -1e-10, -0.4, -1, -153.8, 1e-100]
PLACES = [-1e100, -1e10, -1000, -400, -309, -308, -300, -100, -20, -10, -1, -0.5, 0, 0.5]


def rows_for(name):
    if name == 'TEXT':
        return [[number(v), text(c)] for c in CODES for v in VALUES]
    if name in ('VALUE', 'DATEVALUE', 'TIMEVALUE'):
        return [[text(t)] for t in TEXTS]
    if name == 'NUMBERVALUE':
        rows = []
        for t in NUMBERVALUE_TEXTS:
            for sep in NUMBERVALUE_SEPARATORS:
                rows.append([text(t)] + ([] if sep is None else [text(sep[0]), text(sep[1])]))
        return rows
    if name == 'DOLLAR':
        return [[number(x), number(p)] for x in TINY for p in PLACES]
    if name == 'FIXED':
        return [[number(x), number(p), bank.boolean(flag)] for x in TINY for p in PLACES for flag in (False, True)]
    raise ValueError(name)


FUNCTIONS = ['TEXT', 'VALUE', 'DATEVALUE', 'TIMEVALUE', 'NUMBERVALUE', 'DOLLAR', 'FIXED']


def capture(client, name, out):
    rows = rows_for(name)
    reply = bank.ask_checked(client, name, rows)
    accepted, rejected = [], {}
    for i, (requested, observed, readback) in enumerate(zip(rows, reply['rows'], reply['inputs'])):
        result = bank.drift.answer(observed) if observed else None
        if bank.drift.without_mode(requested) != bank.drift.without_mode(readback):
            why = 'changed input'
        elif result is None:
            why = 'unread or unsupported answer: ' + str(observed.get('type'))
        else:
            accepted.append({'id': f'locale-{name}-{i:04}', 'args': [bank.witness_arg(a) for a in readback],
                             'expected_bits': result})
            continue
        rejected[why] = rejected.get(why, 0) + 1
    env = {'excel_build': reply.get('excel_build'), 'platform': 'Windows', 'excel_input_plumbing': 'cell_value2_bulk'}
    doc = {'function': name, 'witnesses': accepted,
           'capture_provenance': {'environment': env, 'calculation_settings': reply.get('calculation_settings'),
                                  'env_id': reply.get('env_id'), 'timestamp': reply.get('timestamp'),
                                  'service': 'touchstone/1'},
           'capture_summary': {'requested': len(rows), 'accepted': len(accepted), 'rejected': rejected, 'seed': SEED}}
    path = out / ('answers-' + name.lower() + '-locale.json')
    path.write_text(json.dumps(doc, ensure_ascii=True, separators=(',', ':')) + '\n')
    return f'{name}: accepted {len(accepted)}/{len(rows)} rejected {rejected} env {reply.get("env_id")}'


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--out', type=pathlib.Path, required=True)
    p.add_argument('--only')
    o = p.parse_args()
    o.out.mkdir(parents=True, exist_ok=True)
    client = Client(transport='socket', timeout=900)
    for name in FUNCTIONS:
        if o.only and name != o.only:
            continue
        print(capture(client, name, o.out), flush=True)


if __name__ == '__main__':
    main()
