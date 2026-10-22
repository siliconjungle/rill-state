#!/usr/bin/env python3
"""Describe every matched copying/transfer tuple; never a final speed gate."""
import argparse
import hashlib
import itertools
import json
import pathlib
import statistics


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('folder', type=pathlib.Path)
    parser.add_argument('--audit', type=pathlib.Path, required=True)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    a = parser.parse_args()
    receipt = json.loads((a.folder / 'receipt.json').read_text())
    audit = json.loads(a.audit.read_text())
    assert receipt['complete'] and receipt['failures'] == 0
    assert receipt['timing_eligible'] and audit['timing_eligible']
    assert audit['exact_tuple_coverage'] and audit['independently_rebuilt_trace_checksums']
    assert audit['runs_sha256'] == digest(a.folder / 'runs.jsonl')
    rows = [json.loads(line) for line in (a.folder / 'runs.jsonl').read_text().splitlines()]
    index = {(r['engine'], r['shape'], r['seed'], r['repetition'], r['step']): r for r in rows}
    assert len(index) == len(rows) == receipt['tasks'] * 16
    pairs = [(engine, engine.replace('adaptive-shared-', 'adaptive-reuse-', 1)
              if engine.startswith('adaptive-shared-') else
              engine.replace('adaptive-cache-', 'adaptive-reusecache-', 1))
             for engine in receipt['engines']
             if engine.startswith(('adaptive-shared-', 'adaptive-cache-'))]
    assert len(pairs) == 16 and all(new in receipt['engines'] for _, new in pairs)
    cells = []
    for (old, new), shape, step in itertools.product(pairs, receipt['shapes'], range(16)):
        matched = [(index[(old, shape, seed, rep, step)], index[(new, shape, seed, rep, step)])
                   for seed, rep in itertools.product(receipt['seeds'], range(receipt['repetitions']))]
        for left, right in matched:
            for field in ['checksum', 'phase', 'operations', 'before_paged', 'after_paged',
                          'cache_after_scan1', 'cache_after_scan2', 'cache_after_scan3']:
                assert left[field] == right[field]
        row = matched[0][0]
        direction = ('flat-to-paged' if row['after_paged'] else 'paged-to-flat') if row['before_paged'] != row['after_paged'] else 'unchanged'
        fields = {}
        for field in ['batch_ns', 'scan1_ns', 'scan2_ns', 'scan3_ns']:
            before = [left[field] for left, _ in matched]
            after = [right[field] for _, right in matched]
            baseline = statistics.median(before)
            fields[field] = dict(copying_median_ns=baseline,
                transfer_median_ns=statistics.median(after),
                transfer_to_copying_medians=statistics.median(after) / baseline if baseline else None,
                median_logical_pair_ratio=statistics.median([right / left for left, right in zip(before, after)]) if baseline else None,
                pairs_with_lower_transfer=sum(right < left for left, right in zip(before, after)))
        cells.append(dict(copying=old, transfer=new, shape=shape, step=step,
                          phase=row['phase'], transition=direction, matched_seed_repetitions=len(matched), fields=fields))
    result = dict(claim='Complete descriptive development grid. Fresh processes overlap other work; matching seed/repetition does not make timings isolated or confidence-qualified. No per-cell oracle, universal winner, capacity claim or default selection.',
        source_commit=receipt['build']['source_commit'], binary_sha256=receipt['build']['binary_sha256'],
        audit=str(a.audit), audit_sha256=digest(a.audit), runs_sha256=audit['runs_sha256'],
        reporter_sha256=digest(pathlib.Path(__file__)), fixed_policy_pairs=len(pairs),
        shapes=receipt['shapes'], pairs_per_cell=len(receipt['seeds']) * receipt['repetitions'],
        cells=cells)
    with a.output.open('x') as stream:
        json.dump(result, stream, indent=2)
        stream.write('\n')
    print(json.dumps(dict(output=str(a.output), fixed_pairs=len(pairs), cells=len(cells))))


if __name__ == '__main__':
    main()
