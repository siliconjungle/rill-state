#!/usr/bin/env python3
"""Run a predeclared development panel through fresh-process proof/screen/serial stages.

This is orchestration, not the final holdout acceptance gate. Existing evidence
labels are never overwritten. Inspect a terminal failed stage before resuming it
manually; an observation timeout is not evidence that a stage has stopped.
"""
import argparse
import hashlib
import json
import os
import pathlib
import subprocess
import sys
import time

import study


def stages(plan, version, workers):
    result = []
    for layer, driver, references in [
        ('state', 'tools/study.py', plan['scalar_references']),
        ('shapes', 'tools/shapes_study.py', plan['generic_references']),
    ]:
        broad = list(dict.fromkeys(plan.get('broad_candidates', plan.get('all_small_proof_shards', [])) + references))
        selected = list(dict.fromkeys(plan.get('selected_candidates', plan.get('selected_fixed_shards', [])) + references))
        assert broad and selected
        for phase, mode, engines, settings in [
            ('proof-small', 'prove', broad, plan['proof']['small']),
            ('proof-20k', 'prove', selected, plan['proof']['full_selected']),
            ('threaded-20k', 'thread', selected, plan['proof']['thread_selected']),
            ('screen', 'screen', broad, plan['proof']['full_selected']),
            ('serial', 'serial', selected, dict(
                records=20000, readers=20000,
                rounds=plan['timing']['scalar_rounds' if layer == 'state' else 'generic_rounds'],
                repetitions=plan['timing']['serial_repetitions'])),
        ]:
            label = f'{layer}-{version}-{phase}'
            argv = [sys.executable, driver, mode, '--binary', f'build/{layer}-{version}',
                    '--label', label, '--engines', ','.join(engines),
                    '--seeds', ','.join(map(str, plan['seeds'])), '--workers', str(workers)]
            for name, value in settings.items():
                argv += ['--' + name, ','.join(value) if isinstance(value, list) else str(value)]
            result.append(dict(layer=layer, phase=phase, label=label, argv=argv))
    # All proofs, then all screens, then the two serial panels. Never overlap
    # this runner's proofs/screens with its serial measurements.
    order = ['proof-small', 'proof-20k', 'threaded-20k', 'screen', 'serial']
    return sorted(result, key=lambda stage: order.index(stage['phase']))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('plan', type=pathlib.Path)
    parser.add_argument('--workers', type=int, default=2)
    parser.add_argument('--phases', help='Optional comma-separated phase subset; other required proofs remain pending')
    parser.add_argument('--list', action='store_true', help='Inspect exact stages without launching')
    args = parser.parse_args()
    assert 1 <= args.workers <= 4
    plan_bytes = args.plan.read_bytes()
    plan = json.loads(plan_bytes)
    version = plan['source_binary_version']
    assert version.isalnum()
    commands = stages(plan, version, args.workers)
    if args.phases:
        phases = args.phases.split(',')
        assert len(set(phases)) == len(phases)
        assert set(phases) <= {s['phase'] for s in commands}
        commands = [s for s in commands if s['phase'] in phases]
    assert len({s['label'] for s in commands}) == len(commands)
    if args.list:
        print(json.dumps(commands, indent=2))
        return
    for stage in commands:
        assert not (study.ROOT / 'evidence' / stage['label']).exists(), stage['label']
    bindings = {layer: study.build_binding(study.ROOT / f'build/{layer}-{version}')
                for layer in ['state', 'shapes']}
    receipt = dict(plan=str(args.plan), plan_sha256=hashlib.sha256(plan_bytes).hexdigest(),
                   runner_sha256=hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),
                   driver_source_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
                   claim='Development stages only; holdout acceptance not implied',
                   bindings=bindings, stages=commands, completed=[], started_at_unix=time.time())
    suffix = '-' + '-'.join(dict.fromkeys(s['phase'] for s in commands)) if args.phases else ''
    output = study.ROOT / 'evidence' / f'planned-{version}{suffix}-receipt.json'
    with output.open('x') as stream:
        json.dump(receipt, stream, indent=2)
        stream.write('\n')
    for stage in commands:
        print(json.dumps(dict(starting=stage['label'])), flush=True)
        completed = subprocess.run(stage['argv'], cwd=study.ROOT,
                                   env=dict(os.environ, RUSTUP_TOOLCHAIN='stable'))
        receipt['last_exit_code'] = completed.returncode
        if completed.returncode == 0:
            receipt['completed'].append(stage['label'])
        receipt['updated_at_unix'] = time.time()
        output.write_text(json.dumps(receipt, indent=2) + '\n')
        if completed.returncode:
            raise SystemExit(completed.returncode)
    receipt['finished_at_unix'] = time.time()
    output.write_text(json.dumps(receipt, indent=2) + '\n')


if __name__ == '__main__':
    main()
