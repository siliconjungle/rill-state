#!/usr/bin/env python3
"""Complete fixed-candidate and matched-policy descriptions, never an acceptance gate."""
import argparse
import hashlib
import json
import pathlib

from describe_shapes import describe


def compare(folder):
    data = describe(folder)  # Requires exact, complete serial tuple coverage.
    cells = {(c['engine'], c['shape'], c['case']): c for c in data['cells']}
    engines = {c['engine'] for c in data['cells']}
    workloads = sorted({(c['shape'], c['case']) for c in data['cells']})
    pairs = []
    for candidate in sorted(engines):
        if candidate != 'hashbrown' and 'hashbrown' in engines:
            pairs.append(('fixed candidate versus baseline', candidate, 'hashbrown'))
        if candidate.startswith('array-owned-'):
            shared = candidate.replace('array-owned-', 'array-', 1)
            if shared in engines:
                pairs.append(('inline versus shared hash builder', candidate, shared))
        if candidate.startswith('inline-owned-'):
            shared = candidate.replace('inline-owned-', 'inline-shared-', 1)
            if shared in engines:
                pairs.append(('inline versus shared hash builder', candidate, shared))
        if candidate.startswith('inline-') and '-index-' in candidate:
            spans = candidate.replace('-index-', '-span-', 1)
            if spans in engines:
                pairs.append(('indexed versus contiguous-span probing', candidate, spans))
        if candidate.startswith('array-') and '-shared-' in candidate:
            copied = candidate.replace('-shared-', '-copy-', 1)
            if copied in engines:
                pairs.append(('shared versus copied canonical key', candidate, copied))
    comparisons = []
    for kind, candidate, reference in pairs:
        rows = []
        for shape, case in workloads:
            current, prior = cells[candidate, shape, case], cells[reference, shape, case]
            ratios = {field: current[field] / prior[field]
                      for field in ['total_ms', 'publish_ms', 'read_ms']
                      if prior[field] > 0}
            rows.append(dict(shape=shape, case=case, candidate_median_ms=current['total_ms'],
                             reference_median_ms=prior['total_ms'], ratios=ratios))
        comparisons.append(dict(kind=kind, candidate=candidate, reference=reference,
                                cells=len(rows),
                                lower_total_median=sum(r['ratios']['total_ms'] < 1 for r in rows),
                                higher_total_median=sum(r['ratios']['total_ms'] > 1 for r in rows),
                                comparisons=rows))
    return dict(claim='Descriptive fixed-candidate medians only; no paired confidence '
                      'interval, oracle envelope, multiple-comparison gate or final acceptance',
                source_commit=data['source_commit'], source_sha256=data['source_sha256'],
                binary_sha256=data['binary_sha256'],
                reporter_sha256=hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),
                comparisons=comparisons)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('folder', type=pathlib.Path)
    parser.add_argument('--output', type=pathlib.Path)
    args = parser.parse_args()
    output = args.output or args.folder / 'layout-comparisons.json'
    data = compare(args.folder)
    with output.open('x') as stream:
        json.dump(data, stream, indent=2)
        stream.write('\n')
    print(json.dumps(dict(output=str(output), pairs=len(data['comparisons']))))


if __name__ == '__main__':
    main()
