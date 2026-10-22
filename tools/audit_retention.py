#!/usr/bin/env python3
"""Audit exact retention coverage and frozen inputs, not performance superiority."""
import argparse
import hashlib
import itertools
import json
import pathlib

from audit_proofs import file_hash, git_bytes, source_at
from retention_study import validate_row


def audit(folder, binary, build_path):
    receipt = json.loads((folder / 'receipt.json').read_text())
    binding = json.loads(build_path.read_text())
    assert receipt['complete'] and receipt['errors'] == 0
    assert receipt['build'] == binding
    assert file_hash(str(build_path)) == receipt['build_receipt_sha256']
    assert file_hash(str(binary)) == binding['binary_sha256']
    assert source_at(binding['source_commit']) == binding['source_sha256']
    assert hashlib.sha256(git_bytes(binding['source_commit'],
                                   'examples/retention_profile.rs')).hexdigest() == binding['example_sha256']
    for field, path in [('driver_sha256', 'tools/retention_study.py'),
                        ('executor_sha256', 'tools/study.py')]:
        assert hashlib.sha256(git_bytes(receipt['driver_source_commit'], path)).hexdigest() == receipt[field]
    raw = (folder / 'runs.jsonl').read_bytes()
    rows = [json.loads(line) for line in raw.splitlines()]
    expected = set(itertools.product(receipt['engines'], receipt['windows'], [False, True], [False, True]))
    actual = [(r['engine'], r['window'], r['pin_initial'], r['mixed']) for r in rows]
    assert len(actual) == len(set(actual)) == len(expected) == receipt['tasks'] == receipt['completed']
    assert set(actual) == expected
    for row in rows:
        for field in ['records', 'changed', 'rounds']:
            assert row[field] == receipt[field]
        assert row.get('scans_per_revision',0)==receipt.get('scans_per_revision',0)
        validate_row(row)
    return dict(folder=str(folder), rows=len(rows), exact_tuple_coverage=True,
                executable_ownership_and_reclamation_markers_passed=True,
                exact_journal_counts=True, frozen_inputs_and_driver_verified=True,
                rows_sha256=hashlib.sha256(raw).hexdigest(),
                source_commit=binding['source_commit'], binary_sha256=binding['binary_sha256'])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('folders', type=pathlib.Path, nargs='+')
    parser.add_argument('--binary', type=pathlib.Path, required=True)
    parser.add_argument('--build-receipt', type=pathlib.Path, required=True)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    args = parser.parse_args()
    matrices = [audit(folder, args.binary, args.build_receipt) for folder in args.folders]
    result = dict(claim='Artifact coverage and binding audit; executable assertions check retained '
                        'values and ownership. Instrumented payloads and process RSS are not '
                        'library-only heap bounds, speed acceptance or universal superiority.',
                  auditor_sha256=file_hash(str(pathlib.Path(__file__).resolve())),
                  matrices=matrices, completed_processes=sum(r['rows'] for r in matrices))
    with args.output.open('x') as stream:
        json.dump(result, stream, indent=2)
        stream.write('\n')
    print(json.dumps(dict(matrices=len(matrices), processes=result['completed_processes'],
                          exact_coverage_and_binding=True)))


if __name__ == '__main__':
    main()
