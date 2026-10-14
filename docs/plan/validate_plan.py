#!/usr/bin/env python3
"""Validate exported planning artifacts. Does not run Rust, Excel or br.

Usage: python validate_plan.py [--directory DIR] [--self-test]
Requires Python 3.9+ and only the standard library.
"""
from __future__ import annotations
import argparse
import copy
import hashlib
import json
import re
import shutil
import sys
from collections import Counter
from graphlib import TopologicalSorter, CycleError
from pathlib import Path
from typing import Any

SECTIONS = ['Outcome', 'Context', 'Do', 'Port from', 'Acceptance', 'Platforms', 'Out of scope']
FIELDS = {'id', 'title', 'issue_type', 'priority', 'labels', 'dependencies', 'description'}
ID = re.compile(r'^(infra|cfunc|cfml|ceng|cdoc|chost|onecalc|dnacalc)-m\d+(?:\.\d+)?$')
TEST = re.compile(r'^- `([a-z][a-z0-9_]+)`: ', re.M)

class InvalidPlan(ValueError):
    pass

def require(condition: bool, message: str) -> None:
    if not condition:
        raise InvalidPlan(message)

def read_jsonl(path: Path) -> list[dict[str, Any]]:
    records = []
    for number, line in enumerate(path.read_text(encoding='utf-8').splitlines(), 1):
        require(bool(line.strip()), f'{path.name}:{number}: blank record')
        try:
            item = json.loads(line)
        except json.JSONDecodeError as exc:
            raise InvalidPlan(f'{path.name}:{number}: {exc}') from exc
        require(isinstance(item, dict), f'{path.name}:{number}: expected object')
        records.append(item)
    return records

def validate_records(records: list[dict[str, Any]]) -> dict[str, Any]:
    require(bool(records), 'empty graph')
    by_id: dict[str, dict[str, Any]] = {}
    names: set[str] = set()
    for record in records:
        require(set(record) == FIELDS, f'Unexpected/missing fields on {record.get("id")}')
        rid = record['id']
        require(isinstance(rid, str) and bool(ID.fullmatch(rid)), f'Invalid provisional ID {rid}')
        require(rid not in by_id, f'Duplicate ID {rid}')
        by_id[rid] = record
        require(record['issue_type'] in {'epic','task','bug','chore'}, f'{rid}: invalid type')
        require(type(record['priority']) is int and 0 <= record['priority'] <= 4, f'{rid}: priority')
        require(isinstance(record['title'], str) and 0 < len(record['title']) <= 500, f'{rid}: title')
        require(isinstance(record['labels'], list) and len(set(record['labels'])) == len(record['labels']), f'{rid}: labels')
        require(isinstance(record['description'],str), f'{rid}: description')
        require(re.findall(r'^## ([^\n]+)$', record['description'], re.M) == SECTIONS, f'{rid}: description sections/order')
        require(isinstance(record['dependencies'], list), f'{rid}: dependencies')
        if record['issue_type'] != 'epic':
            cases = TEST.findall(record['description'])
            require(len(cases) >= 2, f'{rid}: fewer than two explicit acceptance cases')
            require('```text\n' in record['description'], f'{rid}: no command block')
            for case in cases:
                require(case not in names, f'Duplicate acceptance name {case}')
                names.add(case)
    dag = {}
    edge_counts = Counter()
    for rid, record in by_id.items():
        seen = set(); parents = []; blockers = set()
        for dep in record['dependencies']:
            require(set(dep) == {'depends_on_id','type'}, f'{rid}: dependency field shape')
            target, kind = dep['depends_on_id'], dep['type']
            require(target in by_id, f'{rid}: dangling dependency {target}')
            require(target != rid, f'{rid}: self dependency')
            require(kind in {'blocks','parent-child'}, f'{rid}: dependency type')
            require((target,kind) not in seen, f'{rid}: duplicate dependency')
            seen.add((target,kind));edge_counts[kind] += 1
            if kind == 'parent-child':
                parents.append(target)
                require(by_id[target]['issue_type'] == 'epic', f'{rid}: parent is not epic')
                require(target == rid.rsplit('.',1)[0], f'{rid}: unexpected parent')
            else:
                require(by_id[target]['issue_type'] != 'epic', f'{rid}: blocks on grouping epic')
                blockers.add(target)
        require(len(parents) == (0 if record['issue_type']=='epic' else 1), f'{rid}: parent count')
        if record['issue_type'] != 'epic': dag[rid] = blockers
    try:
        ordered = list(TopologicalSorter(dag).static_order())
    except CycleError as exc:
        raise InvalidPlan(f'Blocking dependency cycle: {exc}') from exc
    return {'records':len(records),'tasks':len(dag),'epics':len(records)-len(dag),
            'blocking_edges':edge_counts['blocks'],'parent_child_edges':edge_counts['parent-child'],
            'named_acceptance_cases':len(names),'topological_task_count':len(ordered),
            'tasks_by_milestone':dict(sorted(Counter(t['labels'][0] for t in records if t['issue_type']!='epic').items()))}

def refs_in(value: Any):
    if isinstance(value, str) and ID.fullmatch(value):
        yield value
    elif isinstance(value, list):
        for item in value: yield from refs_in(item)
    elif isinstance(value, dict):
        for item in value.values(): yield from refs_in(item)

def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--directory', type=Path, default=Path(__file__).resolve().parent)
    ap.add_argument('--self-test', action='store_true')
    args = ap.parse_args(); root = args.directory.resolve()
    try:
        records = read_jsonl(root/'beads.plan.jsonl')
        report = validate_records(records)
        by_id = {t['id']:t for t in records}
        coverage = json.loads((root/'coverage.json').read_text(encoding='utf-8'))
        references = list(refs_in(coverage))
        for rid in references: require(rid in by_id, f'Coverage references absent ID {rid}')
        native = read_jsonl(root/'beads.br.jsonl')
        idmap = json.loads((root/'br-id-map.json').read_text(encoding='utf-8'))
        require(set(idmap) == set(by_id), 'ID map does not cover all records')
        require(len(set(idmap.values())) == len(idmap), 'ID map is not one-to-one')
        require(len(native) == len(records), 'Native/plan record counts differ')
        native_by = {t['id']:t for t in native}
        require(set(native_by) == set(idmap.values()), 'Native IDs differ from map')
        for rid, record in by_id.items():
            n = native_by[idmap[rid]]
            for field in FIELDS - {'id','dependencies'}:
                require(n[field] == record[field], f'{rid}: native {field} changed')
            require(n['status']=='open', f'{rid}: unexpected initial state')
            for key in ['created_at','updated_at']:
                require(bool(re.fullmatch(r'\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ',n[key])), f'{rid}: timestamp')
            expected = [(idmap[d['depends_on_id']],d['type']) for d in record['dependencies']]
            got = [(d['depends_on_id'],d['type']) for d in n['dependencies']]
            require(got == expected, f'{rid}: native dependencies changed')
            for d in n['dependencies']:
                require(d['issue_id']==n['id'], f'{rid}: wrong dependency source')
                require('created_at' in d and 'metadata' in d, f'{rid}: native dependency metadata missing')
        completed = {t['id'] for t in records if t['labels'][0]=='m0' and t['issue_type']!='epic'}
        lanes = coverage['independent_lanes_after_bootstrap']
        require(len({by_id[r]['labels'][1] for r in lanes}) >= 4, 'Fewer than four distinct bootstrap lanes')
        for rid in lanes:
            blockers={d['depends_on_id'] for d in by_id[rid]['dependencies'] if d['type']=='blocks'}
            require(blockers <= completed, f'{rid}: not ready after bootstrap: {blockers-completed}')
        # All existing task work must feed the release-checker implementation.
        tasks={t['id']:t for t in records if t['issue_type']!='epic'}
        ancestors=set()
        def visit(rid):
            if rid in ancestors: return
            ancestors.add(rid)
            for dep in tasks[rid]['dependencies']:
                if dep['type']=='blocks': visit(dep['depends_on_id'])
        visit(coverage['release_gate']['implementation_task'])
        require(ancestors == set(tasks), f'Tasks not connected to release checker: {set(tasks)-ancestors}')
        outline=(root/'BEAD_GRAPH.md').read_text(encoding='utf-8')
        plan=(root/'DNA_CALC_REIMPLEMENTATION_PLAN.md').read_text(encoding='utf-8')
        for rid in by_id:
            require(f'`{rid}`' in outline, f'{rid}: absent from readable outline')
            require(f'`{rid}`' in plan, f'{rid}: absent from integrated outline')
        negatives=[]
        if args.self_test:
            for kind in ['duplicate','dangling','cycle','missing_section','wrong_priority']:
                bad=copy.deepcopy(records)
                task=next(t for t in bad if t['issue_type']!='epic')
                if kind=='duplicate': bad.append(copy.deepcopy(task))
                elif kind=='dangling': task['dependencies'].append({'depends_on_id':'infra-m99.1','type':'blocks'})
                elif kind=='cycle':
                    other=next(t for t in bad if t['issue_type']!='epic' and t['id']!=task['id'])
                    task['dependencies'].append({'depends_on_id':other['id'],'type':'blocks'})
                    if not any(d['depends_on_id']==task['id'] and d['type']=='blocks' for d in other['dependencies']):
                        other['dependencies'].append({'depends_on_id':task['id'],'type':'blocks'})
                elif kind=='missing_section': task['description']=task['description'].replace('## Port from','## Source')
                elif kind=='wrong_priority': task['priority']=7
                try: validate_records(bad)
                except InvalidPlan as exc:
                    if kind=='cycle': require('cycle' in str(exc).lower(), 'Cycle self-test failed for the wrong reason')
                    negatives.append(kind)
                else: raise InvalidPlan(f'Validator failed to reject {kind}')
        report.update(status='passed',coverage_references_checked=len(references),
                      independent_ready_lanes=lanes,all_tasks_reach_release_checker=True,negative_tests_rejected=negatives,
                      br_executable_available=bool(shutil.which('br')),
                      br_import_executed=False,rust_compilation_executed=False,excel_tests_executed=False,
                      scope='Artifact validation only: JSON fields, graph integrity, description/test shape, coverage IDs, outline presence and native-export equivalence. Not proof of implementation correctness or br CLI compatibility.')
        report['sha256']={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in [root/'beads.plan.jsonl',root/'beads.br.jsonl',root/'coverage.json',root/'DNA_CALC_REIMPLEMENTATION_PLAN.md']}
        (root/'validation-report.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
        print(json.dumps(report,indent=2))
        return 0
    except (InvalidPlan, OSError, json.JSONDecodeError, KeyError, TypeError) as exc:
        print(f'VALIDATION FAILED: {exc}',file=sys.stderr)
        return 1

if __name__=='__main__':
    raise SystemExit(main())
