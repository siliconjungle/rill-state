#!/usr/bin/env python3
"""Describe a complete serial development study; never a superiority gate."""
import argparse
import collections
import json
import pathlib
import statistics


def describe(folder):
    receipt = json.loads((folder / 'receipt.json').read_text())
    assert receipt['mode'] == 'serial' and receipt['workers'] == 1
    assert receipt['output_agreement'] and receipt['errors'] == 0
    rows = [json.loads(line) for line in (folder / 'runs.jsonl').read_text().splitlines()]
    assert len(rows) == receipt['tasks'] == receipt['completed']
    expected = {(engine, shape, case, seed, rep)
                for engine in receipt['engines'] for shape in receipt['shapes']
                for case in receipt['cases'] for seed in receipt['seeds']
                for rep in range(receipt['repetitions'])}
    actual = [(r['engine'], r['shape'], r['case'], r['seed'], r['repetition']) for r in rows]
    assert len(set(actual)) == len(actual) and set(actual) == expected
    groups = collections.defaultdict(list)
    for row in rows:
        assert row['rounds'] == receipt['rounds'] and row['total_ns'] > 0
        groups[(row['shape'], row['case'], row['engine'])].append(row)
    cells = []
    for (shape, case, engine), values in sorted(groups.items()):
        per_round = lambda field: statistics.median(r[field] / r['rounds'] / 1e6 for r in values)
        cells.append(dict(shape=shape, case=case, engine=engine, processes=len(values),
                          total_ms=per_round('total_ns'), publish_ms=per_round('publish_ns'),
                          read_ms=per_round('read_ns'),
                          process_peak_rss_median=statistics.median(r['max_rss_bytes'] for r in values)))
    baselines = {(c['shape'], c['case']): c for c in cells if c['engine'] == 'hashbrown'}
    for cell in cells:
        baseline = baselines.get((cell['shape'], cell['case']))
        if baseline:
            cell['ratio_to_hashbrown'] = cell['total_ms'] / baseline['total_ms']
    return dict(claim='Descriptive serial development medians; not paired confidence, '
                      'multiple-comparison corrected superiority or final acceptance',
                memory='Peak RSS includes the full process fixture, inputs, oracle and output storage; '
                       'not library-only heap memory',
                source_commit=receipt['source_commit'], source_sha256=receipt['source_sha256'],
                binary_sha256=receipt['binary_sha256'], cells=cells)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('folder', type=pathlib.Path)
    args = parser.parse_args()
    output = args.folder / 'descriptive-medians.json'
    data = describe(args.folder)
    with output.open('x') as stream:
        json.dump(data, stream, indent=2)
        stream.write('\n')
    print(json.dumps(dict(output=str(output), cells=len(data['cells']))))


if __name__ == '__main__':
    main()
