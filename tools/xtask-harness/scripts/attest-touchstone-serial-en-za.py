"""Builds migration/evidence/dc-ymjf-2-85-touchstone-serial-en-za.json: the recorded facts that the
Touchstone serial worker's environment (the one answering `order="serial"` requests, multithreaded
calculation off) is Excel 16.0.20527 under the en-ZA user culture, with no add-in file open.
It asks that environment directly, as one-call serial requests, the culture questions dc-7ofm
read from the bank: DOLLAR (currency sign and grouping) and DATEVALUE (short-date order).
Usage: python3 attest-touchstone-serial-en-za.py <DnaCalc checkout> <out json> [any [ADDIN.XLAM]]
With "any" the questions go to the ordinary (multithreaded) workers instead of the serial one; with an add-in
file named as well, to the environment variant whose Excel has it open."""
import json, sys

repo, out = sys.argv[1], sys.argv[2]
order = sys.argv[3] if len(sys.argv) > 3 else 'serial'
addins = sys.argv[4:] or None
sys.path.insert(0, repo + '/tools/excel-service')
from client import Client, number, text  # noqa: E402

QUESTIONS = [('DOLLAR', [number(-1234.5), number(2.0)]), ('DOLLAR', [number(1234567.891), number(2.0)]),
             ('DOLLAR', [number(0.5), number(0.0)]), ('DATEVALUE', [text('04/02/01')]),
             ('DATEVALUE', [text('4/2/2001')]), ('VALUE', [text('1 234.5')]), ('VALUE', [text('1,234')])]
client = Client(transport='socket', timeout=600)
sock = client._socket()
answers, envs = [], set()
for function, row in QUESTIONS:
    pending = sock.submit_rows(function, [row], priority='bulk' if order == 'any' else 'interactive', echo=True, order=order, addins=addins)
    reply = pending.result(timeout=600)
    envs.update(pending.env_ids)
    answers.append({'function': function, 'args': reply['inputs'][0], 'answer': reply['rows'][0],
                    'timestamp': reply.get('timestamp')})
if len(envs) != 1:
    raise SystemExit('several environments answered: %s' % sorted(envs))
env_id = envs.pop()
record = sock.envs.get(env_id, {}).get('env')
doc = {
    'bead': 'dc-ymjf.2.85',
    'claim': ('Touchstone environment %s (%s) is Excel 16.0.20527 on the same machine as '
              '29bedb2b6722 (migration/evidence/dc-7ofm-touchstone-en-za.json), under the Windows user culture '
              'en-ZA, whose Excel International separators are "." and " ".'
              % (env_id, ('the variant with %s open' % '+'.join(addins)) if addins else
                 'an ordinary multithreaded worker' if order == 'any' else 'the serial worker')),
    'evidence': [
        'The environment record (touchstone_environment) gives the same build, OS, CPU and scratch sheets as '
        '29bedb2b6722, with a later file version (and, for the serial worker, multithreaded calculation off).',
        'Its own answers (answers): DOLLAR gives the Rand sign R and U+0020 grouping; DATEVALUE reads 04/02/01 '
        'year-first and refuses 4/2/2001, the en-ZA short-date order; VALUE reads U+0020 as the group separator.',
    ],
    'touchstone_environment': record,
    'env_id': env_id,
    'service': 'touchstone/1',
    'profile': {'locale': 'en-ZA', 'decimal_separator': '.', 'thousands_separator': ' '},
    'addins_open': (record.get('addins_open') or []) if record else None,
    'answers': answers,
}
with open(out, 'w') as f:
    json.dump(doc, f, indent=1, ensure_ascii=True)
    f.write('\n')
print(env_id, [(a['function'], a['answer']) for a in answers])
