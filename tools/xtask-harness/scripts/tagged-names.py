#!/usr/bin/env python3
"""Month and weekday names under a number format's locale tag, as Excel writes them (dc-zsn.4).

  python3 tools/xtask-harness/scripts/tagged-names.py ask --out <readings.json>
  python3 tools/xtask-harness/scripts/tagged-names.py write <readings.json>

`ask` sends Touchstone sheet cases (tools/excel-service/touchstone/PROTOCOL.md), one per tag: the TEXT of
the first of each month of 2023 and of 1 to 7 January 2023 (a Sunday first) under `[$-<tag>]mmmm`, `mmm`,
`dddd` and `ddd`, and of 6 am and 6 pm under `[$-<tag>]AM/PM`, joined by |. The tags are every LCID with a
primary language 0x01 to 0xFF and a sublanguage 1 to 16, LCID 0, and the BCP 47 forms (`de-AT`) of the LCIDs
Python's locale.windows_locale names. Nothing is computed: each answer is Excel's.

`write` turns the readings into
  crates/calc-func/tests/fixtures/fn5_tagged_names/excel-lcids.tsv (the readings the test checks), and
  crates/calc-func/src/format/tagged_names.rs (the table calc-func reads).
An LCID whose four lists are those of LCID 0 (Excel's fallback to the host's names) is not listed: calc-func
then writes the host's names as well. A BCP 47 tag is listed when its names are its LCID's.
"""
import argparse, json, locale, pathlib, sys, time

HERE = pathlib.Path(__file__).resolve()
REPO = HERE.parents[3]
FIXTURE = REPO / 'crates/calc-func/tests/fixtures/fn5_tagged_names/excel-lcids.tsv'
TABLE = REPO / 'crates/calc-func/src/format/tagged_names.rs'
FIELDS = ('mmmm', 'mmm', 'dddd', 'ddd')
ALL = FIELDS + ('AM/PM',)
MONTHS = 'DATE(2023,SEQUENCE(12),1)'
DAYS = 'DATE(2023,1,SEQUENCE(7))'


def probe(tag):
    text = lambda dates, code: 'TEXT(%s,"[$-%s]%s")' % (dates, tag, code)
    return '=TEXTJOIN("|",FALSE,%s,%s,%s,%s,%s,%s)' % (text(MONTHS, 'mmmm'), text(MONTHS, 'mmm'), text(DAYS, 'dddd'),
                                                      text(DAYS, 'ddd'), text('0.25', 'AM/PM'), text('0.75', 'AM/PM'))


def ask(out):
    sys.path.insert(0, str(REPO / 'tools' / 'excel-service'))
    from client import Client, sheet_case
    lcids = [0] + sorted({(sub << 10) | prim for prim in range(1, 0x100) for sub in range(1, 17)})
    bcp47 = sorted({name.replace('_', '-') for name in locale.windows_locale.values()})
    tags = ['%X' % lcid for lcid in lcids] + bcp47
    client = Client(timeout=3600)
    sock = client._socket()
    rows = [[sheet_case([{'name': 'Sheet1', 'cells': {}}], [{'at': 'Sheet1!H10', 'formula': probe(tag)}])] for tag in tags]
    answers, envs = [], set()
    for start in range(0, len(rows), 500):
        pending = sock.submit_rows('_CASE', rows[start:start + 500], priority='bulk')
        reply = pending.result(timeout=3600)
        envs.update(pending.env_ids)
        answers += reply['rows']
    if len(envs) != 1:
        sys.exit('answered by several environments: %s' % sorted(envs))
    readings = {}
    for tag, row in zip(tags, answers):
        probe_answer = row['probes'][0] if row.get('type') == 'sheet_case' else None
        value = probe_answer and probe_answer.get('value') or {}
        if value.get('type') != 'text':
            sys.exit('%s: no text answer: %r' % (tag, row))
        names = value['value'].split('|')
        if len(names) != 40:
            sys.exit('%s: %d names' % (tag, len(names)))
        readings[tag] = {'mmmm': names[0:12], 'mmm': names[12:24], 'dddd': names[24:31], 'ddd': names[31:38],
                         'AM/PM': names[38:40]}
    doc = {'service': 'touchstone/1', 'env_id': envs.pop(), 'timestamp': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()),
           'script': 'tools/xtask-harness/scripts/tagged-names.py', 'readings': readings}
    pathlib.Path(out).write_text(json.dumps(doc, ensure_ascii=False, indent=0) + '\n', encoding='utf-8')
    print('%d tags, env %s' % (len(readings), doc['env_id']))


def rust_text(s):
    return json.dumps(s, ensure_ascii=False)


def write(path):
    doc = json.loads(pathlib.Path(path).read_text(encoding='utf-8'))
    readings = doc['readings']
    fallback = readings['0']
    lcids = sorted(int(tag, 16) for tag in readings if '-' not in tag)
    known = [lcid for lcid in lcids if lcid and readings['%X' % lcid] != fallback]
    fallen = [lcid for lcid in lcids if lcid and readings['%X' % lcid] == fallback]
    by_name = {}
    for lcid in known:
        name = locale.windows_locale.get(lcid)
        if name and readings.get(name.replace('_', '-')) == readings['%X' % lcid]:
            by_name[name.replace('_', '-')] = lcid
    lists = {}
    for lcid in known:
        for field in FIELDS:
            lists.setdefault(tuple(readings['%X' % lcid][field]), len(lists))
    head = ['# dc-zsn.4: month and weekday names under [$-<tag>] as Excel writes them, %s (env %s, en-ZA host), read by' % (
                doc['timestamp'][:10], doc['env_id']),
            '# tools/xtask-harness/scripts/tagged-names.py: TEXT of 1 Jan to 1 Dec 2023 (mmmm, mmm) and 1 to 7 Jan 2023',
            '# (dddd, ddd), 6 am and 6 pm (AM/PM). Lines: tag, then the five lists joined by |. The fallback line lists',
            '# the LCIDs that write what LCID 0 writes (the host\'s names and designators).']
    lines = head + ['fallback\t' + ','.join('%X' % lcid for lcid in fallen)]
    for tag in ['0'] + ['%X' % lcid for lcid in known] + sorted(by_name):
        lines.append('\t'.join([tag] + ['|'.join(readings[tag][field]) for field in ALL]))
    FIXTURE.parent.mkdir(parents=True, exist_ok=True)
    FIXTURE.write_text('\n'.join(lines) + '\n', encoding='utf-8')
    rust = ['//! Month and weekday names under a number format\'s locale tag (`[$-407]mmmm`), as Excel 16.0.%s writes them' % '20527',
            '//! under an en-ZA host (dc-zsn.4). Written by tools/xtask-harness/scripts/tagged-names.py from its Touchstone',
            '//! readings (env %s, %s), which tests/fixtures/fn5_tagged_names/excel-lcids.tsv keeps; do not edit by hand.' % (
                doc['env_id'], doc['timestamp'][:10]),
            '//! An LCID that is not listed writes the host\'s names and designators (Excel\'s answer for the LCIDs it does not',
            '//! know).',
            '',
            '/// The distinct lists of names.',
            '#[rustfmt::skip]',
            'pub(super) const LISTS: [&[&str]; %d] = [' % len(lists)]
    for names in lists:
        rust.append('    &[%s],' % ', '.join(rust_text(n) for n in names))
    rust += ['];', '',
             '/// Each LCID Excel names otherwise than its fallback: its long and short months, then its long and short',
             '/// weekdays (Sunday first), as indexes into [`LISTS`], and its AM and PM designators; in LCID order.',
             '#[rustfmt::skip]',
             'pub(super) const BY_LCID: [(u16, [u16; 4], [&str; 2]); %d] = [' % len(known)]
    for lcid in known:
        r = readings['%X' % lcid]
        rust.append('    (0x%04X, [%s], [%s]),' % (lcid, ', '.join(str(lists[tuple(r[f])]) for f in FIELDS),
                                                 ', '.join(rust_text(x) for x in r['AM/PM'])))
    rust += ['];', '',
             '/// BCP 47 tags (`[$-de-AT]`) whose names Excel writes as its LCID\'s; in tag order.',
             '#[rustfmt::skip]',
             'pub(super) const BY_TAG: [(&str, u16); %d] = [' % len(by_name)]
    for tag in sorted(by_name):
        rust.append('    (%s, 0x%04X),' % (rust_text(tag), by_name[tag]))
    rust.append('];')
    TABLE.write_text('\n'.join(rust) + '\n', encoding='utf-8')
    print('%d LCIDs named, %d fall back, %d lists, %d BCP 47 tags' % (len(known), len(fallen), len(lists), len(by_name)))


def main():
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest='command', required=True)
    a = sub.add_parser('ask'); a.add_argument('--out', required=True)
    w = sub.add_parser('write'); w.add_argument('readings')
    args = parser.parse_args()
    ask(args.out) if args.command == 'ask' else write(args.readings)


if __name__ == '__main__':
    main()
