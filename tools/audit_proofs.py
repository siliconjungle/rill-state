#!/usr/bin/env python3
"""Independently audit complete frozen proof matrices, input bindings and markers."""
import argparse
import functools
import hashlib
import itertools
import json
import pathlib
import subprocess

import study


@functools.cache
def source_at(commit):
    return study.source_hash_at(commit)


@functools.cache
def git_bytes(commit, path):
    return subprocess.check_output(['git', 'show', commit + ':' + path], cwd=study.ROOT)


@functools.cache
def file_hash(path):
    return hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()


def audit(folder):
    receipt = json.loads((folder / 'receipt.json').read_text())
    assert receipt['mode'] in ('prove', 'thread')
    assert receipt.get('finished_at_unix') and receipt['output_agreement']
    assert receipt.get('errors', receipt.get('failures', -1)) == 0
    raw = (folder / 'runs.jsonl').read_bytes()
    rows = [json.loads(line) for line in raw.splitlines()]
    shapes = receipt.get('shapes', [None])
    expected = set(itertools.product(receipt['engines'], shapes, receipt['cases'],
                                     receipt['seeds'], range(receipt['repetitions'])))
    actual = [(r['engine'], r.get('shape'), r['case'], r['seed'], r['repetition']) for r in rows]
    assert len(rows) == len(actual) == len(set(actual)) == len(expected)
    assert len(rows) == receipt['tasks'] == receipt['completed'] and set(actual) == expected
    for row in rows:
        assert row['verified'] is True
        if receipt['mode'] == 'thread':
            assert row['thread_checked'] is True and row['threads'] == 4
        for field in ['records', 'readers', 'rounds']:
            assert row[field] == receipt[field], (folder, field)
    binding_path = study.ROOT / receipt['build_binding']
    binding = json.loads(binding_path.read_text())
    assert file_hash(str(binding_path)) == receipt['build_binding_sha256']
    assert binding['source_commit'] == receipt['source_commit']
    assert source_at(binding['source_commit']) == binding['source_sha256'] == receipt['source_sha256']
    assert binding['rustc'] == receipt['rustc']
    binaries = [study.ROOT / p for p, digest in binding['binaries'].items()
                if digest == receipt['binary_sha256']]
    assert len(binaries) == 1 and file_hash(str(binaries[0])) == receipt['binary_sha256']
    commit = receipt['driver_source_commit']
    for field, path in [('driver_sha256', 'tools/shapes_study.py'),
                        ('executor_sha256', 'tools/study.py'),
                        ('orchestrator_sha256', 'tools/study.py')]:
        if field in receipt:
            assert hashlib.sha256(git_bytes(commit, path)).hexdigest() == receipt[field], (folder, field)
    return dict(folder=str(folder), mode=receipt['mode'], rows=len(rows),
                exact_tuple_coverage=True, all_executable_proof_markers_passed=True,
                frozen_inputs_and_driver_verified=True,
                rows_sha256=hashlib.sha256(raw).hexdigest(),
                source_commit=receipt['source_commit'], binary_sha256=receipt['binary_sha256'])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('folders', type=pathlib.Path, nargs='+')
    parser.add_argument('--output', type=pathlib.Path)
    args = parser.parse_args()
    results = [audit(folder) for folder in args.folders]
    result = dict(claim='Artifact coverage and binding audit. Internal executable assertions '
                        'supply behavioral proofs; this does not establish performance or final acceptance.',
                  auditor_sha256=file_hash(str(pathlib.Path(__file__).resolve())),
                  matrices=results, completed_proof_processes=sum(r['rows'] for r in results))
    if args.output:
        with args.output.open('x') as stream:
            json.dump(result, stream, indent=2)
            stream.write('\n')
    print(json.dumps(dict(matrices=len(results), processes=result['completed_proof_processes'],
                          exact_coverage_and_binding=True)))


if __name__ == '__main__':
    main()
