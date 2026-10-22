#!/usr/bin/env python3
"""Execute every registered native MVCC adapter edge probe in fresh processes."""
import argparse
import concurrent.futures
import hashlib
import json
import pathlib
import subprocess

import study


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--binary', type=pathlib.Path, required=True)
    p.add_argument('--plan', type=pathlib.Path, required=True)
    p.add_argument('--label', required=True)
    a = p.parse_args()
    assert a.label.replace('-', '').isalnum()
    root = study.ROOT
    commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
    for path in [a.plan.resolve(), pathlib.Path(__file__).resolve()]:
        assert path.read_bytes() == subprocess.check_output(['git', 'show', f'{commit}:{path.relative_to(root)}'], cwd=root)
    plan = json.loads(a.plan.read_text())
    engines = plan['extra_contracts']['engines']
    assert len(engines) == len(set(engines)) == 8
    binding = study.build_binding(a.binary)
    folder = root / 'evidence' / a.label
    folder.mkdir(exist_ok=False)
    receipt = dict(claim='Eight actual native scalar adapter edge proofs. No generic-payload, heap, speed, native transferable-session or sockets admission.',
        plan=str(a.plan), plan_sha256=digest(a.plan), build=binding,
        driver_source_commit=commit, driver_sha256=digest(pathlib.Path(__file__)),
        executor_sha256=digest(root / 'tools/study.py'), engines=engines, tasks=len(engines))
    (folder / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    def execute(engine):
        command = [str(a.binary.resolve()), '--engine', engine, plan['extra_contracts']['argument']]
        result = study.run_bounded(command, timeout=300)
        assert result.returncode == 0, (engine, result.stdout, result.stderr)
        row = json.loads(result.stdout)
        assert row == dict(engine=engine, extra_contracts=True, verified=True,
                           revisions=plan['extra_contracts']['revisions'])
        return dict(row, command=command, exit_code=result.returncode,
                    expected_caught_panics_stderr=result.stderr)
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
        rows = list(pool.map(execute, engines))
    assert {r['engine'] for r in rows} == set(engines)
    runs = folder / 'runs.jsonl'
    with runs.open('x') as out:
        for row in rows:
            out.write(json.dumps(row) + '\n')
    receipt.update(completed=len(rows), complete=True, failures=0, runs_sha256=digest(runs))
    (folder / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print(json.dumps(dict(engines=len(rows), edge_probes_passed=True)))


if __name__ == '__main__':
    main()
