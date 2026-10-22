#!/usr/bin/env python3
"""Run the registered cold and retention cohorts, followed by exact artifact audits."""
import argparse
import hashlib
import json
import os
import pathlib
import re
import subprocess
import sys
import time

import study


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--version', required=True)
    p.add_argument('--cohort', choices=['smoke', 'full'], required=True)
    p.add_argument('--list', action='store_true')
    a = p.parse_args()
    assert re.fullmatch(r'v[0-9a-z]+', a.version)
    plan_path = pathlib.Path('research/v30-diagnostics-plan.json')
    plan = json.loads((study.ROOT / plan_path).read_text())
    cold_label = f'adaptive-cold-{a.version}'
    cold_build = f'evidence/adaptive-cold-{a.version}-build-receipt.json'
    memory_build = f'evidence/retention-profile-{a.version}-build-receipt.json'
    stages = [[sys.executable, 'tools/adaptive_cold_study.py',
               '--binary', f'build/adaptive-cold-{a.version}', '--build-receipt', cold_build,
               '--plan', str(plan_path), '--label', cold_label, '--cohort', a.cohort, '--workers', '2'],
              [sys.executable, 'tools/audit_adaptive_cold.py', f'evidence/{cold_label}',
               '--build-receipt', cold_build, '--output', f'evidence/{cold_label}-audit.json']]
    memory_folders = []
    memory = plan['retention'][a.cohort]
    for scans in plan['retention']['scans_per_revision']:
        label = f'retention-{a.version}-scans{scans}'
        memory_folders.append(f'evidence/{label}')
        stages.append([sys.executable, 'tools/retention_study.py',
                       '--binary', f'build/retention-profile-{a.version}', '--build-receipt', memory_build,
                       '--label', label, '--engines', ','.join(plan['retention']['engines']),
                       '--records', str(memory['records']), '--changed', str(memory['changed']),
                       '--rounds', str(memory['rounds']), '--windows', ','.join(map(str, memory['windows'])),
                       '--scans', str(scans), '--workers', '1'])
    stages.append([sys.executable, 'tools/audit_retention.py', *memory_folders,
                   '--binary', f'build/retention-profile-{a.version}', '--build-receipt', memory_build,
                   '--output', f'evidence/retention-{a.version}-audit.json'])
    if a.list:
        print(json.dumps(stages, indent=2))
        return
    commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=study.ROOT, text=True).strip()
    for path in [str(plan_path), 'tools/planned_diagnostics.py']:
        assert subprocess.check_output(['git', 'show', f'{commit}:{path}'], cwd=study.ROOT) == (study.ROOT / path).read_bytes()
    for path in [cold_build, memory_build]:
        binding = json.loads((study.ROOT / path).read_text())
        if a.cohort == 'full':
            assert 'completed_cargo_dependencies' not in binding
    output = study.ROOT / 'evidence' / f'planned-diagnostics-{a.version}-receipt.json'
    receipt = dict(plan=str(plan_path), plan_sha256=hashlib.sha256((study.ROOT / plan_path).read_bytes()).hexdigest(),
                   runner_sha256=hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),
                   driver_source_commit=commit, cohort=a.cohort,
                   claim='Registered component diagnostics; no final engine performance admission.',
                   stages=stages, completed=[], started_at_unix=time.time())
    with output.open('x') as stream:
        json.dump(receipt, stream, indent=2)
        stream.write('\n')
    for index, command in enumerate(stages):
        print(json.dumps(dict(starting=index, command=command)), flush=True)
        result = subprocess.run(command, cwd=study.ROOT, env=dict(os.environ, RUSTUP_TOOLCHAIN='stable'))
        receipt['last_exit_code'] = result.returncode
        receipt['updated_at_unix'] = time.time()
        if result.returncode == 0:
            receipt['completed'].append(index)
        output.write_text(json.dumps(receipt, indent=2) + '\n')
        if result.returncode:
            raise SystemExit(result.returncode)
    receipt['finished_at_unix'] = time.time()
    output.write_text(json.dumps(receipt, indent=2) + '\n')


if __name__ == '__main__':
    main()
