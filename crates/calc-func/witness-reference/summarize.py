"""Summarize per-function reference tables and reconcile them with a witness judge report."""

import argparse
import csv
import json
from collections import Counter
from decimal import Decimal
from pathlib import Path


CLASSES = ('ours-wrong', 'excel-quirk', 'large-deviation')


def summarize(directory, judge):
    summaries = []
    counts = Counter()
    for path in sorted(directory.glob('*.tsv')):
        if path.stem == 'summary':
            continue
        classes = Counter()
        details = Counter()
        worst = [Decimal(0), Decimal(0)]
        with path.open(encoding='utf-8', newline='') as table:
            for row in csv.DictReader(table, delimiter='\t', quoting=csv.QUOTE_NONE):
                classes[row['class']] += 1
                if 'both-inexact' in row['detail'].split(','):
                    details['both-inexact'] += 1
                if 'text15' in row['detail'].split(','):
                    details['text15'] += 1
                for i, field in enumerate(('excel_ulp', 'ours_ulp')):
                    if row[field]:
                        worst[i] = max(worst[i], Decimal(row[field]))
        counts.update(classes)
        summaries.append((path.stem, sum(classes.values()), classes, details, worst))

    if judge:
        report = json.loads(judge.read_text(encoding='utf-8'))
        totals = report['totals']
        reported = {r['function']: r['failed'] for r in report['functions']}
        untraced_exclusions = Counter(row['function'] for row in report['excluded_rows']
                                      if 'detail' in row)
        missing = Counter(reported)
        missing.subtract({function: n for function, n, *_ in summaries})
        missing.subtract(untraced_exclusions)
        if any(n < 0 for n in missing.values()) or sum(missing.values()) != totals['unread']:
            differences = {f: n for f, n in missing.items() if n}
            raise ValueError(f'Tables disagree with judge after exclusions and '
                             f'{totals["unread"]} unread: {differences}')
        covered = {s[0] for s in summaries}
        empty = (f for f, n in (missing + untraced_exclusions).items() if n and f not in covered)
        summaries.extend((f, 0, Counter(), Counter(), [Decimal(0), Decimal(0)]) for f in empty)
        print(f'Judge: {totals["matched"]} matched, {totals["failed"]} failed of {totals["rows"]}; '
              f'{totals["unread"]} unread, {sum(untraced_exclusions.values())} excluded without trace')
    else:
        missing = Counter()
        untraced_exclusions = Counter()

    with (directory / 'summary.tsv').open('w', encoding='utf-8', newline='') as output:
        writer = csv.writer(output, delimiter='\t', lineterminator='\n')
        writer.writerow(('function', 'mismatching_rows', *CLASSES, 'other_classes',
                         'worst_excel_ulp', 'worst_ours_ulp', 'both_inexact', 'text15', 'unread',
                         'excluded_without_trace'))
        for f, n, classes, details, worst in sorted(summaries, key=lambda s: (-s[1], s[0])):
            other = ';'.join(f'{k}={v}' for k, v in sorted(classes.items()) if k not in CLASSES)
            writer.writerow((f, n, *(classes[k] for k in CLASSES), other, *worst,
                             details['both-inexact'], details['text15'], missing[f],
                             untraced_exclusions[f]))
    print(f'{sum(counts.values())} traced mismatches in '
          f'{sum(n > 0 for _, n, *_ in summaries)} functions')
    for cls, count in sorted(counts.items()):
        print(f'{cls}: {count}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=Path)
    parser.add_argument('--judge', type=Path)
    options = parser.parse_args()
    summarize(options.directory, options.judge)


if __name__ == '__main__':
    main()
