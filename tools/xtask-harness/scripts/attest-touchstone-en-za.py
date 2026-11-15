"""Builds migration/evidence/dc-7ofm-touchstone-en-za.json: the recorded facts that Touchstone
environment 29bedb2b6722 is Excel 16.0.20527 under the en-ZA user culture.
Usage: python3 -I attest-en-za.py <CalcFunc-Evidence checkout> <DnaCalc checkout>"""
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
        'The capture worker on the same Excel (16.0.20527.20088, same CPU and OS build) recorded the user culture '
        'and Application.International readings in catalogue_capture '
        '(crates/calc-func/tests/captures/cfunc_m3_2/oracle.capture.json).',
        'Every DOLLAR answer of this environment that holds a digit starts with the Rand sign R (the en-ZA '
        'currency), after a minus for negatives; grouped answers use U+0020, the International thousands '
        'separator, although Application.ThousandsSeparator (calculation_settings) reported U+00A0.',
    ],
    'catalogue_capture': {k: s[k] for k in keep},
    'evidence_repository': 'CalcFunc-Evidence',
    'evidence_commit': commit,
    'env_id': env_id,
    'service': 'touchstone/1',
    'profile': {'locale': 'en-ZA', 'decimal_separator': '.', 'thousands_separator': ' '},
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
