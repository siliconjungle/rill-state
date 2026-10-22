#!/usr/bin/env python3
"""Independently check registered raw-growth tuples, identities and diagnostic medians."""
import argparse
import hashlib
import itertools
import json
import pathlib
import statistics
import subprocess

import study


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('folder', type=pathlib.Path)
    p.add_argument('--build-receipt', type=pathlib.Path, required=True)
    p.add_argument('--output', type=pathlib.Path, required=True)
    a = p.parse_args()
    receipt = json.loads((a.folder / 'receipt.json').read_text())
    assert receipt['kind'] == 'raw-compact-growth-diagnostic'
    assert receipt['failures'] == 0 and receipt['repetitions'] >= 7
    assert digest(a.build_receipt) == receipt['build_receipt_sha256']
    build = json.loads(a.build_receipt.read_text())
    assert receipt['build'] == build
    assert digest(study.ROOT / build['binary']) == build['binary_sha256']
    assert study.source_hash_at(build['source_commit']) == build['source_sha256']
    def git_hash(commit, path):
        data = subprocess.check_output(['git', 'show', f'{commit}:{path}'], cwd=study.ROOT)
        return hashlib.sha256(data).hexdigest()
    assert git_hash(build['source_commit'], 'examples/compact_conversion.rs') == build['example_sha256']
    for path, key in [('tools/conversion_study.py', 'driver_sha256'), ('tools/study.py', 'executor_sha256')]:
        assert git_hash(receipt['driver_source_commit'], path) == receipt[key]
    rows = [json.loads(line) for line in (a.folder / 'runs.jsonl').read_text().splitlines()]
    expected = set(itertools.product(receipt['shapes'], receipt['loads'], receipt['capacities'],
                     receipt['adaptive'], receipt['pin_initial'], range(receipt['repetitions'])))
    keys = ['shape', 'load', 'capacity_before', 'adaptive', 'pin_initial']
    tuples = [tuple(row[k] for k in keys + ['repetition']) for row in rows]
    assert len(tuples) == len(set(tuples)) == len(expected) == receipt['completed'] == receipt['tasks']
    assert set(tuples) == expected
    grouped = {}
    for row in rows:
        capacity = row['capacity_before']
        assert row['verified'] and row['kind'] == 'compact-single-growth'
        assert row['capacity_after'] == capacity * 2
        assert row['records_before'] == capacity * row['load'] // 100
        assert row['word_bytes_before'] == (4 if row['adaptive'] and capacity <= 65536 else 8)
        assert row['word_bytes_after'] == (4 if row['adaptive'] and capacity * 2 <= 65536 else 8)
        assert row['insertion_ns'] > 0
        grouped.setdefault(tuple(row[k] for k in keys), []).append(row['insertion_ns'])
    medians = json.loads((a.folder / 'diagnostic-medians.json').read_text())
    cells = {tuple(c[k] for k in keys): c['raw_insertion_ns_median'] for c in medians['cells']}
    assert len(cells) == len(medians['cells']) == len(grouped)
    assert cells == {k: statistics.median(v) for k, v in grouped.items()}
    output = dict(claim='Exact artifact coverage, input binding and median audit. '
                       'Executable assertions check complete values/old views; parallel raw-growth timings '
                       'do not establish engine throughput, service latency or final acceptance.',
                  processes=len(rows), cells=len(cells), exact_tuple_coverage=True,
                  all_executable_proof_markers_passed=True, frozen_inputs_and_driver_verified=True,
                  medians_independently_recomputed=True, runs_sha256=digest(a.folder / 'runs.jsonl'),
                  source_commit=build['source_commit'], binary_sha256=build['binary_sha256'],
                  auditor_sha256=digest(pathlib.Path(__file__)))
    with a.output.open('x') as stream:
        json.dump(output, stream, indent=2)
        stream.write('\n')
    print(json.dumps(output))


if __name__ == '__main__':
    main()
