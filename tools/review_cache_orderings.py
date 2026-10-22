#!/usr/bin/env python3
"""Retain every fixed ordering/reuse contrast from a complete registered raw study."""
import argparse
import hashlib
import itertools
import json
import pathlib
import statistics


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('folder', type=pathlib.Path)
    p.add_argument('--audit', type=pathlib.Path, required=True)
    p.add_argument('--output', type=pathlib.Path, required=True)
    a = p.parse_args()
    receipt = json.loads((a.folder / 'receipt.json').read_text())
    audit = json.loads(a.audit.read_text())
    plan_path = pathlib.Path(receipt['plan'])
    plan = json.loads(plan_path.read_text())
    assert digest(plan_path) == receipt['plan_sha256']
    assert receipt['complete'] and receipt['failures'] == 0 and receipt['timing_eligible']
    assert audit['timing_eligible'] and audit['exact_tuple_coverage']
    assert audit['independently_rebuilt_trace_checksums'] and audit['representation_and_cache_states']
    assert audit['runs_sha256'] == digest(a.folder / 'runs.jsonl')
    rows = [json.loads(line) for line in (a.folder / 'runs.jsonl').read_text().splitlines()]
    index = {(r['engine'], r['shape'], r['seed'], r['repetition'], r['step']): r for r in rows}
    assert len(index) == len(rows) == receipt['tasks'] * 16
    pairs = []
    for engine in receipt['engines']:
        if not engine.startswith('adaptive-'):
            continue
        mode = engine.split('-')[1]
        if mode not in plan['ordering']['policies']:
            continue
        baseline = 'reusecache' if plan['ordering']['policies'][mode]['reuse_records'] else 'cache'
        pairs.append(('ordering', engine.replace(f'adaptive-{mode}-', f'adaptive-{baseline}-', 1), engine))
        if mode.startswith('reuse'):
            pairs.append(('reuse-with-same-order', engine.replace('adaptive-reuse', 'adaptive-copy', 1), engine))
        if mode.endswith('bucket'):
            pairs.append(('bucket-versus-exact-order', engine.replace('bucket-', 'addr-', 1), engine))
    assert len(pairs) == 64 and len(set(pairs)) == len(pairs)
    assert all(left in receipt['engines'] and right in receipt['engines'] for _, left, right in pairs)
    cells = []
    for (factor, left, right), shape, step in itertools.product(pairs, receipt['shapes'], range(16)):
        matched = [(index[(left, shape, seed, rep, step)], index[(right, shape, seed, rep, step)])
                   for seed, rep in itertools.product(receipt['seeds'], range(receipt['repetitions']))]
        for before, after in matched:
            for field in ['checksum', 'phase', 'operations', 'before_paged', 'after_paged',
                          'cache_after_scan1', 'cache_after_scan2', 'cache_after_scan3']:
                assert before[field] == after[field]
        fields = {}
        for field in ['batch_ns', 'scan1_ns', 'scan2_ns', 'scan3_ns']:
            before = [l[field] for l, _ in matched]
            after = [r[field] for _, r in matched]
            baseline = statistics.median(before)
            fields[field] = dict(baseline_median_ns=baseline,
                candidate_median_ns=statistics.median(after),
                ratio_of_development_medians=statistics.median(after) / baseline if baseline else None,
                median_matching_seed_rep_ratio=statistics.median([r/l for l,r in zip(before,after)]) if baseline else None,
                lower_candidate_samples=sum(r<l for l,r in zip(before,after)))
        cells.append(dict(factor=factor, baseline=left, candidate=right, shape=shape, step=step,
                          phase=matched[0][0]['phase'], logical_seed_repetitions=len(matched), fields=fields))
    assert len(cells) == 6144
    result = dict(claim='Every predeclared fixed contrast from a complete parallel component study. Logical seed/repetition matching does not establish isolated paired confidence. No per-case policy, universal winner, total heap, allocator-locality, connection-capacity or default-switch claim.',
        source_commit=receipt['build']['source_commit'], binary_sha256=receipt['build']['binary_sha256'],
        plan=receipt['plan'], plan_sha256=receipt['plan_sha256'],
        audit=str(a.audit), audit_sha256=digest(a.audit), runs_sha256=audit['runs_sha256'],
        reporter_sha256=digest(pathlib.Path(__file__)), fixed_contrasts=len(pairs), cells=cells)
    with a.output.open('x') as out:
        json.dump(result, out, indent=2)
        out.write('\n')
    print(json.dumps(dict(output=str(a.output), contrasts=len(pairs), cells=len(cells))))


if __name__ == '__main__':
    main()
