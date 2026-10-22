#!/usr/bin/env python3
"""Selected raw-getter baseline. Never a complete snapshot/feed performance gate."""
import concurrent.futures
import hashlib
import itertools
import json
import pathlib
import random
import statistics
import subprocess
import sys
import time

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(ROOT / 'tools'))
import study


def main():
    plan = json.loads((HERE / 'RAW-POINT-PLAN.json').read_text())
    build_path = HERE / 'evidence/point-build-receipt.json'
    build = json.loads(build_path.read_text())
    binary = ROOT / build['binary']
    assert hashlib.sha256(binary.read_bytes()).hexdigest() == build['binary_sha256']
    for path, expected in build['input_sha256'].items():
        data = subprocess.check_output(['git', 'show', f"{build['source_commit']}:research/group-byte-index-proof/{path}"], cwd=ROOT)
        assert hashlib.sha256(data).hexdigest() == expected
    engines = [f'group-{s}-{l}-{g}' for s, l, g in itertools.product(
        plan['policies']['builders'], plan['policies']['loads'], plan['policies']['groups'])] + plan['references']
    tasks = [(binary, e, case, seed, plan['records'], plan['readers'], plan['rounds'],
              False, shape, 0, False) for e, shape, case, seed in itertools.product(
                  engines, plan['shapes'], plan['cases'], plan['seeds'])]
    assert len(tasks) == plan['processes']
    random.Random(0xB17E).shuffle(tasks)
    folder = HERE / 'evidence/point-v1'
    folder.mkdir(exist_ok=False)
    receipt = dict(claim=plan['claim'] + ' Parallel selection, not isolated speed or final acceptance.',
                   plan=plan, build=build, tasks=len(tasks), workers=plan['workers'],
                   driver_sha256=hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),
                   executor_sha256=hashlib.sha256((ROOT / 'tools/study.py').read_bytes()).hexdigest(),
                   driver_source_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
                   build_receipt_sha256=hashlib.sha256(build_path.read_bytes()).hexdigest(),
                   started_at_unix=time.time())
    (folder / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    rows, errors = [], []
    with (folder / 'runs.jsonl').open('x') as out, concurrent.futures.ThreadPoolExecutor(plan['workers']) as pool:
        futures = {pool.submit(study.execute, task): task for task in tasks}
        for f in concurrent.futures.as_completed(futures):
            task = futures[f]
            try:
                row = f.result()
                for k, v in [('engine',task[1]),('case',task[2]),('seed',task[3]),
                             ('records',task[4]),('readers',task[5]),('rounds',task[6]),('shape',task[8])]:
                    assert row[k] == v
                assert row['verified'] and row['kind'] == 'raw-held-map-getter' and row['total_ns'] > 0
                rows.append(row);out.write(json.dumps(row) + '\n');out.flush()
            except Exception as e:
                errors.append(dict(task=list(map(str,task)),error=str(e)))
                (folder / 'errors.json').write_text(json.dumps(errors,indent=2)+'\n')
            if (len(rows)+len(errors)) % 25 == 0:
                print(f'{len(rows)} completed, {len(errors)} failures / {len(tasks)}',flush=True)
    receipt.update(completed=len(rows),failures=len(errors),finished_at_unix=time.time())
    (folder / 'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
    if errors:raise SystemExit(1)
    actual={(r['engine'],r['shape'],r['case'],r['seed'])for r in rows}
    assert len(actual)==len(rows)==len(tasks)
    assert actual==set(itertools.product(engines,plan['shapes'],plan['cases'],plan['seeds']))
    grouped={};checksums={}
    for r in rows:
        key=(r['shape'],r['case'],r['seed'])
        assert checksums.setdefault(key,r['checksum'])==r['checksum']
        grouped.setdefault((r['engine'],r['shape'],r['case']),[]).append(r['total_ns']/r['rounds'])
    medians={k:statistics.median(v)for k,v in grouped.items()}
    cells=[]
    for (engine,shape,case),ns in sorted(medians.items()):
        reference='hashbrown-sip'if '-sip-'in engine or engine in ['std-hash','hashbrown-sip']else'hashbrown'
        cells.append(dict(engine=engine,shape=shape,case=case,ns_per_round_median=ns,
                          reference=reference,ratio_to_reference=ns/medians[(reference,shape,case)]))
    (folder / 'selection-medians.json').write_text(json.dumps(dict(claim=receipt['claim'],cells=cells),indent=2)+'\n')
    print(json.dumps(dict(completed=len(rows),failures=0,cells=len(cells),output_agreement=True)),flush=True)


if __name__ == '__main__':
    main()
