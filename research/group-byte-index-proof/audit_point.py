#!/usr/bin/env python3
"""Independent raw-getter artifact and unsigned-checksum audit, not a speed gate."""
import argparse
import hashlib
import itertools
import json
import pathlib
import statistics
import subprocess

ROOT = pathlib.Path(__file__).resolve().parents[2]
MASK = (1 << 64) - 1


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def git_hash(commit, path):
    return hashlib.sha256(subprocess.check_output(['git','show',f'{commit}:{path}'],cwd=ROOT)).hexdigest()


def checksum(seed, records, readers, rounds, case):
    def rol(x, n):return ((x << n) | (x >> (64-n))) & MASK
    total = 0
    miss = {'read':0,'miss50':50,'miss100':100}[case]
    for i in range(readers):
        seed ^= (seed << 13) & MASK
        seed ^= seed >> 7
        seed ^= (seed << 17) & MASK
        key = seed % records
        if i % 100 >= miss:
            value = rol(key,7) ^ (((~key & MASK)*19)&MASK) ^ rol(rol(key,13),23) ^ ((key*317)&MASK)
            total = (total + value) & MASK
    return (total * rounds) & MASK


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('folder',type=pathlib.Path)
    p.add_argument('--build-receipt',type=pathlib.Path,required=True)
    p.add_argument('--output',type=pathlib.Path,required=True)
    a=p.parse_args();r=json.loads((a.folder/'receipt.json').read_text());plan=r['plan']
    build=json.loads(a.build_receipt.read_text());assert r['build']==build
    assert digest(a.build_receipt)==r['build_receipt_sha256']
    assert digest(ROOT/build['binary'])==build['binary_sha256']
    for path,h in build['input_sha256'].items():
        assert git_hash(build['source_commit'],'research/group-byte-index-proof/'+path)==h
    assert git_hash(r['driver_source_commit'],'research/group-byte-index-proof/run_point.py')==r['driver_sha256']
    assert git_hash(r['driver_source_commit'],'tools/study.py')==r['executor_sha256']
    engines=[f'group-{s}-{l}-{g}'for s,l,g in itertools.product(plan['policies']['builders'],plan['policies']['loads'],plan['policies']['groups'])]+plan['references']
    expected=set(itertools.product(engines,plan['shapes'],plan['cases'],plan['seeds']))
    rows=[json.loads(line)for line in (a.folder/'runs.jsonl').read_text().splitlines()]
    tuples=[tuple(row[k]for k in ['engine','shape','case','seed'])for row in rows]
    assert r['failures']==0 and len(tuples)==len(set(tuples))==len(expected)==r['tasks']==r['completed']==plan['processes']
    assert set(tuples)==expected
    models={(seed,case):checksum(seed,plan['records'],plan['readers'],plan['rounds'],case)for seed,case in itertools.product(plan['seeds'],plan['cases'])}
    grouped={}
    for row in rows:
        assert row['verified'] and row['kind']=='raw-held-map-getter' and row['total_ns']>0
        for key in ['records','readers','rounds']:assert row[key]==plan[key]
        assert row['checksum']==models[(row['seed'],row['case'])]
        grouped.setdefault(tuple(row[k]for k in ['engine','shape','case']),[]).append(row['total_ns']/row['rounds'])
    medians={k:statistics.median(v)for k,v in grouped.items()}
    cells=json.loads((a.folder/'selection-medians.json').read_text())['cells']
    assert len(cells)==len(medians)
    for cell in cells:
        key=tuple(cell[k]for k in ['engine','shape','case'])
        assert cell['ns_per_round_median']==medians[key]
        assert cell['ratio_to_reference']==medians[key]/medians[(cell['reference'],cell['shape'],cell['case'])]
    result=dict(claim='Complete raw-getter tuple/source/binary audit and independent unsigned checksum/median recomputation. Parallel timing is selection only; no engine/feed or final performance acceptance.',
                processes=len(rows),cells=len(cells),exact_tuple_coverage=True,all_executable_markers_passed=True,
                all_independent_model_checksums_match=True,frozen_inputs_and_driver_verified=True,
                rows_sha256=digest(a.folder/'runs.jsonl'),auditor_sha256=digest(pathlib.Path(__file__)),
                source_commit=build['source_commit'],binary_sha256=build['binary_sha256'])
    with a.output.open('x')as stream:json.dump(result,stream,indent=2);stream.write('\n')
    print(json.dumps(result))


if __name__=='__main__':main()
