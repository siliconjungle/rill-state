#!/usr/bin/env python3
"""Audit exact cold tuples and independently rebuild the Rust trace/checksums in Python."""
import argparse
import functools
import hashlib
import itertools
import json
import pathlib
import statistics

import study
from adaptive_cold_study import digest, git_hash, validate_rows

MASK = (1 << 64) - 1


def rotate(n, bits):
    return ((n << bits) | (n >> (64 - bits))) & MASK


@functools.cache
def oracle(seed, records, inline):
    epochs = [0] * records
    result = []
    def random_word():
        nonlocal seed
        seed = (seed ^ (seed << 13)) & MASK
        seed ^= seed >> 7
        seed = (seed ^ (seed << 17)) & MASK
        return seed
    for step in range(16):
        if step and step % 5 == 0:
            ids = list(range(records))
            for i in range(records - 1, 0, -1):
                j = random_word() % (i + 1)
                ids[i], ids[j] = ids[j], ids[i]
            ids = ids[:(records + 1) // 2]
        elif step:
            ids = [random_word() % records for _ in range(records // 100)]
        else:
            ids = []
        for key in ids:
            epochs[key] = step
        if inline:
            checksum = sum(rotate(key, 7) ^ (epoch * 317 & MASK) ^ (MASK ^ key)
                           ^ rotate((key + epoch) & MASK, 23)
                           for key, epoch in enumerate(epochs)) & MASK
        else:
            checksum = sum(key ^ epoch for key, epoch in enumerate(epochs)) & MASK
        result.append(checksum)
    return result


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('folder', type=pathlib.Path)
    p.add_argument('--build-receipt', type=pathlib.Path, required=True)
    p.add_argument('--output', type=pathlib.Path, required=True)
    a = p.parse_args()
    receipt = json.loads((a.folder / 'receipt.json').read_text())
    assert receipt['kind'] == 'raw-adaptive-cold-diagnostic'
    assert receipt['complete'] and receipt['failures'] == 0
    binding = json.loads(a.build_receipt.read_text())
    assert receipt['build'] == binding
    assert digest(a.build_receipt) == receipt['build_receipt_sha256']
    assert digest(study.ROOT / binding['binary']) == binding['binary_sha256']
    assert study.source_hash_at(binding['source_commit']) == binding['source_sha256']
    assert git_hash(binding['source_commit'], 'examples/adaptive_cold.rs') == binding['example_sha256']
    commit = receipt['driver_source_commit']
    for field, path in [('driver_sha256', 'tools/adaptive_cold_study.py'),
                        ('executor_sha256', 'tools/study.py'), ('plan_sha256', receipt['plan'])]:
        assert git_hash(commit, path) == receipt[field]
    assert digest(study.ROOT / receipt['plan']) == receipt['plan_sha256']
    plan = json.loads((study.ROOT / receipt['plan']).read_text())['cold']
    for field in ['engines', 'shapes', 'seeds']:
        assert receipt[field] == plan[field]
    cohort = plan[receipt['cohort']]
    for field in ['records', 'repetitions']:
        assert receipt[field] == cohort[field]
    rows = [json.loads(line) for line in (a.folder / 'runs.jsonl').read_text().splitlines()]
    expected = set(itertools.product(receipt['engines'], receipt['shapes'], receipt['seeds'],
                                    range(receipt['repetitions']), range(16)))
    keys = ['engine', 'shape', 'seed', 'repetition', 'step']
    actual = [tuple(row[k] for k in keys) for row in rows]
    assert len(actual) == len(set(actual)) == len(expected) == receipt['tasks'] * 16
    assert set(actual) == expected
    assert receipt['completed'] == receipt['tasks'] == cohort['processes']
    processes = {}
    cells = {}
    for row in rows:
        key = tuple(row[k] for k in keys[:-1])
        processes.setdefault(key, []).append(row)
        assert row['checksum'] == oracle(row['seed'], receipt['records'], row['shape'].endswith('-inline'))[row['step']]
        cells.setdefault((row['engine'], row['shape'], row['step']), []).append(row)
    for (engine, shape, seed, _), process_rows in processes.items():
        validate_rows(sorted(process_rows, key=lambda row: row['step']),
                      engine, shape, seed, receipt['records'])
    fields = ['batch_ns', 'scan1_ns', 'scan2_ns', 'scan3_ns']
    medians = [dict(engine=e, shape=s, step=step,
                    **{field: statistics.median(row[field] for row in values) for field in fields})
               for (e, s, step), values in sorted(cells.items())]
    result = dict(claim=receipt['claim'], processes=len(processes), rows=len(rows),
                  exact_tuple_coverage=True, independently_rebuilt_trace_checksums=True,
                  representation_and_cache_states=True, frozen_inputs_and_driver_verified=True,
                  runs_sha256=digest(a.folder / 'runs.jsonl'), auditor_sha256=digest(pathlib.Path(__file__)),
                  source_commit=binding['source_commit'], binary_sha256=binding['binary_sha256'],
                  descriptive_medians=medians)
    with a.output.open('x') as out:
        json.dump(result, out, indent=2)
        out.write('\n')
    print(json.dumps({k: v for k, v in result.items() if k != 'descriptive_medians'}))


if __name__ == '__main__':
    main()
