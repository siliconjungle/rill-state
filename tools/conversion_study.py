#!/usr/bin/env python3
"""Predeclared fresh-process Compact growth diagnostics, separate from engine rankings."""
import argparse
import concurrent.futures
import hashlib
import itertools
import json
import pathlib
import random
import statistics
import subprocess
import time

import study


def execute(task):
    binary, shape, load, capacity, adaptive, pin, rep = task
    command = [str(binary), '--shape', shape, '--load', str(load),
               '--capacity', str(capacity), '--adaptive', str(adaptive).lower(),
               '--pin-initial', str(pin).lower()]
    result = study.run_bounded(command, timeout=300)
    if result.returncode:
        raise RuntimeError(json.dumps(dict(command=command, code=result.returncode,
                                          stderr=result.stderr[-3000:], stdout=result.stdout[-3000:])))
    row = json.loads(result.stdout)
    assert row['verified'] and row['kind'] == 'compact-single-growth'
    for key, expected in [('shape', shape), ('load', load), ('capacity_before', capacity),
                          ('adaptive', adaptive), ('pin_initial', pin)]:
        assert row[key] == expected
    assert row['capacity_after'] == capacity * 2
    assert row['records_before'] == capacity * load // 100
    assert row['word_bytes_before'] == (4 if adaptive and capacity <= 65536 else 8)
    assert row['word_bytes_after'] == (4 if adaptive and capacity * 2 <= 65536 else 8)
    assert row['insertion_ns'] > 0
    return dict(row, repetition=rep)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--binary', type=pathlib.Path, required=True)
    p.add_argument('--build-receipt', type=pathlib.Path, required=True)
    p.add_argument('--label', required=True)
    p.add_argument('--workers', type=int, default=2)
    p.add_argument('--repetitions', type=int, default=7)
    a = p.parse_args()
    assert 1 <= a.workers <= 4 and a.repetitions >= 7
    assert __import__('re').fullmatch(r'[a-zA-Z0-9_.-]+', a.label)
    binary = a.binary.resolve()
    binding = json.loads(a.build_receipt.read_text())
    assert hashlib.sha256(binary.read_bytes()).hexdigest() == binding['binary_sha256']
    assert study.source_hash_at(binding['source_commit']) == binding['source_sha256']
    source = subprocess.check_output(['git', 'show', binding['source_commit'] +
                                     ':examples/compact_conversion.rs'], cwd=study.ROOT)
    assert hashlib.sha256(source).hexdigest() == binding['example_sha256']
    shapes = ['u64-inline', 'string-inline', 'u64-owned256', 'string-owned256']
    loads = [25, 50, 70, 85]
    capacities = [32768, 65536, 131072]
    tasks = [(binary, *t) for t in itertools.product(shapes, loads, capacities,
                 [False, True], [False, True], range(a.repetitions))]
    random.Random(0xC04FAC7).shuffle(tasks)
    folder = study.ROOT / 'evidence' / a.label
    folder.mkdir(exist_ok=False)
    receipt = dict(kind='raw-compact-growth-diagnostic', build=binding,
                   claim='Single raw insertion including growth/COW. Setup and full validation untimed. '
                         'Parallel diagnostic medians are not complete engine throughput, service latency or final acceptance.',
                   shapes=shapes, loads=loads, capacities=capacities, pin_initial=[False, True],
                   adaptive=[False, True], repetitions=a.repetitions, workers=a.workers, tasks=len(tasks),
                   driver_sha256=hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),
                   executor_sha256=hashlib.sha256((study.ROOT / 'tools/study.py').read_bytes()).hexdigest(),
                   driver_source_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
                   build_receipt_sha256=hashlib.sha256(a.build_receipt.read_bytes()).hexdigest(),
                   started_at_unix=time.time())
    (folder / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    rows, errors = [], []
    with (folder / 'runs.jsonl').open('x') as out, concurrent.futures.ThreadPoolExecutor(a.workers) as pool:
        futures = {pool.submit(execute, t): t for t in tasks}
        for f in concurrent.futures.as_completed(futures):
            try:
                row = f.result()
                rows.append(row)
                out.write(json.dumps(row) + '\n')
                out.flush()
            except Exception as error:
                errors.append(dict(task=list(map(str, futures[f])), error=str(error)))
                (folder / 'errors.json').write_text(json.dumps(errors, indent=2) + '\n')
            if (len(rows) + len(errors)) % 50 == 0:
                print(f'{len(rows)} completed, {len(errors)} failures / {len(tasks)}', flush=True)
    receipt.update(completed=len(rows), failures=len(errors), finished_at_unix=time.time())
    (folder / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    if errors:
        raise SystemExit(1)
    groups = {}
    for row in rows:
        key = tuple(row[k] for k in ['shape', 'load', 'capacity_before', 'adaptive', 'pin_initial'])
        groups.setdefault(key, []).append(row['insertion_ns'])
    assert len(groups) == 192 and all(len(values) == a.repetitions for values in groups.values())
    cells = [dict(shape=k[0], load=k[1], capacity_before=k[2], adaptive=k[3], pin_initial=k[4],
                  raw_insertion_ns_median=statistics.median(values)) for k, values in sorted(groups.items())]
    (folder / 'diagnostic-medians.json').write_text(json.dumps(dict(claim=receipt['claim'], cells=cells), indent=2) + '\n')
    print(json.dumps(dict(label=a.label, completed=len(rows), failures=0)), flush=True)


if __name__ == '__main__':
    main()
