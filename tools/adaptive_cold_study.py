#!/usr/bin/env python3
"""Registered mixed-density raw-map diagnostics; never final speed acceptance."""
import argparse
import concurrent.futures
import hashlib
import itertools
import json
import pathlib
import random
import re
import subprocess
import time

import study


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def git_hash(commit, path):
    return hashlib.sha256(subprocess.check_output(
        ['git', 'show', f'{commit}:{path}'], cwd=study.ROOT)).hexdigest()


def validate_rows(rows, engine, shape, seed, records):
    assert len(rows) == 16
    adaptive = engine.startswith('adaptive-')
    if adaptive:
        _, mode, builder, threshold, delay, gate, page, direction = engine.split('-')
        assert mode in ['inline', 'shared', 'cache', 'reuse', 'reusecache'] and builder == 'fast'
        assert threshold == '1' and gate == 'drop' and page in ['16', '64']
        assert delay in ['1', '3'] and direction in ['oneway', 'reset']
        delay = int(delay)
    else:
        assert engine in ['hashbrown', 'std-fast', 'std-btree', 'imbl-fast',
                          'record-paged16', 'record-paged64']
    paged = engine.startswith('record-paged')
    consecutive = 0
    for step, row in enumerate(rows):
        assert row['kind'] == 'adaptive-cold-mixed-density' and row['verified'] is True
        for field, expected in [('engine', engine), ('shape', shape), ('seed', seed),
                                ('records', records), ('step', step)]:
            assert row[field] == expected, (field, row[field], expected)
        phase = 'initial' if step == 0 else 'large' if step % 5 == 0 else 'small'
        assert row['phase'] == phase
        assert row['operations'] == (0 if step == 0 else (records + 1) // 2
                                     if phase == 'large' else records // 100)
        assert row['before_paged'] is paged
        if adaptive and step:
            if phase == 'large' and direction == 'reset':
                paged = False
                consecutive = 0
            elif not paged:
                consecutive = consecutive + 1 if phase == 'small' and shape != 'u64-inline' else 0
                if consecutive >= delay:
                    paged = True
        assert row['after_paged'] is paged, (engine, shape, step, paged)
        assert row['batch_ns'] == 0 if step == 0 else row['batch_ns'] > 0
        for scan in [1, 2, 3]:
            assert row[f'scan{scan}_ns'] > 0
            expected_cache = records if adaptive and mode in ['cache', 'reusecache'] and scan >= 2 else 0
            assert row[f'cache_after_scan{scan}'] == expected_cache


def execute(task):
    binary, engine, shape, seed, records, repetition = task
    command = [str(binary), '--engine', engine, '--shape', shape,
               '--seed', str(seed), '--records', str(records)]
    result = study.run_bounded(command, timeout=300)
    if result.returncode:
        raise RuntimeError(json.dumps(dict(command=command, code=result.returncode,
                            stderr=result.stderr[-4000:], stdout=result.stdout[-4000:])))
    rows = [json.loads(line) for line in result.stdout.splitlines()]
    validate_rows(rows, engine, shape, seed, records)
    return [dict(row, repetition=repetition) for row in rows]


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--binary', type=pathlib.Path, required=True)
    p.add_argument('--build-receipt', type=pathlib.Path, required=True)
    p.add_argument('--plan', type=pathlib.Path, required=True)
    p.add_argument('--label', required=True)
    p.add_argument('--cohort', choices=['smoke', 'proof-20k', 'full'], default='full')
    p.add_argument('--workers', type=int, default=2)
    a = p.parse_args()
    assert re.fullmatch(r'[a-zA-Z0-9_.-]+', a.label) and 1 <= a.workers <= 2
    plan = json.loads(a.plan.read_text())
    commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=study.ROOT, text=True).strip()
    plan_path = str(a.plan.resolve().relative_to(study.ROOT))
    assert git_hash(commit, plan_path) == digest(a.plan)
    binary = a.binary.resolve()
    binding = json.loads(a.build_receipt.read_text())
    if 'completed_cargo_dependencies' in binding:
        assert a.cohort != 'full', 'Proof-only artifacts cannot supply measured full diagnostics'
    assert digest(binary) == binding['binary_sha256']
    assert study.source_hash_at(binding['source_commit']) == binding['source_sha256']
    assert git_hash(binding['source_commit'], 'examples/adaptive_cold.rs') == binding['example_sha256']
    for path in ['tools/adaptive_cold_study.py', 'tools/study.py']:
        assert git_hash(commit, path) == digest(study.ROOT / path)
    cohort = plan['cold'][a.cohort]
    engines, shapes, seeds = (plan['cold'][k] for k in ['engines', 'shapes', 'seeds'])
    tasks = [(binary, e, s, seed, cohort['records'], rep)
             for e, s, seed, rep in itertools.product(engines, shapes, seeds,
                                                      range(cohort['repetitions']))]
    assert len(tasks) == cohort['processes']
    random.Random(0xADA971CE).shuffle(tasks)
    folder = study.ROOT / 'evidence' / a.label
    folder.mkdir(exist_ok=False)
    receipt = dict(kind='raw-adaptive-cold-diagnostic', build=binding,
        claim=plan['cold']['claim'], plan=plan_path, plan_sha256=digest(a.plan),
        timing_eligible=a.cohort == 'full',
        cohort=a.cohort, engines=engines, shapes=shapes, seeds=seeds,
        records=cohort['records'], repetitions=cohort['repetitions'], workers=a.workers,
        tasks=len(tasks), rows_per_process=16, driver_source_commit=commit,
        driver_sha256=digest(pathlib.Path(__file__)), executor_sha256=digest(study.ROOT / 'tools/study.py'),
        build_receipt_sha256=digest(a.build_receipt), started_at_unix=time.time())
    (folder / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    completed = 0
    errors = []
    with (folder / 'runs.jsonl').open('x') as out, concurrent.futures.ThreadPoolExecutor(a.workers) as pool:
        futures = {pool.submit(execute, task): task for task in tasks}
        for future in concurrent.futures.as_completed(futures):
            try:
                for row in future.result():
                    out.write(json.dumps(row) + '\n')
                out.flush()
                completed += 1
            except Exception as error:
                errors.append(dict(task=list(map(str, futures[future])), error=str(error)))
                (folder / 'errors.json').write_text(json.dumps(errors, indent=2) + '\n')
            if (completed + len(errors)) % 30 == 0:
                print(f'{completed} completed, {len(errors)} failures / {len(tasks)}', flush=True)
    receipt.update(completed=completed, failures=len(errors), complete=True, finished_at_unix=time.time())
    (folder / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print(json.dumps(dict(label=a.label, processes=completed, failures=len(errors))), flush=True)
    return int(bool(errors))


if __name__ == '__main__':
    raise SystemExit(main())
