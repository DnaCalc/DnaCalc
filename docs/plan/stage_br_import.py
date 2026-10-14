#!/usr/bin/env python3
"""Import the native JSONL into a NEW, isolated br workspace and verify it.

Requires an installed br (bv is optional). Never overwrites an existing directory and
never uses --force, --rebuild, --skip-invalid-records or --rename-prefix.

Executed on 2026-10-12 with br 0.1.34 on Windows: 252 issues created, no cycles,
one ready task (infra-m0.1), flush-only re-export identical to the import.
"""
from __future__ import annotations
import argparse
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

# br and bv print UTF-8 symbols; a cp1252 console must not abort the helper after a successful import.
for _stream in (sys.stdout, sys.stderr):
    if hasattr(_stream, 'reconfigure'):
        _stream.reconfigure(encoding='utf-8', errors='replace')


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--destination', type=Path, required=True,
                        help='A new directory that does not already exist (for a real roll-out: the new monorepo root)')
    parser.add_argument('--bundle', type=Path, default=Path(__file__).resolve().parent)
    parser.add_argument('--prefix', default='dc',
                        help="Prefix for issues created later in this tracker (default dc). Imported IDs keep their own "
                             "library prefixes. Do not use 'calc': OxCalc's existing tracker already issues calc-* IDs.")
    parser.add_argument('--allow-git-ancestor', action='store_true',
                        help='Permit a destination under an existing git repository (needed when the destination is the new monorepo root inside a parent checkout).')
    args = parser.parse_args()
    root = args.bundle.resolve(); dest = args.destination.resolve()
    if dest.exists():
        parser.error('Destination already exists. Use a new directory; do not overwrite a live tracker.')
    for ancestor in dest.parents:
        if (ancestor / '.beads').exists():
            parser.error('An ancestor directory already has a .beads workspace; br would discover it. Choose another destination.')
        if (ancestor / '.git').exists() and not args.allow_git_ancestor:
            parser.error('An ancestor directory is a git repository. Pass --allow-git-ancestor if that is intended.')
    br = shutil.which('br')
    if not br:
        parser.error('br is not on PATH. Install/configure it separately, then rerun this helper.')
    bv = shutil.which('bv')
    source = root / 'beads.br.jsonl'
    if not source.is_file():
        parser.error(f'Native export is absent: {source}')
    validation = subprocess.run([sys.executable, str(root / 'validate_plan.py'), '--directory', str(root), '--self-test'],
                                text=True, capture_output=True, encoding='utf-8', errors='replace')
    if validation.returncode:
        print(validation.stdout); print(validation.stderr, file=sys.stderr)
        return validation.returncode
    dest.mkdir(parents=True, exist_ok=False)
    log = []
    # Avoid inheriting environment variables that select a different tracker.
    child_env = {k: v for k, v in os.environ.items() if not k.startswith(('BR_', 'BD_', 'BEADS_', 'BEAD_'))}

    def run(command, must_succeed=True):
        r = subprocess.run(command, cwd=dest, text=True, capture_output=True, env=child_env, encoding='utf-8', errors='replace')
        log.append({'command': command, 'returncode': r.returncode, 'stdout': r.stdout, 'stderr': r.stderr})
        if must_succeed and r.returncode:
            raise RuntimeError(f'Command failed ({r.returncode}): {" ".join(command)}\n{r.stdout}\n{r.stderr}')
        return r

    try:
        run([br, '--version'])
        run([br, 'init', '--prefix', args.prefix])
        beads = dest / '.beads'
        if not beads.is_dir():
            raise RuntimeError('br init did not create the expected .beads directory.')
        target = beads / 'issues.jsonl'
        if target.exists() and target.read_text(encoding='utf-8').strip():
            raise RuntimeError('New workspace already contains nonempty issue data; refusing to overwrite it.')
        shutil.copyfile(source, target)
        imported = run([br, 'sync', '--import-only'])
        cycles = run([br, 'dep', 'cycles'])
        if 'no dependency cycles' not in cycles.stdout.lower():
            raise RuntimeError('br dep cycles did not report a cycle-free graph:\n' + cycles.stdout)
        stats = run([br, 'stats'])
        ready = run([br, 'ready', '--json'])
        ready_ids = []
        try:
            data = json.loads(ready.stdout)
            items = data if isinstance(data, list) else data.get('issues', [])
            ready_ids = [i['id'] for i in items]
        except ValueError:
            pass
        expected = sum(1 for line in source.read_text(encoding='utf-8').splitlines() if line.strip())
        if f'Created: {expected} issues' not in imported.stdout:
            raise RuntimeError(f'Expected {expected} created issues; br reported:\n{imported.stdout}')
        if bv:
            run([bv, '--robot-insights'], must_succeed=False)
        print(f'br imported {expected} records into {dest} with no dependency cycles.')
        print('Ready now:', ', '.join(ready_ids) or '(none reported)')
        print(stats.stdout.strip())
        print('Next: br show infra-m0.1 ; bv --robot-next (bv --robot-plan lists epics as single-item tracks; ignore epic rows).')
        print('This is an imported plan, not completed implementation work.')
        return 0
    except (RuntimeError, OSError) as exc:
        print(str(exc), file=sys.stderr)
        print(f'Staging directory retained for inspection: {dest}', file=sys.stderr)
        return 1
    finally:
        (dest / 'import-log.json').write_text(json.dumps(log, indent=2) + '\n', encoding='utf-8')


if __name__ == '__main__':
    raise SystemExit(main())
