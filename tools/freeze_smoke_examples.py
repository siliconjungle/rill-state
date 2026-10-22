#!/usr/bin/env python3
"""Untimed smoke-only builds using completed, source-bound Cargo library artifacts.

Does not acquire Cargo's output lock, mutate its artifacts or supply timing evidence.
The separate Cargo-built optimized diagnostics remain required for measurements.
"""
import hashlib
import json
import os
import pathlib
import subprocess
import time

import study


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    root = study.ROOT
    commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
    progress = json.loads((root / 'evidence/build-v30-progress.json').read_text())
    assert study.source_hash() == study.source_hash_at(commit) == progress['source_sha256']
    assert study.source_hash_at(progress['source_commit']) == progress['source_sha256']
    events = [json.loads(line) for line in (root / 'evidence/build-v30.jsonl').read_text().splitlines()]
    names = {'rill_state': '#0.1.0', 'hashbrown': '#hashbrown@0.17.1', 'imbl': '#imbl@7.0.2'}
    direct = {}
    dependencies = []
    for event in events:
        if event.get('reason') != 'compiler-artifact':
            continue
        for filename in event['filenames']:
            if filename.endswith('.rlib'):
                dependencies.append(dict(path=filename, sha256=digest(pathlib.Path(filename)),
                    package_id=event['package_id'], features=event['features']))
                name = event['target']['name']
                if name in names and event['package_id'].endswith(names[name]):
                    assert name not in direct
                    direct[name] = filename
    assert set(direct) == set(names)
    env = dict(os.environ, RUSTUP_TOOLCHAIN='stable')
    compiler = subprocess.check_output(['rustc', '-Vv'], env=env, text=True)
    assert compiler == progress['rustc']
    prepared = json.loads(subprocess.check_output(
        ['ink-cache', 'prepare-sources', str(root), '--offline'], cwd=root, env=env, text=True))
    for example in ['adaptive_cold', 'retention_profile']:
        source = root / 'examples' / f'{example}.rs'
        source_bytes = subprocess.check_output(['git', 'show', f'{commit}:examples/{example}.rs'], cwd=root)
        assert source.read_bytes() == source_bytes
        binary = root / 'build' / f'{example.replace("_", "-")}-v30smoke'
        receipt = root / 'evidence' / f'{example.replace("_", "-")}-v30smoke-build-receipt.json'
        assert not binary.exists() and not receipt.exists()
        command = ['rustc', '--edition=2021', '--crate-name', example,
                   '--cfg', 'feature="comparisons"', '-C', 'opt-level=0',
                   '-C', 'debuginfo=0', '-C', 'embed-bitcode=no',
                   '-L', f'dependency={root / "target/release/deps"}']
        for name, path in direct.items():
            command += ['--extern', f'{name}={path}']
        command += [str(source), '-o', str(binary)]
        started = time.time()
        completed = subprocess.run(command, cwd=root, env=env, capture_output=True, text=True)
        log = root / 'evidence' / f'{example.replace("_", "-")}-v30smoke-build.log'
        log.write_text(completed.stdout + completed.stderr)
        assert completed.returncode == 0, completed.stderr
        assert study.source_hash() == progress['source_sha256']
        assert source.read_bytes() == source_bytes
        for dependency in dependencies:
            assert digest(pathlib.Path(dependency['path'])) == dependency['sha256']
        data = dict(source_commit=commit, source_sha256=progress['source_sha256'],
                    example=example, example_sha256=digest(source),
                    binary=str(binary.relative_to(root)), binary_sha256=digest(binary),
                    command=command, rustc=compiler, source_preflight=prepared,
                    completed_cargo_dependencies=dependencies,
                    library_source_commit=progress['source_commit'],
                    library_build_command=progress['command'],
                    library_build_environment=progress['environment'],
                    environment={'RUSTUP_TOOLCHAIN': 'stable'},
                    builder_sha256=digest(pathlib.Path(__file__)),
                    started_at_unix=started, finished_at_unix=time.time(), build_exit_code=0,
                    timing='Smoke correctness only. Unoptimized example code with optimized completed '
                           'Cargo dependencies. Excluded from all speed rankings and acceptance.')
        with receipt.open('x') as out:
            json.dump(data, out, indent=2)
            out.write('\n')
        print(json.dumps(dict(example=example, smoke_binary=str(binary))), flush=True)


if __name__ == '__main__':
    main()
