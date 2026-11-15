"""Builds migration/evidence/dc-7ofm-touchstone-en-za.json: the recorded facts that Touchstone
environment 29bedb2b6722 is Excel 16.0.20527 under the en-ZA user culture, with no add-in
file open (dc-ymjf.2.65).
Usage: python3 -I attest-touchstone-en-za.py <CalcFunc-Evidence checkout> <DnaCalc checkout> [<environment record json>]"""
import json, glob, hashlib, re, sys, subprocess

ev, repo = sys.argv[1], sys.argv[2]
env_id = '29bedb2b6722'
commit = subprocess.check_output(['git', '-C', ev, 'rev-parse', 'HEAD'], text=True).strip()
files, dollar = [], []
for f in sorted(glob.glob(ev + '/readings/excel-20527/*.json')):
    raw = open(f, 'rb').read()
    d = json.loads(raw)
    p = d.get('capture_provenance', {})
    if p.get('env_id') != env_id or p.get('service') != 'touchstone/1':
        continue
    rel = f[len(ev) + 1:]
    files.append({'path': rel, 'function': d['function'], 'sha256': hashlib.sha256(raw).hexdigest()})
    if d['function'] == 'DOLLAR':
        for w in d['witnesses']:
            e = w['expected_bits']
            if e.startswith('text:') and re.search(r'\d', e):
                dollar.append(e[5:])
cat = json.load(open(repo + '/crates/calc-func/tests/captures/cfunc_m3_2/oracle.capture.json', encoding='utf-8'))
s = cat['environment']['settings']
keep = ['excel_build_number', 'os_version', 'cpu', 'user_culture', 'install_language_id', 'country_setting',
        'decimal_separator', 'thousands_separator', 'date_order', 'use_system_separators']
rand = [t for t in dollar if re.match(r'^-?R\d', t)]
grouped = [t for t in dollar if re.search(r'\d \d', t)]
out = {
    'bead': 'dc-7ofm',
    'claim': 'Touchstone environment %s is Excel 16.0.20527 on GOVERT-OMEN under the Windows user culture en-ZA, '
             'whose Excel International separators are "." and " ".' % env_id,
    'evidence': [
        'docs/research/text-date-coercion-2026-11-12.md records env_id %s as Excel build 20527 on GOVERT-OMEN, '
        'reading DATEVALUE year-first (04/02/01 = 38018, 4/2/2001 = #VALUE!), the en-ZA short-date order.' % env_id,
        'The catalogue capture worker on the same machine and Excel build (file version 16.0.20527.20088; '
        'the Touchstone environment record gives 16.0.20527.20150, same CPU and OS build, in touchstone_environment) '
        'recorded the Windows user culture and Application.International readings in catalogue_capture '
        '(crates/calc-func/tests/captures/cfunc_m3_2/oracle.capture.json); the user culture is a Windows setting '
        'that an Excel update does not change.',
        'Every DOLLAR answer of this environment that holds a digit starts with the Rand sign R (the en-ZA '
        'currency), after a minus for negatives; grouped answers use U+0020, the International thousands '
        'separator, although Application.ThousandsSeparator (calculation_settings) reported U+00A0.',
    ],
    'catalogue_capture': {k: s[k] for k in keep},
    # The broker's record of the environment whose hash is env_id (asked 2026-11-15).
    'touchstone_environment': json.load(open(sys.argv[3], encoding='utf-8')) if len(sys.argv) > 3 else None,
    'evidence_repository': 'CalcFunc-Evidence',
    'evidence_commit': commit,
    'env_id': env_id,
    'service': 'touchstone/1',
    'profile': {'locale': 'en-ZA', 'decimal_separator': '.', 'thousands_separator': ' '},
    # dc-ymjf.2.65: no add-in file is open in this environment's sessions.
    'addins_open': [],
    # dc-ymjf.2.94: its scratch workbook holds no visible defined name and no table.
    'defined_names': [],
    'defined_names_evidence': [
        'OwnedExcel.cs Setup makes the scratch book by Workbooks.Add (a new workbook holds no defined name or '
        'table), and the fingerprint compared with the baseline after every batch counts its visible names '
        '(VisibleNames; Excel\'s hidden _xlfn.* future-function names aside), so a batch that left a name would '
        'have recycled the instance.',
    ],
    # dc-ymjf.2.84/.85: where a bulk batch places a call's argument cells.
    'cell_layout': 'touchstone/1-bulk',
    'cell_layout_evidence': [
        'tools/excel-service/engines/excel/Program.cs (the bulk path, identical in every version from 03be1c0f of '
        '2026-11-09 to 21920da5 of 2026-11-13, around the 2026-11-12 12:17-12:30 UTC captures): call i of a batch '
        'has its formula in column A of row i of sheet S and its arguments as cells from column B of that row '
        '(Formula: RC[k+1]; Rng(sheet, 1, 2, totalRows, maxCols).Value2), an error argument entered as its error '
        'formula (Formula2R1C1 "=#N/A"); OwnedExcel.cs Setup names the sheets S, A, Q in that order. The row '
        'depends on the batch and is not recorded.',
    ],
    'addins_evidence': [
        'The environment record (touchstone_environment, whose JSON env_id hashes) lists addins_installed [] and '
        'com_addins_connected [VS15ExcelAdaptor], the Visual Studio design-time adaptor, which adds no worksheet function.',
        'The worker (tools/excel-service/engines/excel/OwnedExcel.cs Setup, unchanged since fdcc4472 of '
        '2026-11-08, before the 2026-11-12 captures) refuses an Excel instance with any AddIns2 entry open or any '
        'registered XLL function, and opens no workbook besides its scratch book (sheets S, A, Q).',
    ],
    'dollar_answers_with_digits': len(dollar),
    'dollar_answers_rand_prefixed': len(rand),
    'dollar_answers_space_grouped': len(grouped),
    'dollar_answer_sample': sorted(set(grouped))[:12] + sorted(set(rand))[:12],
    'files': files,
}
with open(repo + '/migration/evidence/dc-7ofm-touchstone-en-za.json', 'w', encoding='utf-8', newline='\n') as h:
    json.dump(out, h, indent=1, ensure_ascii=False)
    h.write('\n')
print(len(files), len(dollar), len(rand), len(grouped))
