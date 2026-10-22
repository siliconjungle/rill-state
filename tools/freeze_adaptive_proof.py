#!/usr/bin/env python3
"""Freeze committed adaptive contract tests before any candidate timings."""
import argparse
import hashlib
import json
import os
import pathlib
import re
import shutil
import subprocess
import time

import study


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('version')
    p.add_argument('--plan', type=pathlib.Path, required=True)
    p.add_argument('--core-log', type=pathlib.Path, required=True)
    p.add_argument('--clippy-log', type=pathlib.Path, required=True)
    p.add_argument('--tests', type=int, required=True)
    a = p.parse_args()
    assert re.fullmatch(r'v[0-9a-z]+', a.version)
    root = study.ROOT
    commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
    source_hash = study.source_hash()
    assert source_hash == study.source_hash_at(commit)
    test_path = root / 'tests/adaptive.rs'
    test_hash = digest(test_path)
    for path in [test_path, a.plan.resolve(), pathlib.Path(__file__).resolve()]:
        assert path.read_bytes() == subprocess.check_output(['git', 'show', f'{commit}:{path.relative_to(root)}'], cwd=root)
    plan = json.loads(a.plan.read_text())
    core_tests = sum(map(int, re.findall(r'test result: ok\. (\d+) passed', a.core_log.read_text())))
    assert core_tests >= a.tests and 'error:' not in a.core_log.read_text()
    assert 'Finished' in a.clippy_log.read_text() and 'error:' not in a.clippy_log.read_text()
    env = dict(os.environ, RUSTUP_TOOLCHAIN='stable', CARGO_BUILD_JOBS='2')
    prepared = json.loads(subprocess.check_output(['ink-cache', 'prepare-sources', str(root), '--offline'], cwd=root, env=env, text=True))
    command = ['cargo', 'test', '--locked', '--no-default-features', '--features', 'experimental', '--test', 'adaptive', '--no-run', '--message-format=json']
    prefix = root / 'evidence' / f'adaptive-contracts-{a.version}'
    start = time.time()
    with pathlib.Path(str(prefix) + '-build.jsonl').open('x') as out, pathlib.Path(str(prefix) + '-build.log').open('x') as err:
        result = subprocess.run(command, cwd=root, env=env, stdout=out, stderr=err)
    assert result.returncode == 0
    assert source_hash == study.source_hash() and test_hash == digest(test_path)
    events = [json.loads(line) for line in pathlib.Path(str(prefix) + '-build.jsonl').read_text().splitlines()]
    artifacts = [e['executable'] for e in events if e.get('reason') == 'compiler-artifact' and e.get('executable') and e['target']['name'] == 'adaptive']
    assert len(artifacts) == 1
    binary = root / 'build' / f'adaptive-contracts-{a.version}'
    with pathlib.Path(artifacts[0]).open('rb') as source, binary.open('xb') as target:
        shutil.copyfileobj(source, target)
    binary.chmod(0o755)
    run = [str(binary), '--test-threads=1']
    result = study.run_bounded(run, timeout=300)
    log = pathlib.Path(str(prefix) + '.log')
    log.write_text(result.stdout + result.stderr)
    assert result.returncode == 0 and f'{a.tests} passed; 0 failed; 0 ignored; 0 measured; 0 filtered out' in result.stdout
    receipt = dict(claim='Raw generic ordering/model/value/failure/thread/reclamation proofs only; complete-engine and speed admission remain separate.',
        source_commit=commit, source_sha256=source_hash, test_source_sha256=test_hash,
        plan=str(a.plan), plan_sha256=digest(a.plan), driver_sha256=digest(pathlib.Path(__file__)),
        source_preflight=prepared, build_command=command, build_exit_code=0,
        binary=str(binary.relative_to(root)), binary_sha256=digest(binary), executed_command=run,
        exit_code=result.returncode, policy_count=plan['proof_before_timing']['raw_policies'],
        passed_adaptive_tests=a.tests, all_core_experimental_tests=core_tests,
        reader_modes=['inline','shared','cache','reuse','reusecache', *plan['ordering']['policies']],
        reader_records=20000, reader_threads=4, threaded_cache_roots=['flat','paged'],
        scope='All raw policy model cases use the forced-collision builder. Four-reader probes cover nine representative modes with RandomState, not every policy/configuration.',
        logs_sha256={str(path.relative_to(root)): digest(path) for path in [a.core_log.resolve(), a.clippy_log.resolve(), log]},
        rustc=subprocess.check_output(['rustc','-Vv'],env=env,text=True),
        environment={key:env[key] for key in ['RUSTUP_TOOLCHAIN','CARGO_BUILD_JOBS']},
        started_at_unix=start, finished_at_unix=time.time())
    with pathlib.Path(str(prefix) + '-receipt.json').open('x') as stream:
        json.dump(receipt, stream, indent=2)
        stream.write('\n')
    print(json.dumps(dict(tests=a.tests, policies=receipt['policy_count'], binary_sha256=receipt['binary_sha256'])), flush=True)


if __name__ == '__main__':
    main()
